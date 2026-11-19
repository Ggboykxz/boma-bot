# Worker #280 — `crates/infra/src/connectors.rs` — LENS: reliability — v0.4.0 proposal

- Module: `crates/infra/src/connectors.rs` (281 lines, REQ-032); lens: reliability; mode: READ-ONLY proposal.
- CONFIRMED (read this session): the full file above; line refs are from it. Proposals, constants and risks are GUESS (design inference; not implemented, not tested).
- No wire/API/data-format change is proposed by default; anything externally visible is flagged as a DEVIATION below and needs `docs/PARITY.md` plus operator approval before shipping.

## Scope note — mapping the brief onto this module

This is the outbound connector executor: two GET tools (`rss.read`, `http-json.get`) behind `check_outbound_url`. All three named failure modes map directly here: connection failures hit `HttpConnectorRunner::fetch` (L42-58) and the inline `http-json` send (L110-122); credential rotation is the per-call `Option<&SecretString>` bearer (L68, L102-116); retry storms are currently absent — the gap is that *every* failure collapses to one `AppError::Unavailable`, so any caller-side retry is blind and unbounded.

## Failure modes

### FM1 — Connection failures (CONFIRMED)

- **C1 (HIGH):** both paths buffer the *whole* body before checking the cap. `fetch` does `response.bytes().await` (L56) before the 512 KiB check at L83; `http-json` does the same at L130 before the 16 KiB check at L131. A hostile/huge upstream is fully allocated first — memory-exhaustion / OOM risk, worsened by parallel tool calls.
- **C2 (HIGH):** `fetch` never inspects `response.status()` (L50-58). A 404/500 HTML error page is parsed as a feed and returned as a `200`-shaped success JSON with empty/partial entries — silent data-quality failure; the caller cannot tell "empty feed" from "endpoint down".
- **C3 (MED):** `send()` errors (DNS, refused, TLS, timeout) all map to one `AppError::Unavailable` with a single `warn!` (L46-49); `bytes().await` errors map to `Unavailable` with *no* log (L56, L130). Operators cannot distinguish a dead host from a reset mid-body.
- **C4 (MED):** one flat 30s timeout (L34) covers connect + headers + body. An unreachable host occupies a task for the full 30s; no connect timeout, no body-read deadline. No concurrency bound, so N parallel calls to one dead host tie up N tasks (a self-inflicted resource storm).
- **C5 (LOW, GUESS):** `check_outbound_url` (L43, L107) resolves/validates, then `reqwest` resolves again at send — a DNS-rebinding window. Correctness/security crossover; noted, not fixed here.

### FM2 — Credential rotation (CONFIRMED)

- **A1 (HIGH):** on 401/403 the `http-json` tool returns `Ok` with `"status": 401` in the body (L123, L140-144). There is no signal to the caller that the bearer is stale, so rotation is never triggered; the failure is indistinguishable from a normal answer.
- **A2 (MED):** `format!("Bearer {}", token.expose_secret())` (L114-116) allocates a *plaintext* copy of the secret outside `SecretString`'s protected buffer; it lives in the `HeaderValue` and allocator until drop. No empty-token check — an empty `apiKey` yields `Bearer ` and a silent 401.
- **A3 (MED):** rotation contract is implicit. Each `run` receives a fresh `&SecretString` (good — no in-run credential cache), but nothing documents that the *caller* must refresh per call, and there is no metric/event on auth failure to drive a refresh upstream.
- **A4 (LOW):** no distinguishing log for auth failure vs. transport failure; `tracing::warn!(%error, ...)` (L119-120) is the only observability, and reqwest errors here rarely carry status.

### FM3 — Retry storms (CONFIRMED structure, GUESS behavior)

- **R1 (HIGH):** all causes collapse to `AppError::Unavailable` (C3). A caller retrying on that variant will retry 4xx, 5xx, DNS and timeout alike — selective retry is impossible. With a 30s timeout, a broad retry policy multiplies load 2-4x against an already-struggling host.
- **R2 (HIGH):** no circuit breaker, backoff, jitter, concurrency limit or per-host rate limit anywhere. A burst of connector calls to one failing endpoint is a thundering herd; retries synchronize because there is no jitter.
- **R3 (MED):** replaying a 401 from rotation (A1) just resends the stale token; retrying is useless there. Both tools are GET (idempotent, so retry is *safe* in principle) but carry no request id for upstream dedupe.
- **R4 (MED):** `429` is treated as an ordinary 200-shaped answer (L123); no `Retry-After` parsing means the service's own backoff signal is ignored.

## Concrete hardening (prioritized, GUESS — not implemented)

- **H1 (C1, P0):** stream the body with `bytes_stream()` and accumulate under a cumulative cap (`take`/`try_fold`), aborting past 16 KiB (json) / 512 KiB (feed) instead of buffering first. Keep the byte-identical cap error strings (L84, L132) to avoid a DEVIATION. *Risks:* off-by-one at the cap must be pinned by tests; no truncation of a valid 512 KiB feed.
- **H2 (C2, A1, P0):** classify status. Treat non-2xx for `rss.read` as a transport-style failure; expose 401/403 as a distinct internal outcome so rotation can be triggered. Prefer a new *internal* typed signal + log/metric before changing the JSON body — any new `"authExpired"` field or status-based error is a wire change → DEVIATION, `docs/PARITY.md` + approval. *Risks:* changing current "always Ok + status field" behavior breaks callers that rely on it.
- **H3 (A2, A3, P0):** reject empty tokens; build the header without a plaintext `format!` copy where feasible (`SecretString` + `HeaderValue::from_bytes`), never log the token, and emit an auth-failure metric/event. Document the rotation contract in rustdoc. *Risks:* header construction APIs need a checked `docs.rs` read; metric names/cardinality must be fixed.
- **H4 (C3, C4, R1, P1):** split connect timeout (e.g. 5s, GUESS) from the 30s total; add a bounded retry (max 2) with exponential backoff + jitter for transient classes only — connection reset/timeout, 429, 5xx — never 401/403/4xx; keep total within the request budget. *Risks:* added latency, new dependency (`backoff` or hand-rolled), jitter constants need confirmation; must not replay stale tokens (H2 gate).
- **H5 (R2, R4, P1):** add a per-host circuit breaker (open after N consecutive failures, half-open probe) plus a global/per-host concurrency semaphore; parse `Retry-After` on 429. *Risks:* breaker state must be keyed by host so one bad host does not penalize another; global limiter needs careful bound sizing; adds shared mutable state and lock-across-await discipline.
- **H6 (C3, A4, P2):** metrics for call count, failure class, latency histogram, status buckets and breaker trips; structured `tracing` with connector id and **redacted** URL (no query secrets/PII). Log the `bytes().await` failure path currently missing at L56/L130. *Risks:* hot-path logging needs level checks; redaction must cover query tokens.
- **H7 (tests, P2):** wiremock-based tests (default pick per AGENTS.md) for connection refused, timeout, 401/403, 429 + `Retry-After`, 500-then-200 retry, oversized body cut-off, malformed feed, and the Atom/RSS parsers. REQ-032 in every test name per project rules. *Risks:* wiremock is a dev-dependency — verify `Cargo.toml`; proptest for `clean_text`/`parse_feed` if not already covered.

## Risks & mitigations

- Largest risk is DEVIATION: H2 changes externally visible status/error behavior. Land internal signals (logs/metrics) first; only change the wire shape after `docs/PARITY.md` + approval.
- Retry (H4) and breaker (H5) add dependencies and shared state to a deliberately simple module; keep constants small and justify in an ADR if the single-maintainer simplicity criterion is challenged.
- H1 byte caps must not alter the accepted feed/JSON sizes or error text; those tests are the guard and land first.

## Acceptance tests (suggested names)

- `req_032_oversized_body_is_capped_while_streaming` (H1)
- `req_032_non_success_status_does_not_look_like_a_feed` (H2)
- `req_032_unauthorized_marks_auth_failure_not_success` (H2/A1)
- `req_032_empty_credential_is_rejected` (H3)
- `req_032_transient_failure_retries_with_backoff_and_jitter` (H4)
- `req_032_unauthorized_is_never_retried` (H2/H4)
- `req_032_circuit_breaker_opens_per_host` (H5)
- `req_032_retry_after_is_honored` (H5)
- `req_032_body_read_failure_is_logged` (H6)

## Decisions needed (GUESS → operator/spec)

1. Should non-2xx feed responses become a hard error, or keep the current best-effort silent shape (H2)?
2. Is a wire-level auth signal (`"authExpired"`) acceptable, or must it stay internal (H2)?
3. Are connect=5s / total=30s / max 2 retries with jitter the agreed budgets (H4)?
4. Is a per-host circuit breaker + semaphore in scope for v0.4.0, or deferred (H5)?
