# 0122 — `crates/api/src/auth.rs` reliability proposal (v0.4.0)

Module: `crates/api/src/auth.rs`
Lens: reliability
Spec anchor: REQ-040 (auth methods), REQ-041 (sessions/trusted origins), REQ-043 (tenant
isolation), REQ-066 (local-first), D-08 (assert auth config, don't inherit defaults).

Status of claims: "Current behavior" is CONFIRMED by reading this session — `auth.rs`, its callers
(`state.rs:83-94`, `rpc.rs:318-321`), `usecases::{resolve_session, resolve_actor, ensure_local_owner}`,
`rpc::requested_space`, `error.rs`, `crypto::TokenGenerator::hash`, `sessions::find_by_token_hash`,
`config.rs`, and `spec/05-known-defects.md`. Proposals and risks are GUESS (design inference, not
implemented or tested).

## Current behavior (CONFIRMED)

- `session_cookie` reads only the first `Cookie` header (`headers.get`), requires the whole header
  to be UTF-8 (`to_str().ok()?`), splits on `;`/`=`, trims, and returns the **first** `session=`.
  No duplicate handling, length cap, or quoted-value handling. Absence → `None`.
- `session_cookie_header` sets `session=<token>; Path=/; HttpOnly; SameSite=Lax; Max-Age=<i64>`
  plus optional `Secure`. `cleared_session_cookie_header` uses `Max-Age=0`, no past `Expires`.
- `authenticate`: with a cookie it requires `resolve_session` → `resolve_actor`; with no cookie and
  `local_first` it calls `ensure_local_owner` (unauthenticated, mutating); otherwise 401. TTL is a
  dead parameter: `let _ = session_ttl_seconds;`. Store errors propagate via `?` and the
  `AppError->ApiError` conversion maps `Unavailable` to 503 (fail-closed, grep-confirmed).
- Expiry lives only in `resolve_session`: SHA-256 token hash → exact DB lookup → a second
  constant-time `ct_eq` in `sessions.rs` → lexical compare `expires_at.as_str() <= now.as_str()` →
  `None` for expired/unknown/deactivated user. The returned `_expires_at` is discarded.
- `local_first` is gated at config construction: `with_local_first` rejects a non-loopback bind
  (CONFIRMED, `config.rs:217-221` + tests 410-455).

## Failure modes

1. **Cookie validation.** One non-UTF-8 byte in *any* cookie discards the entire header, so a
   sibling cookie from another app yields a spurious 401 for a valid session. Only the first
   `Cookie` header is read; a second one carrying the real session is ignored. Duplicate
   `session=` values are resolved first-wins, so a stale or injected cookie can shadow a fresh one
   and cause persistent 401 (availability, not disclosure — `name == "session"` is not a secret).
   No token length cap bounds the hashing work or the header size.
2. **Session expiry.** DB expiry and the sign-in cookie's `Max-Age` are independent sources of
   truth that can drift; the unused TTL parameter shows an intended server-side cap that never
   landed. Expiry is a lexical compare of RFC-3339 strings, correct only while both sides share one
   canonical format/precision/timezone — a format change silently alters expiry boundaries.
   Expired vs unknown vs deactivated are collapsed (good for anti-enumeration) but unobservable,
   so an expiry-storm incident has no signal. No sliding renewal because `_expires_at` is dropped.
3. **Constant-time checks.** None live in this module; the guarantee is delegated to
   `sessions::find_by_token_hash` (`subtle::ct_eq`) over a fast SHA-256 hash. That is sound only
   because `tokens.generate()` is 256-bit random (CONFIRMED) — swapping in a low-entropy token or
   dropping the `ct_eq` in a refactor would break it with no local test to catch it. The DB exact
   match already narrows the set, so `ct_eq` is defence-in-depth, not the primary lookup.
4. **Fail-open.** The only credential-free path is `local_first`: no cookie → implicit owner, and
   `ensure_local_owner` *creates* user/org/space on first request. The loopback gate is the sole
   barrier; a reverse proxy, container port-map, or SSH forward to a loopback bind turns that into
   unauthenticated admin with no code change. `resolve_actor` correctly fails closed (a space the
   caller is not in → 404), and a valid session with zero spaces → 404, which can strand a
   legitimate new user. Store outages fail closed to 503, never anonymous.

## Concrete hardening for v0.4.0 (GUESS)

1. Replace `headers.get` with `get_all` and iterate, skipping non-UTF-8/sibling cookies individually
   instead of abandoning the header. Reject when more than one `session=` is present (401) rather
   than first-wins, and cap token length before hashing (e.g. 512 B). *Risk:* rejecting duplicates
   may break clients carrying a stale + fresh cookie; confirm observed browser behaviour and record
   any which-cookie-wins change in `docs/PARITY.md` (DEVIATION gate).
2. Remove the dead `session_ttl_seconds` parameter and its call-site plumbing, or wire it to a
   single authoritative expiry check; prefer removal for v0.4.0 (internal crate signature, no
   external DEVIATION). Add a test pinning that cookie `Max-Age` and DB `expires_at` agree at
   sign-in (owner: `accounts.rs`, noted here). *Risk:* silently dropping the parameter without a
   decision leaves the drift bug unresolved.
3. Replace the lexical expiry compare with a parsed timestamp (`time`/`chrono`) in the use case,
   or add a regression test asserting canonical RFC-3339 output if the string compare is kept.
   Emit an internal *counter* for expired vs unknown vs deactivated (never client-visible, never
   the token value) so expiry storms are diagnosable. *Risk:* parsing adds a dependency and a
   failure mode; run both formats through the test before switching.
4. Pin the constant-time invariant with a code-level test in `infra` asserting the session lookup
   still calls `constant_time_eq`, plus a doc comment/ADR that tokens must stay high-entropy.
   *Risk:* none behaviourally; timing-based tests are flaky — do not write a wall-clock assertion.
5. Keep `local_first` fail-closed in depth: keep the config gate, add a loud startup log/metric when
   it is active, and document that a loopback bind must never be proxied/forwarded to a public
   interface. Add a regression test that a DB outage with a valid token returns 503 and never 200.
   *Risk:* the operational constraint cannot be enforced inside this module; the unauthenticated
   `ensure_local_owner` mutation could be moved to startup, but that changes behaviour and needs
   an ADR.

## Constraints

- Preserve the uniform 401/404 responses (anti-enumeration, REQ-043); diagnostics must stay
  internal-only, with no token, secret, or PII in logs.
- Keep `HttpOnly`/`SameSite` cookie attributes and the loopback-only local-first gate intact; both
  are parity/security invariant. Any observable auth-behaviour change awaits approval per
  `docs/PARITY.md`.
- D-08: convert the "assert auth configuration" recommendation into a test (session lifetime and
  cookie attributes) rather than relying on defaults.
