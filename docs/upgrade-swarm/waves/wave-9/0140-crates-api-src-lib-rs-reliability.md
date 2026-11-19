# v0.4.0 reliability proposal — `crates/api/src/lib.rs` (crate root / public surface)

Worker #140 · LENS=reliability · target v0.4.0 · READ-ONLY analysis (no code changed).

Status of claims: "current state" statements are CONFIRMED by a direct read of `lib.rs` (all 37
lines), `router.rs`, `state.rs`, `config.rs`, `openapi.rs`, `pages.rs`, `crates/bin/src/{cli,
commands}.rs` and both `Cargo.toml` files. The grep for `cfg(feature` across `crates/**/*.rs`
returned 0 matches and `crates/api/Cargo.toml` has no `[features]` table (CONFIRMED). No tests
were run; forward-looking items are GUESS until implemented and tested.

## Current structure (CONFIRMED)

- The crate root is *declarative only*: module docs (1-5), `#![deny(missing_docs)]` (7),
  17 unconditional `pub mod` declarations (9-26), and three `pub use` groups (28-36) that
  re-export 13 names from `auth`, `authcap`, `config`, `error`, `router`, `state`, `wire`.
- There are no functions, statics, initializers, `cfg` attributes or feature gates in this
  file. It cannot fail at runtime on its own; its reliability role is as the **single
  enforceable boundary** for what the HTTP crate exposes and assembles.
- Lints come from `[workspace.lints]`: `unsafe_code = "forbid"`, `unwrap_used`/`expect_used`/
  `panic`/`todo`/`unimplemented` all `deny`. This crate additionally denies `missing_docs`.
  These are *compile-time* gates and are bypassable via `#[allow(...)]` (e.g. `pages.rs:75-96`
  allows `expect_used` with a justification comment).
- Assembly happens outside the root: `router(state: AppState) -> Router` (`router.rs:22+`)
  is infallible, clones `HttpConfig`, registers every route/layer, and carries
  `#[allow(clippy::too_many_lines)]` for the route table. `AppState::new` (`state.rs:23+`) is
  `#[must_use]` but infallible and takes "already-validated parts" by convention.
- `utoipa`'s `ApiDoc` (`openapi.rs:19-49`) lists a *subset* of the router's paths and schemas;
  a contract test compares `document()` with `spec/openapi.yaml` (per the module docs).
- Startup order in `commands::serve` (`commands.rs:42-99`): `connect_lazy` → `Readiness::new`
  → `computer.provision().await` → `build_app(...)` → `AppState::new` → `TcpListener::bind`
  → `axum::serve(... router(state) ...)` with graceful shutdown. Config validation is in
  `cli::load_config()` (`Config::from_env()` + telemetry init); `HttpConfig::new` and its
  `with_*`/`validate_web_root` methods return typed `HttpConfigError` (fail-fast).
- `pages::runtime_etag` (`pages.rs:51-61`) is the crate's only `OnceLock`; its closure is
  infallible and initializes on first request, not at startup.

## Guiding principle for v0.4.0

Keep the root declarative, but make its implied invariants machine-checkable: one place declares
modules, public names, routes and docs. A valid build must prove they agree under every feature
combination, and no startup path may reach `bind` with unvalidated state or a panicking assembler.

## Failure modes

1. **Init ordering (highest severity).** The "already-validated parts" precondition of
   `AppState::new` is prose, not a type; any caller can pass an unvalidated `HttpConfig` or a
   `Readiness` built against a different pool. `router()` returns `Router`, not `Result`, so
   assembly cannot report a problem, and in `serve` it is invoked *after* the socket is bound:
   a fault during route construction leaves a listening port whose probes answer before the
   surface is complete. `cli::run` also routes `Command::Automation` straight to
   `automation::run(command)` without `load_config()` (CONFIRMED in `cli.rs:70-78`), so
   validation coverage differs by subcommand. Risk: inconsistent startup state, misleading
   `/ready` results, and a crash loop that rebinds on every restart.
2. **Feature-gate drift.** Today nothing is gated (CONFIRMED), so v0.4.0 gates added for
   optional subsystems (`openapi`, `realtime`, `automation`, static pages) would have to be
   mirrored in four independent lists: `pub mod`, `pub use`, `router()` route registration,
   and `openapi.rs` `paths(...)`/`components(...)`. Gate a `mod` without its `pub use` →
   `E0603`/`E0432`; drop a route without updating `ApiDoc` → contract-test failure that only
   appears with that exact feature set. With `resolver = "3"`, feature unification across
   workspace members can silently re-enable a gate in one binary and not another. Default-
   feature CI would not detect any of this. Risk: broken builds, or a served route absent
   from the published contract (a parity defect, not just a compile error).
3. **Panic at startup.** The root has no runtime code, but it re-exports the assembly entry
   points; a future `.expect()` inside `router()`/`AppState::new`, or a panicking `OnceLock`
   initializer in a re-exported module, would panic *after* bind in `serve` (the crash-loop
   shape above). Clippy's `panic = deny` is not a runtime guarantee: `#[allow(...)]` is legal
   and already used in the crate, and denied lints do not cover panics from dependencies or
   from `const`/`static` evaluation. `Uuid::new_v4()`-style RNG panics in a re-exported path
   (see the correlation proposal, worker #134) have the same effect. Risk: the process exits
   without an `anyhow` context line, so operators see no actionable cause.

## Upgrades (concrete hardening, with risks)

1. **Make initialization typed.** Add `AppState::try_new(...) -> Result<Self, ApiInitError>`
   that re-checks the cheap invariants (`HttpConfig` bounds already enforced upstream,
   non-empty capability version, validated web root) and keep `new` as the infallible
   inner constructor. In `serve`, build the router **before** `TcpListener::bind`.
   *Risk:* moving bind changes bind-failure ordering (a DEVIATION only if observable in logs;
   record in `docs/PARITY.md` if so). Mitigation: keep log text and error context identical;
   add an integration test that starts `serve` with a bad config and asserts no port opens.
2. **Expose a fallible assembly entry point additively.** Provide `try_router(state) ->
   Result<Router, ApiInitError>` and let today's `router` delegate to it only after the
   validated state exists. *Risk:* two public builders invite drift themselves. Mitigation:
   make `router` a thin `#[must_use]` wrapper with no logic of its own and add a test that
   `try_router` succeeds for a golden `AppState`.
3. **Adopt an explicit feature policy before any gating.** Define `[features]` in
   `crates/api/Cargo.toml`; put the `pub mod` and its matching `pub use` in the same `#[cfg]`
   block, and generate route registration from one table that also feeds `ApiDoc` where
   practical. Add a compile-time contract test that asserts "every re-exported module has a
   route or is explicitly listed as non-HTTP" and "every `ApiDoc` path is registered".
   *Risk:* macro-generated route tables hurt readability; cfg on utoipa derives can drop
   schemas and break the `spec/openapi.yaml` comparison. Mitigation: start with the test and
   a documented feature matrix, then gate only one optional module as a pilot.
4. **Gate the gates in CI.** Run `cargo check -p boma-api --no-default-features`,
   `--all-features`, and `cargo test -p boma-api --all-features` (plus the existing contract
   test) on every PR. *Risk:* roughly 2-3x more CI time on this crate; `--all-features` can
   mask a `--no-default-features` miscompile when dependencies unify features. Mitigation:
   keep both extremes, not just the union, and record the commands in `docs/PLAN.md`.
5. **Audit panic escapes, don't rely on lint denial.** Require every `#[allow(clippy::
   expect_used|panic)]` to carry a one-line justification and keep a CI grep that fails on
   new ones; add a `#[test] fn req_..._startup_assembly_never_panics()` using
   `std::panic::catch_unwind` around `router(state)` with a golden `AppState`, and a test that
   `runtime_etag()` is stable and infallible. *Risk:* `catch_unwind` still prints the panic
   hook and requires `UnwindSafe`; with `panic = "unwind"` (CONFIRMED in `Cargo.toml`) it
   works; an ADR would be needed to change that profile. *Mitigation:* treat the test as a
   regression tripwire, not production recovery.

## Verification (for the implementing agent; not run here — GUESS)

- `cargo fmt --check && cargo clippy -p boma-api --all-targets -- -D warnings`
- `cargo test -p boma-api --all-features` / `--no-default-features`; `cargo check` likewise.
- The OpenAPI contract test vs `spec/openapi.yaml`; any diff is a DEVIATION to file in
  `docs/PARITY.md`. Re-run `grep -rn --include=*.rs 'cfg(feature' crates` after gating.

## Scope note

This proposal writes only to this document. The `commands.rs`/`cli.rs` changes are recommendations
for the owning worker; `lib.rs` should stay a pure manifest of modules and re-exports, with new
checks expressed as lints, tests and feature metadata rather than startup code.
