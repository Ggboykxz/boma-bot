# v0.4.0 architecture proposal — `crates/api/src/authroutes.rs`

Lens: architecture (route structure: sign-in, sign-up, reset, forgot-password, sign-out).
Proposal only — no code changed. **CONFIRMED** = verified by reading this worktree in this
session; **GUESS** = needs an oracle/spec check before implementation.

## As-is (CONFIRMED)

- Five free-function POST handlers are mounted one-by-one in `router.rs:36-55`. There is no
  auth sub-router, shared extractor or route-layer; the grouping exists only in the file name.
- `over_limit` is copied into four handlers with per-endpoint constants
  (`authroutes.rs:17-52`, calls at 141/178/210/259). Rate-limit-before-parse is therefore a
  convention, not a guarantee: a new handler can silently skip it.
- `sign_out` is the outlier: no `ClientAddr`/`HeaderMap`/`Bytes`, no rate limit, no use case.
  Its doc comment says "revokes the session" but the body only emits a clearing cookie
  (`authroutes.rs:281-289`).
- Cookie attachment is silently fallible twice: `if let Ok(HeaderValue::from_str(&cookie))`
  (`authroutes.rs:285`, `306`). A 200 with `{"user":...}` but no `Set-Cookie` is a broken login.
- All four request DTOs derive `Debug` with plain `String` password/token fields (lines 62-103).
- Generated OpenAPI registers only `/api/auth/capabilities` (`openapi.rs:17-49`); the five auth
  handlers are absent. `spec/openapi.yaml:107-131` instead declares one catch-all
  `/api/auth/{path}` (GET+POST, 64 KiB cap, "authentication library surface").
- Auth POSTs use the global body limit (`router.rs:135`) with no per-route cap; `parse()` maps
  every JSON error to a generic 400 (`authroutes.rs:312-318`) — good, no internal leak.
- Good invariants worth keeping: central client-IP resolution with trusted proxies (`:39`),
  strict `deny_unknown_fields`, constant reply for unknown emails on forgot-password,
  no token in the response body (`:343-355`).

## Concrete upgrades (ranked)

**U1 — Resolve the route-shape contract first (DEVIATION gate).** The spec models auth as a
single wildcard `/api/auth/{path}` with GET+POST; the rebuild serves five explicit POSTs and no
GET. Decide explicitly: (a) keep explicit routes and amend `spec/openapi.yaml` to enumerate
them, or (b) add a catch-all fallback after the explicit routes for spec shape/method
coverage. Explicit routes must stay first so the wildcard cannot shadow them. Under (b),
unknown paths get 404/405, never a catch-all 200.
*Risk:* a wildcard accepts unintended paths and can hide routing mistakes; amending the spec is
an externally visible contract change requiring approval and a `docs/PARITY.md` entry. GUESS:
verify whether the original's GET side (session/callback/verification) is actually exercised by
the UI before promising it.

**U2 — Put the auth routes in the generated OpenAPI document.** Derive `utoipa::ToSchema` on
`SignInRequest`, `SignUpRequest`, `ForgotPasswordRequest`, `ResetPasswordRequest`,
`SessionReply`, `UserView`; add `#[utoipa::path]` with status codes and register the five paths
and schemas in `ApiDoc` (`openapi.rs:26-42`). Clients are generated from this document, so today
they cannot see sign-in/up/reset.
*Risk:* generated schemas will differ from the spec's permissive `additionalProperties: true`
object — the contract test will fail until U1's decision is reflected in `spec/openapi.yaml`.
Derives add a dependency surface; keep wire field names identical (camelCase).

**U3 — Replace duplicated handler plumbing with one typed pipeline.** Introduce an
`AuthInput<T>` extractor (or an `auth_guard` route-layer) that owns IP resolution, the rate-limit
check and `parse()`, driven by a `RateLimitPolicy { bucket, window, max }` table. Handlers become
usecase calls only, and "limit before parse" becomes impossible to skip.
*Risk:* axum requires the body extractor last; a custom rejection must map back to `ApiError`
and keep the same 429/400 envelopes. Extractor ordering is easy to get subtly wrong — land this
only with router-level tests for all five routes, and keep handlers explicit if the abstraction
costs more than the four duplicated calls it removes.

**U4 — Make secrets opaque and identity canonical at the boundary.** Wrap password/token fields
in `secrecy::SecretString` (Debug redacted) and trim/lowercase email once in `parse`/`AuthInput`
instead of relying on each usecase.
*Risk:* public DTO field types change and existing tests compare `body.email` to `&str`; add
`.expose_secret()` at the usecase edge. Canonicalising email can change behaviour for mixed-case
addresses — check the oracle and record any externally visible change as a DEVIATION before
shipping.

**U5 — Make session responses infallible and align sign-out with its contract.** Add a
`SessionResponse` `IntoResponse` newtype that builds the cookie once via a typed builder and
returns a controlled 500 if the header cannot be built; assert in tests that every success
carries exactly one `Set-Cookie`. Apply it to `session_response` and `sign_out` so the silent
`if let Ok` disappears. Then either implement server-side revocation in `sign_out` via a usecase
port, or correct the doc comment if the original is deliberately client-only.
*Risk:* server-side revocation needs a store port and changes behaviour (DEVIATION if the
original only clears the cookie) — GUESS until compared with the oracle. The new 500 path must
use the existing error envelope, not a raw string.

## Sequencing / verification

U1 (decision) → U2 (contract) → U3 (structure) → U4/U5 (hardening). After each step:
`cargo fmt --check`, `cargo clippy -- -D warnings`, the auth unit tests, and the OpenAPI contract
test; for U5 add a router test asserting `Set-Cookie` on sign-in/up/reset and its absence on
sign-out. Keep REQ-040/REQ-041 test names intact.
