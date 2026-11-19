# v0.4.0 architecture proposal — `crates/api/src/state.rs` (shared AppState)

Worker #155 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no code changed).

Status of claims: every "current state" statement below is CONFIRMED by direct reads of
`crates/api/src/{state,auth,authcap,config,health,realtime,correlation,lib,router}.rs`,
`crates/bin/src/commands.rs` and the `AppState::new` call sites in `crates/api/tests/` and
`crates/conformance/tests/` in this worktree. Forward-looking items are proposals (GUESS until
implemented and verified). English per repo disk convention; no gate weakened.

## Current structure (CONFIRMED)

- `AppState` is a cheap-to-clone container of five private `Arc`s — `Readiness`, `Capabilities`,
  `AuthCapabilities`, `HttpConfig`, `App` (state.rs:14-20) — re-exported at the crate root
  (lib.rs:33) and injected into every handler via axum `State<AppState>` (router.rs:24,260;
  rpc.rs:281+; health.rs:114,135,186; correlation.rs:165; realtime.rs:49). Read-only getters and
  a hand-written, non-exhaustive `Debug` (state.rs:97-105) keep the shape stable.
- One allocation per field means each of axum's per-request clones bumps five refcounts; there is
  no single "inner" allocation, hence no natural point for a later atomic swap or shared handle.
- `app()` returns `&App`, the whole application: handlers reach `app().stores.*` and
  `app().services.*` directly (realtime.rs:61-75; health.rs:136-149). The state imposes no boundary
  on *what* a handler may touch — the crate comment (lib.rs:3-5) is a convention, not a type.
- Construction is duplicated: the binary calls `new()` once (commands.rs:74-80) but nine test call
  sites rebuild the same five parts (api/tests/http_surface.rs:85,283,593,974,1206,1247;
  rpc_surface.rs:69; screen.rs:58; conformance/tests/golden_api.rs:77/87). Any new field fans out
  to every site, and a test helper can silently diverge from production wiring.
- `invitation_ttl_seconds()` (state.rs:65-69) does not read state: it returns
  `HttpConfig::DEFAULT_INVITATION_TTL_SECONDS` directly. It is a dangling constant a caller can
  mistake for configuration — the same rot the config worker flagged for this constant.
- `authenticate()` (state.rs:83-94) puts auth *behaviour* on a dependency container and takes raw
  `HeaderMap`; it duplicates the `crate::auth::authenticate` path used by `rpc_inner`.
- Graceful shutdown is wired only at the server edge (`axum::serve(...).with_graceful_shutdown`,
  commands.rs:91-95), but handlers spawn untracked tasks (realtime.rs:82 `tokio::spawn`). The state
  carries no signal those tasks can observe.
- There is no `#[cfg(test)] mod tests` in this file: sharing, Debug redaction and TTL wiring are
  unverified here.

## Guiding principle for v0.4.0

Keep `AppState` a typed, immutable dependency *container*: one cheap clone, one construction
boundary, explicit capabilities. Behaviour (authentication) and unbounded access to `App` move out
or narrow; cross-cutting runtime signals move in.

## Upgrades (5 concrete items)

1. **Single `Arc<StateInner>` behind the public type.** Store the five parts in a private
   `StateInner` and keep only `inner: Arc<StateInner>` on `AppState`, with today's getters as thin
   delegates. A request clone becomes one atomic increment and all fields share an allocation,
   giving a later hot-swap/shutdown coordination point.
   *Risk:* pointer-identity assumptions in tests and any per-field `Arc::clone` change; getters must
   keep returning `&T` so call sites compile unchanged. *Mitigation:* add a unit test that two
   clones expose equal `&T` addresses and that `Debug` output is unchanged; land as one mechanical
   commit guarded by the full suite. No wire change.

2. **One construction boundary: `StateParts` + `AppState::from_parts`.** Introduce a struct of the
   validated parts with a fallible constructor, have the binary call it, and let test helpers
   obtain a `StateParts::test(...)` fixture instead of repeating five arguments. Field additions
   then have one production site and one fixture.
   *Risk:* fixtures still using `new()` would silently keep stale defaults (e.g. `local_first`);
   making `new()` crate-private breaks external test crates. *Mitigation:* keep `new()` as a
   deprecated delegate for one release, assert in a test that the fixture's `http()` values match
   the binary's validated config, and migrate all nine call sites before removing it.

3. **Remove the dangling TTL and narrow the surface.** Either add a bounded `invitation_ttl` field
   sourced from validated config, or delete `invitation_ttl_seconds()`; do not keep a getter that
   ignores state. Add typed accessors for the capabilities handlers actually need (e.g.
   `metrics()`, `publisher()`) rather than handing out the whole `App`, while keeping `app()` for
   the existing procedure table during migration.
   *Risk:* a field change alters a wire value if invitations are exposed; a new env var changes the
   config surface (record in `docs/STATE.md`; a value change needs `docs/PARITY.md`).
   *Mitigation:* check golden fixtures first; prefer deletion if no caller needs a configurable TTL;
   make new accessors additive so no handler breaks.

4. **Authentication leaves the container.** Delete `AppState::authenticate` and let the auth layer
   own the single extractor proposed in worker #121's architecture note; `AppState` then supplies
   only `app()`/`http()` dependencies to that extractor.
   *Risk:* the two proposals must land in a coordinated order or both paths stay live; a missed
   conversion could run a handler unauthenticated. *Mitigation:* convert `rpc_inner` first, add a
   test that every registered namespace resolves a principal through the extractor, then remove
   the convenience method; keep REQ ids 040/041/066 in test names.

5. **Carry a shutdown signal for spawned work.** Add a `tokio::sync::watch::Receiver<bool>` (or, if
   choosing `tokio-util::sync::CancellationToken`, via `cargo add` + `docs/DEPENDENCIES.md`) built
   in `commands.rs::serve`, hand the sender to `shutdown_signal()`, and have `realtime::subscribe`
   select on it so SSE producer tasks end when the server drains.
   *Risk:* a new dependency needs cargo-deny and DEPENDENCIES entries; closing streams early is
   client-visible (a DEVIATION if reconnect semantics change). *Mitigation:* prefer the no-new-
   dependency `watch` option, fire it only after `with_graceful_shutdown` resolves, and test that
   an in-flight stream still finishes its last frame.

## Cross-cutting risks / sequencing

- Sequence 1 → 2 → 3/4 → 5. Step 1 is the low-risk base; step 2 removes test/prod drift; 3 and 4
  are independent afterwards; 5 depends on the binary wiring from 2.
- No secrets live in `AppState` today. If any are ever added, use `secrecy::SecretString` and keep
  the hand-written `Debug` non-exhaustive and redacted (REQ-054).
- The REQ-066 loopback-only `local_first` invariant stays in `HttpConfig`; this refactor must not
  widen it.
- Verification gate per step: `cargo fmt --check`, `clippy -D warnings`, unit + `tests/`
  integration, golden and contract suites.

## Out of scope for this module

`App` internals, provider configuration, auth policy and the router's route table belong to other
modules; this proposal only reshapes the shared container and its construction.

## Evidence

CONFIRMED — read this session: `state.rs:1-105`; `lib.rs:1-37`; `config.rs:64-288`;
`authcap.rs:1-68`; `health.rs:114-188`; `realtime.rs:48-110`; `correlation.rs:160-189`;
`router.rs:24`; `commands.rs:42-98,425-449`; the `AppState::new` call sites listed above.
GUESS — that axum clones `State<AppState>` once per request (standard axum behaviour, not measured
here); whether any golden asserts `Debug` or `invitation_ttl_seconds()` output (verify before 3).
