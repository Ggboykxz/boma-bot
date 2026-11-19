# 0126 — `crates/api/src/authroutes.rs` — reliability lens (v0.4.0)

Target: `crates/api/src/authroutes.rs` (365 lines, the `/api/auth/*` surface). Dependencies read this
session: `auth.rs`, `client.rs`, `error.rs`, `router.rs`, `config.rs`, `crates/app/src/{accounts.rs,
usecases.rs,ports.rs}`, `crates/infra/src/{crypto.rs,stores/sessions.rs,stores/rate_limits.rs}`.
Spec anchors: REQ-040/041 (auth + cookie sessions), REQ-066 (local-first), D-01/D-04 (rate limit,
spoof-proof client IP), D-08 (cookie attributes). CONFIRMED = read this session; GUESS = inferred,
needs a test. Read-only review; this document is the only write.

## Current behaviour (read, not run)

- `over_limit()` (lines 31–52) runs a shared-counter hit before body parsing for all four write
  endpoints; key is `auth:{bucket}:{client_ip}` — **IP only**, no account dimension (CONFIRMED).
- Limit check is `decision.count > max`; fixed, wall-clock-aligned buckets, no `Retry-After` header
  and no sliding window (CONFIRMED: `authroutes.rs` 45–50, `stores/rate_limits.rs` 41–66).
- Limiter failure is fail-closed: `Err -> ApiError(Unavailable)` -> 503 for every auth call (CONFIRMED).
- `forgot_password` answers a byte-identical `200 {"ok":true}` for malformed/unknown/existing
  addresses (CONFIRMED); `sign_in` uses a dummy hash + uniform `InvalidCredentials` (accounts.rs
  132–158). `sign_up` returns `That already exists.` on a duplicate address (`error.rs` conflict,
  CONFIRMED) — a deliberate enumeration answer.
- Tokens: 32-byte CSPRNG, base64url, SHA-256 at rest, constant-time compare (crypto.rs 16–19,
  100–114; sessions.rs 47–62) (CONFIRMED) — structurally strong.
- `sign_out()` (282–289) only clears the cookie; it does **not** revoke the server session, although
  `SessionStore::delete`/`delete_for_user` exist (ports.rs 406/415) (CONFIRMED).
- Default session TTL is 30 days (config.rs 86); `authenticate()` explicitly ignores its
  `session_ttl_seconds` argument (`let _ = session_ttl_seconds;`, auth.rs 86) — no renewal, no idle
  expiry (CONFIRMED).
- `reset_password` atomically consumes the link, revokes all links, deletes all sessions and grants
  a fresh session (accounts.rs 284–335) (CONFIRMED).
- With `BOMA_AUTH_PRINT_RESET_LINK=true`, the full one-time link and user id are written to process
  logs; with it off, no delivery channel exists (accounts.rs 248–258) (CONFIRMED).
- `session_response()`/`sign_out()` silently drop the `Set-Cookie` header if `HeaderValue::from_str`
  fails (`if let Ok`, authroutes.rs 285/306) (CONFIRMED).

## Failure modes

### Enumeration
1. **Direct, by design (high).** `sign_up` Conflict confirms a registered address; `forgot_password`
   and `sign_in` are uniform, so the sign-up difference is the leak (CONFIRMED).
2. **Residual timing (medium, GUESS until measured).** For an existing account `request_password_reset`
   mints + inserts a token and writes an audit row; the unknown path only hashes the email; the code
   comment concedes the gap (accounts.rs 228–231). A deactivated sign-in skips the dummy hash while a
   wrong password runs Argon2 (accounts.rs 143–155) — measurably faster, so deactivated accounts are
   enumerable (CONFIRMED code, GUESS impact).

### Rate limiting
3. **Per-IP only (high).** Distributed stuffing (many IPs, one account) is unthrottled; conversely a
   NAT/proxy shares one bucket and locks out legitimate users (CONFIRMED key, GUESS impact).
4. **Fixed-window boundary burst (medium).** Up to 2x attempts can land across a window edge inside
   seconds (CONFIRMED by store semantics).
5. **No backoff signal (medium).** `429` carries no `Retry-After`/`X-RateLimit-*`, so clients retry
   blindly and amplify load (CONFIRMED).
6. **Hot shared counter (medium).** Every hit is a transaction with an `INSERT ... ON CONFLICT` plus
   a `DELETE ... WHERE bucket < bucket-24` cleanup on the same table (rate_limits.rs 46–66) — write
   amplification and row contention under a storm, and all auth fails 503 if that store hiccups
   (CONFIRMED code, GUESS load impact).
7. **Proxy misconfiguration (medium).** Spoof resistance depends entirely on `trusted_proxies` being
   exact; wrong config means either universal lockout (loopback bucket) or bypass (CONFIRMED logic).

### Token handling
8. **Sign-out leaves a live credential (high).** The cookie is cleared but the 30-day session row is
   not deleted: a stolen/old cookie authenticates until absolute expiry (CONFIRMED).
9. **No rotation, no idle timeout (medium).** Each sign-in mints a new session without retiring the
   old one; `authenticate` never slides expiry, so sessions accumulate and live the full 30 days
   (CONFIRMED).
10. **Recovery link in logs / no default delivery (high).** The opt-in flag prints a live takeover
    token to general logs (any log reader = account takeover); with it off, forgot-password returns
    success but delivers nothing (CONFIRMED).
11. **Silent cookie failure (low).** `if let Ok` drops a malformed `Set-Cookie`, returning a 200 that
    reports success yet cannot authenticate (CONFIRMED; base64url makes it unlikely — GUESS).

### Session fixation
12. **Fixation proper is blocked** — the server always generates the token on sign-in/up, so an
    attacker cannot pin one (CONFIRMED). Residual risk: the long-lived token surviving sign-out
    (8) and no rotation when an authenticated caller signs in again (9) (CONFIRMED, GUESS exploit).
13. **Cookie hardening gaps (low).** `SameSite=Lax` (not `Strict`), no `__Host-` prefix or `Domain`
    pinning; `secure_cookies=false` sends the cookie over HTTP (auth.rs 36–52) (CONFIRMED; Lax blocks
    cross-site POST — GUESS residual risk).

## Concrete hardening (v0.4.0)

- **Revoke on sign-out:** make `sign_out` take `HeaderMap`, read `session_cookie`, hash it and call a
  new `revoke_session(app, &token)` usecase; always clear the cookie even on failure and log/metric it.
- **Rotation + bounded lifetime:** on sign-in/up retire the presented session; add idle-expiry/sliding
  renewal in `authenticate` (use the ignored TTL) and cut the default TTL (30 d -> 24 h). Configurable.
- **Per-account limiter:** add a second bucket keyed by a hash of the normalized email/account id on
  `sign_in`, `forgot_password` and `reset_password`, alongside the IP bucket; refuse if either trips.
  Tighten trusted-proxy validation and document the deployment requirement.
- **Backoff + smoother window:** emit `Retry-After` (seconds to rollover) and `X-RateLimit-Remaining`
  on `429`/success; move cleanup off the hot path (periodic job) and consider a token-bucket or
  sliding-window counter to kill boundary bursts.
- **Timing equalization:** run one dummy Argon2 verify + one dummy audit/insert-shaped write on every
  failure branch (missing, deactivated, bad password); for `sign_up` duplicates, keep the wire
  `Conflict` but add uniform delay + tighter limiter. Any uniform response is an externally visible
  change — record as DEVIATION in `docs/PARITY.md` and wait for approval before shipping.
- **Recovery channel:** stop printing live links to general logs by default; deliver via a dedicated
  sink or a short-lived reference, and redact `recovery_link` from `tracing`. If spec mandates it,
  gate printing to a sink with rotation/retention limits.
- **Cookie hardening:** add `__Host-` prefix (forces `Secure`/`Path=/`), consider `SameSite=Strict`,
  and replace silent `if let Ok` with an error/metric so a dropped `Set-Cookie` cannot look like
  success. Add boundary length checks on email/name before hashing.
- **Observability:** add request-id `tracing` spans and counters for sign-in, limiter trips, resets and
  cookie failures — IP/email truncated, never passwords or tokens (AGENTS.md: no PII/secrets in logs).

## Risks / trade-offs

- Revocation needs a new store method + port + migrations for all adapters (infra, testkit memory);
  sign-out gains a DB dependency and can fail — mitigate with always-clear-cookie and best-effort log.
- Shorter TTL / sliding renewal shortens stolen-cookie usefulness but changes session lifetime
  (externally visible only for very long idle users): ADR + `docs/PARITY.md` entry, operator approval.
- Per-account bucketing needs the email parsed before the check; malformed bodies must still be limiter-
  gated by IP. Hashing the key avoids storing addresses in the counter table (PII).
- Uniform enumeration responses may break golden fixtures that expect `Conflict` on duplicate sign-up;
  verify against `spec/golden` before changing and gate behind approval.
- Removing log-printed links can make recovery unusable in deployments that relied on them — require a
  working delivery channel or explicit opt-in flag first, and document the migration.
- `SameSite=Strict`/`__Host-` may affect cross-subdomain/embedded flows; check UI origin and CORS (GUESS impact).
- Rate-limit hot-path changes touch shared infra used by other public endpoints (`public.rs`) — keep
  behavior backward-compatible and benchmark contention before/after.
