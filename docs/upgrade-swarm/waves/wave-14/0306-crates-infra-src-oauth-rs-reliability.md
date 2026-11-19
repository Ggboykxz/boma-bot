# 0306 — crates/infra/src/oauth.rs — reliability (v0.4.0)

Worker #306 · module `crates/infra/src/oauth.rs` · lens: reliability · READ-ONLY review of
`rebuild-workspace/.worktrees/before-m5` at v0.3.0-era source. Proposals only; no code in this
change. Scope note: `OAuthTokenClient` is small (mint S256 PKCE pair, POST a form-encoded code
exchange). State/CSRF and the session lifecycle actually live one layer up in
`crates/app/src/usecases.rs` (`mcp_oauth_start`/`complete`, ~L680–836) and
`crates/infra/src/stores/mcp.rs` (~L401–504); I cover them as boundaries because the reliability
of this flow depends on them. Line refs are against this worktree. Claims are tagged CONFIRMED
(read this session) or GUESS (inferred).

## What it already does right (keep, don't re-propose)

- 30 s timeout, `redirect(Policy::none())` — SSRF-via-redirect closed (CONFIRMED, oauth.rs L26–30).
- 64 KiB response cap; non-JSON failure bodies wrapped into a stable refusal (L71–80).
- PKCE: 32-byte CSPRNG via `getrandom`, S256 challenge, verifier held as `SecretString` (L37–43).
- State: 32-byte CSPRNG (`crypto.rs` `TOKEN_BYTES = 32`), 10-min TTL, deleted atomically with a
  `server_id`+`state`+expiry match — single-shot binding holds (CONFIRMED, usecases L703–709,
  stores L472–482). ADR-021 documents the flow; keep its guard and redirect semantics.

## Failure modes → concrete hardening

### F1. State / CSRF — sound core, two reliability gaps (boundary)

- **Gap A — one session per server.** `store_oauth_session` uses `ON CONFLICT (server_id) DO
  UPDATE` (CONFIRMED, stores L427–433): a second `start` (double-click, two tabs, retry after a
  flake) silently clobbers the first state, so the first callback fails "expired or not
  recognized". P1. Keep a short bounded history (≤3 states/server) keyed by `(server_id,state)`,
  each with its own expiry, deleting on match; P2. at minimum make `start` idempotent for an
  identical in-flight request. Risk: extra rows + migration/RLS; a slightly wider state window —
  mitigated by retaining per-state single-shot DELETE and a hard row cap.
- **Gap B — coarse callback error.** Unknown / expired / already-used state all return one
  message (CONFIRMED, usecases L770–773). P3. Log a precise reason (request id) server-side and
  return distinct-but-safe client hints ("start a new sign-in") so members aren't stuck pasting
  the same stale callback. Risk: an online oracle for session state — state is high-entropy, so
  enumeration is infeasible; keep the client text non-specific.

### F2. Token refresh — the largest hole (GUESS→confirm by grep: no `refresh_token` anywhere in
`crates/`)

- CONFIRMED: `exchange_grant` only sends `grant_type=authorization_code`; `expires_in` is ignored,
  no expiry is stored on the bearer, no 401 handling in the MCP transport (usecases L801–825).
  A server stays `connected` forever; when the access token expires, calls 401 indefinitely until
  the member re-runs OAuth. P4. Parse `expires_in`/`token_type`, store an absolute expiry,
  mark `expired` proactively. P5. Persist `refresh_token` sealed under the space DEK and add a
  `grant_type=refresh_token` path (reuse the exchange helper). P6. On transport 401, do **one**
  serialized refresh (per-server lock/single-flight to avoid rotation races), retry the call once,
  else mark `expired` and surface re-auth. P7. Rotate the refresh token when the AS issues a new
  one, keep the old otherwise. Risks: rotation thundering herd (per-server mutex); refresh tokens
  are long-lived secrets (see F4); ASes without refresh support must degrade to today's re-auth
  with no regression; this is externally visible (new auth state/field) → **DEVIATION**, needs an
  ADR + docs/PARITY.md note and REQ-033-named tests before shipping.

### F3. Callback / token-endpoint errors — misclassified and non-resilient

- CONFIRMED: every upstream failure (transport error, 4xx/5xx, malformed body) maps to a single
  `AppError::Rule(DomainError::InvalidName(..))` (L66–89) → HTTP 400. A 503/timeout from the AS is
  reported as a client validation error; `invalid_grant` and `temporarily_unavailable` are
  indistinguishable. CONFIRMED: the one-shot session is consumed *before* the exchange
  (usecases L764–773, stores L472–482), so any transient blip burns the state and forces a full
  restart. P8. Add a typed `OAuthError::{Transient,Denied,Protocol}` in app/domain mapped from
  RFC 6749 `error` codes (transient: 429/5xx/timeouts/`server_error`/`temporarily_unavailable`);
  map Transient→`Unavailable` so HTTP status is honest. P9. Retry only transient failures inside
  `exchange_grant`: 2–3 bounded attempts, exponential backoff + jitter, honour `Retry-After`
  (capped) on 429/503, within the 30 s budget; never retry 4xx protocol errors. P10. Keep the DB
  delete single-shot, but buffer the already-opened verifier in memory for the life of the
  in-process exchange so a retry needs no new state — do not relax CSRF. P11. Treat a 200 with a
  non-JSON body as a protocol error (today it returns `Ok({"error": "...status 200"})`, L77–80).
  P12. Check the response `Content-Type` before parsing. Risks: retry amplification on a down AS
  (bounded + jitter + `Retry-After`); buffering widens the replay window only within one request;
  new error type touches app/domain/API (DEVIATION if client-visible status changes) → traceable
  REQ-033 tests and parity note.

### F4. Secret leakage

- CONFIRMED: the port takes `&[(String,String)]` (ports.rs L2821–2825); `client_secret`, `code`,
  and the exposed `code_verifier` are plain `String`s (usecases L779–794). `exchange_grant` builds
  one joined `String` via `format!`/`join` (L51–55) → several zeroization-free copies; reqwest
  adds another. CONFIRMED: `error_description`/`error` are taken verbatim from the AS and
  surfaced (L81–89) — up to 64 KiB of AS-controlled text (log injection/newlines, reflection).
  CONFIRMED: no URL/form logging in this module — keep `warn!(%error)` as-is. P13. Move to a typed
  secret-bearing form (`SecretString` fields), expose only at serialization, and write into one
  reused/zeroized buffer instead of `join`. P14. Clamp (≤256 B) and strip control chars from AS
  descriptions; return a fixed client message + correlation id, log sanitized detail only.
  P15. Reject embedded URL credentials at this layer too; HTTPS is mandatory except where the
  documented per-space `allow_private` exception permits plain HTTP for LAN (ADR-021) — pass an
  explicit `allow_insecure` flag rather than trusting callers blindly. Risks: reqwest may still
  allocate a plain body internally (document residual exposure); P15 could regress LAN OAuth
  without the flag, so wire it from the caller's exception, not a blanket check.

## Explicit non-goals / rejected

- Do not weaken single-shot state, the 64 KiB cap, `Policy::none()`, or the 30 s timeout to "fix"
  failures — those are the security spine. Do not log secrets to ease debugging. Do not retry
  protocol/authentication errors. Keep all public interface names (spec compatibility).

## Cross-cutting risks and rollout (v0.4.0)

- Wave A (cheap, internal): P3, P5-logging-sanitisation (P14), P11, P12, P1 history migration.
- Wave B (contract-level, gated): P8–P10 retry/error type, then P4–P7 refresh — each behind an ADR
  and docs/PARITY.md entry; update testkit `scripts.rs` mock and all `OAuthClient` impls.
- R1. New stored refresh token widens the secret blast radius → DEK-sealed, never listed, same
  `reveal_*` discipline as bearer. R2. Port/type changes ripple into testkit + conformance tests →
  compile-time catch, keep trace names REQ-033. R3. A real third-party AS rehearsal remains the
  acceptance risk flagged in ADR-021/STATE.md; nothing here replaces that live test.

_All findings are from source read this pass; "no refresh handling" is CONFIRMED by grep across
`crates/`. No tests were run (read-only)._
