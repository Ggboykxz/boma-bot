# v0.4.0 architecture proposal — `crates/api/src/auth.rs` (auth middleware / principal)

Worker #121 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no code changed).

Status of claims: all "current state" statements below are CONFIRMED by direct reads of
`auth.rs`, `state.rs`, `rpc.rs`, `authcap.rs`, `error.rs`, `realtime.rs` and
`app/src/{service,usecases}.rs` in this worktree. Forward-looking items are proposals (GUESS
until implemented and tested).

## Current structure (CONFIRMED)

- `AuthContext` is a thin newtype over `boma_app::Actor { user, context: TenantContext }`
  (`auth.rs:15-18`, `service.rs:556`). It carries no session id, expiry, auth method, or
  privilege set; a real session and the REQ-066 implicit local owner yield the *same* type.
- There is no Axum middleware. `authenticate` is an ordinary async fn called ad hoc inside
  `rpc_inner` (`rpc.rs:318`) and reachable through `AppState::authenticate` (`state.rs:83-94`).
  The resolved `AuthContext` is then threaded explicitly as a parameter through ~70 procedure
  functions (`rpc.rs:733` onward) and into `realtime::subscribe` (`realtime.rs:48`).
- Session cookie parsing is hand-rolled (`auth.rs:22-28`): split on `;`, then first `=`.
  Cookie emission is manual string formatting (`auth.rs:36-53`) with a config-supplied `Secure`.
- Expiry is ignored at this layer: `resolve_session` returns `(user, expires_at)` but the expiry
  is destructured as `_expires_at` (`auth.rs:75`), and `session_ttl_seconds` is discarded with
  `let _ = session_ttl_seconds;` (`auth.rs:86`). Sliding expiry / rotation is not implemented here.
- Tenant scoping is implicit: `requested_space(headers)` reads `x-space-id` / `x-boma-space-id`
  (`rpc.rs:1864-1870`); `resolve_actor` validates membership and builds `TenantContext::Member`
  (`usecases.rs:108-138`). The "not found, never forbidden" policy is a module comment
  (`auth.rs:4-6`) enforced downstream, not typed into the principal.
- `ensure_local_owner` may perform a DB write on an otherwise-read request (`usecases.rs:160`).

## Guiding principle for v0.4.0

Make authentication a single, typed, observable boundary and make the principal a *sealed* value
that carries its own provenance and tenant scope, so handlers cannot accidentally act outside the
caller's space. Prefer composition with existing Axum/tower idioms over bespoke plumbing.

## Upgrades (3-5 concrete items)

1. **One extractor / middleware, not ad-hoc calls.** Implement `FromRequestParts<AppState>` for
   `AuthContext` (or a `tower` `auth` layer) and register it once; expose the principal via
   `Extension<AuthContext>`. Handlers receive the principal through the extractor instead of a
   positional parameter threaded through the whole procedure table. Removes ~70 manual
   `auth: AuthContext` parameters and makes the authenticated boundary uniform for RPC, realtime
   SSE and any future route.
   *Risk:* changing every handler signature is broad and easy to get half-right; a missed handler
   could run unauthenticated. *Mitigation:* keep `rpc_inner` calling the extractor explicitly at
   the top, add a test that every namespace authenticates, and do this as one mechanical commit
   guarded by the full integration suite.

2. **Richer, sealed principal.** Split the single `Actor` wrapper into an enum that records how the
   caller was authenticated — e.g. `Principal::Session { actor, session_id, expires_at }` versus
   `Principal::LocalOwner { actor }` — with `actor()` / `context()` accessors. Carry the validated
   `TenantContext` as the *only* scope; make fields private and the type `#[non_exhaustive]` so a
   handler cannot construct a privileged principal. This replaces the conflation of a real member
   with the machine owner and gives policy code a discriminator.
   *Risk:* touches every `auth.actor` access site (`realtime.rs:60`, `usecases` callers) and may
   alter debug/equality output used in golden fixtures. *Mitigation:* provide accessors that mirror
   the old field paths, add `From<Principal> for Actor`-style helpers, and run golden tests.

3. **Honor expiry; enable session rotation.** Stop discarding `expires_at` and
   `session_ttl_seconds`. Validate expiry once in this boundary and add a `rotate` usecase that
   refreshes the cookie when a session crosses a sliding window; emit `Set-Cookie` from the
   response path rather than only at sign-in. Wrap the raw token in `secrecy::SecretString`
   wherever it is held.
   *Risk:* rotation on every request adds a DB write and can race on concurrent requests; emitting
   a second `Set-Cookie` mid-response is a wire-visible behavior change. *Mitigation:* rotate only
   past a threshold, gate behind config, and record any externally visible change in
   `docs/PARITY.md` before shipping (per AGENTS.md).

4. **Use a vetted cookie parser/builder.** Replace `session_cookie` string splitting with the
   `cookie` crate / `axum-extra` `Cookie` extractor, and build `Set-Cookie` through a typed builder
   (handling `Secure`, `SameSite`, `Max-Age`, an optional `__Host-` prefix, and rejecting negative
   TTL). This removes duplicate-cookie, quoted-value and malformed-header edge cases.
   *Risk:* new dependency must pass cargo-deny and be recorded in `docs/DEPENDENCIES.md` (AGENTS.md
   requires `cargo add` + docs.rs verification, not memory). *Mitigation:* add property tests for
   the parser (per "property or fuzz tests for every external-format parser") and keep the exact
   `session` cookie name (`rpc.rs:1860`) for compatibility.

5. **Observability at the auth boundary.** Add `tracing` spans/instrumentation and counters for
   auth failures (no session, unknown/expired token, no-space, local-owner resolution) with request
   ids; never log the token itself. This is the natural place for the rate-limit / brute-force
   signal required by AGENTS.md.
   *Risk:* counters and spans add per-request overhead and can leak data if a token is ever
   included in a field. *Mitigation:* log only coarse reasons and hashed identifiers, keep fields
   out of the public error envelope (`error.rs`), and benchmark the hot path.

## Cross-cutting risks / sequencing

- Sequence 1 → 2 → 3/4 → 5: the extractor is the prerequisite that gives the sealed principal a
  single construction site; rotation and cookie hardening are independent thereafter.
- Two externally visible changes are possible here (new `Set-Cookie` semantics, principal debug
  shape). Treat each as a DEVIATION in `docs/PARITY.md` and wait for approval (AGENTS.md).
- Keep the REQ ids (040/041/043/066) in test names (`tools/trace_check.py` counts declarations).
- `local_first` (`authcap.rs:36-39`) must stay loopback-only; any refactoring must not widen it.
- Verification gate for each step: `cargo fmt --check`, `clippy -D warnings`, unit + `tests/`
  integration, and the golden/contract suites.

## Out of scope for this module

Provider-specific SSO/passkey flows (`authcap.rs`), password hashing policy, and per-procedure
authorization rules live elsewhere; this proposal only restructures the authentication boundary
and the principal it produces.
