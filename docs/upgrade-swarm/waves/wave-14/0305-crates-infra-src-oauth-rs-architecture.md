# Worker #305 — `crates/infra/src/oauth.rs` — architecture (v0.4.0)

- Module: `crates/infra/src/oauth.rs` (93 lines, REQ-033 / ADR-021); lens: architecture; mode:
  READ-ONLY proposal, no code changed, no tests run.
- CONFIRMED by file read this session: `oauth.rs:1-93`; the port `crates/app/src/ports.rs:2798-2826`;
  call sites `crates/app/src/usecases.rs:678-835`; registration validation `usecases.rs:581-641` +
  `crates/domain/src/mcp.rs:166-199`; encoder `domain/src/lib.rs:113-128`; the mock
  `crates/testkit/src/scripts.rs:1011-1035`; `spec/01-requirements.md:271-277`,
  `docs/adr/021-mcp-servers.md:58-66`, `spec/05-known-defects.md:20`. Upgrades and risks are GUESS
  (design inference). No wire/API/data-format change is proposed by default; anything externally
  visible is flagged as a DEVIATION needing `docs/PARITY.md` + owner approval.

## 1. Current flow structure (CONFIRMED)

- Two-job stateless adapter behind `boma_app::ports::OAuthClient`: `pkce_pair()` (32 random bytes →
  base64url-nopad verifier kept in `SecretString`, SHA-256 → challenge) and `exchange_grant()`
  (form POST → `serde_json::Value`). Client holds only a `reqwest::Client` (30 s total timeout,
  `redirect::Policy::none()`); no state, `Debug + Clone` safe.
- Orchestration lives in the **app** layer: `mcp_oauth_start` builds the authorization URL by string
  formatting, seals the verifier in `mcp_oauth_sessions` (10 min, one attempt); `mcp_oauth_complete`
  consumes the session, builds the form, and **exposes** verifier and client_secret into plain
  `String`s (`usecases.rs:779-794`) before handing them to this port.
- Boundaries: token URL comes from the stored server record; `mcp_server_add` validates it once at
  registration with `validate_mcp_server_url` (shape only, no DNS). The adapter itself does no URL
  validation, no redirect following and no response-shape validation; success is any 2xx with
  parseable JSON, and `access_token` is checked later in the usecase.

## 2. Architecture findings (CONFIRMED unless marked)

1. **Untyped port, secrets cross it in the clear.** `&[(String, String)] -> Value` puts encoding and
   error policy in infra but form construction (with `ExposeSecret`) in app. No `TokenRequest` /
   `TokenResponse` newtypes, no `token_type`/`expires_in`/`scope`/refresh-token handling; a 2xx with
   a token-shaped-but-wrong body is only caught one layer up.
2. **SSRF guarantee is stated but not enforced at the egress point.** ADR-021:63 says the client is
   "HTTPS, no redirects, 30 s, 64 KiB cap"; only the last three hold. HTTPS/private-range checks
   live in registration-time shape validation; the POST re-reads the raw stored string with no
   re-resolution, so DNS rebinding is unguarded (contrast `net_safety.rs`, which re-checks per call
   for MCP probes). `authorization_url` is never validated at all.
3. **Hand-rolled form encoder is the wrong codec.** `domain::percent_encode` is RFC 3986 (`%20`), not
   `application/x-www-form-urlencoded` (`+`); keys are not encoded at all. Today's values (base64url
   verifier, fixed keys) are benign, but any `&`, `=`, space or unicode in a value becomes a
   wire-format compatibility risk with strict authorization servers.
4. **Error taxonomy is collapsed and AS-controlled.** Network failure, oversize body and AS refusal
   all become `AppError::Rule(DomainError::InvalidName(String))`; non-JSON failures are synthesized
   from the status. `error_description` is copied verbatim into a client-visible error, status and
   `error` code are discarded after the branch, and there is no retryable/permanent distinction —
   important because authorization codes are **single-shot** (blind retries can burn a code).
5. **Body cap is checked after full buffering** (`bytes()` then `len()`, L71-76): peak memory is
   unbounded by the 64 KiB limit; a slow chunked response is read to completion before refusal.
   Response `Content-Type` is ignored; a non-JSON success body becomes a refusal via a synthetic object.
6. **No module-local tests.** All REQ-033 evidence is indirect (testkit scripted client, `m1.rs`,
   `rpc_surface.rs`); no wiremock test pins redirect refusal, oversize bodies, non-JSON errors,
   status-vs-description precedence, or PKCE S256 properties at the adapter boundary.

## 3. Concrete upgrades for v0.4.0 (with risks)

**U1 — Type the boundary (do first, structure).**
Define `TokenRequest { code, redirect_uri, client_id, code_verifier: SecretString, client_secret:
Option<SecretString> }` and a validated `TokenResponse { access_token: SecretString, token_type,
expires_in, refresh_token, scope }` in app/ports; the client owns encoding and secret exposure and
returns a typed value. Risks: (a) the port change forces `usecases.rs`, `testkit/scripts.rs`, `m1.rs`
to move together — land behind one commit; (b) keep the exact current form bytes so wiremock and RPC
tests stay green; (c) never make a secret part of a `Debug` derive or a tracing field; (d) reject a
success response with no non-empty `access_token` in the adapter too, without changing the message.

**U2 — Close the egress-time SSRF gap (security).**
Re-validate `token_url` per exchange through `net_safety::check_outbound_url` with the server's
per-space `allow_private`, or better, have the usecase hand the adapter a `CheckedOutbound` so an
unchecked URL cannot be represented; pin the POST to the validated host/port where reqwest allows.
Also validate `authorization_url` at registration. Risks: (a) per-call DNS adds latency — agree a
short timeout with sibling #303's resolver work, do not duplicate it; (b) the scripted test client
needs a checked-URL seam, or validation stays in usecases for tests; (c) `allow_private` flows must
keep working and still audit (D-05); (d) ADR-021's claim becomes true only after this lands.

**U3 — Replace the encoder with a form codec (correctness).**
Use `serde_urlencoded` or reqwest `.form(&request)`; delete the hand-rolled `format!` and stop sharing
`percent_encode` with the form path (keep it for the authorization query string). Risks: (a) new
dependency requires `cargo add`, docs.rs check and `docs/DEPENDENCIES.md`; (b) a `%20` → `+` change is
a wire-byte change — verify with wiremock and record a DEVIATION if any golden or contract test pins
it; (c) duplicate/empty keys and non-ASCII values need explicit tests.

**U4 — Typed OAuth errors and honest observability.**
Add an `OAuthError { Transport, Refused { status, code }, Malformed, Oversized }` mapped to `AppError`,
with structured `tracing` on status/code/host but never URL query, verifier, secret or token;
truncate/sanitize AS `error_description` before it reaches a member; document "no automatic retry"
in rustdoc because codes are single-shot. Risks: (a) current refusal strings are asserted in tests —
preserve them or update with evidence; (b) `Unavailable` vs `Rule` mapping may be load-bearing for
HTTP status codes, so contract-test before changing it; (c) logging status is fine, the raw body is not.

**U5 — Stream the body cap and own the adapter tests (hardening + evidence).**
Read through `tokio::io::take(64 * 1024 + 1)` and check `Content-Type` before parsing; add local
wiremock tests (`req_033_*` in names) for: S256 challenge and fresh entropy (proptest over pairs),
redirect refusal, oversize and slow bodies, non-JSON error vs non-JSON success, status/description
precedence, and exact form bytes. Risks: (a) a strict content-type gate could refuse a tolerable AS —
GUESS: accept absent/unknown types initially and only log; (b) streaming changes when the error is
raised, not what clients see; (c) do not add a second test stack — wiremock is the default pick.

## 4. Ordering, invariants, non-goals

Order: **U1 → U2 → U3 → U4 → U5** (type boundary first, then security, then codec/errors/tests).
Invariants: no redirects, 30 s outbound timeout, 64 KiB response cap, single-shot verifier deleted at
exchange, verifier/secret/token never logged, member-driven callback URI unchanged, D-05 per-space
allow-list + audit preserved, exact REQ-033 test names kept in the trace gate. Non-goals: no
refresh-token rotation feature, no local callback listener, no new allow-list surface, no change to
the `mcp.oauth.start/complete` wire contract without owner approval.

## 5. Evidence & tests to add

- CONFIRMED — this session's reads: `oauth.rs:1-93`, `ports.rs:2798-2826`, `usecases.rs:581-641,
  678-835`, `domain/mcp.rs:166-199`, `domain/lib.rs:113-128`, `testkit/scripts.rs:1011-1035`,
  `spec/01-requirements.md:271-277`, `ADR-021:58-66`, `spec/05-known-defects.md:20`.
- GUESS — that AS deployments tolerate `%20`; that no golden pins the exact form bytes; that per-call
  resolution latency is acceptable; that `AppError::Unavailable` mapping is load-bearing in the API.
- Suggested names: `req_033_pkce_challenge_is_sha256_of_a_fresh_32_byte_verifier`,
  `req_033_the_token_endpoint_post_never_follows_a_redirect`,
  `req_033_an_oversized_token_answer_is_refused_while_streaming`,
  `req_033_form_uses_urlencoded_escaping` (U3), `req_033_unchecked_token_url_cannot_reach_exchange` (U2).

Line budget: <120. Status: proposal only; READ-ONLY worker, no code changed and no tests run.
