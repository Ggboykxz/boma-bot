# 0174 — `crates/app/src/lib.rs` — reliability lens (v0.4.0)

Target: `crates/app/src/lib.rs` (29 lines: crate docs, `#![deny(missing_docs)]`, 15 module
declarations, 4 re-export groups) as the public facade of `boma-app`, plus the composition root it
exposes (`service.rs` `App`/`Stores`/`Services`) and the readiness type it re-exports (`health.rs`).
Spec: AGENTS.md "Rust standards" (fail-fast config, typed errors, no panics in non-test code),
REQ-053/REQ-054 (readiness, capabilities), and the runtime feature flags in REQ-038/045/048/054/064.
Claims: CONFIRMED = read/searched in this tree this session; GUESS = inferred, needs a build/test.
Read-only review; this document is the only write.

## Current behaviour (read, not run)

- All 15 `pub mod` declarations are unconditional; `boma-app/Cargo.toml` has **no `[features]`** and
  a grep of `crates/app/src` finds **no `cfg(feature = ...)`** anywhere (CONFIRMED).
- Every re-export in lines 25-29 is unconditional: `accounts`, `error`, `health`, `service`,
  `worker`. `#![deny(missing_docs)]` applies crate-wide (CONFIRMED).
- `App`, `Stores`, `Services` are plain public structs. `Stores::new` takes **45 positional**
  `Arc<dyn *Store>` arguments; `Services::new` takes 12 and defaults every optional subsystem to
  off/`None`; `with_*` builders then set them one by one (CONFIRMED, `service.rs` 118-535).
- `with_automation_option` is the only builder that sanitises its input (`wait_timeout.max(1)`,
  `"*"` → `None`). No other builder validates (CONFIRMED).
- `Readiness::is_empty()` is documented "the binary refuses to start in that state", but the
  refusal itself lives in the binary, not in this crate (CONFIRMED, `health.rs` 46-50).
- No `panic!`/`unwrap`/`expect` in non-test production code under `crates/app/src` (the four `expect`
  hits are in `worker.rs` tests); workspace lints deny `panic`, `unwrap_used`, `expect_used`.
  `push.rs` uses a `OnceLock<reqwest::Client>` with an infallible `unwrap_or_else` fallback
  (CONFIRMED). `Capabilities::default()` reads only the compile-time `env!("CARGO_PKG_VERSION")`.
- Optional surfaces are runtime `bool`/`Option` fields on `Services`, never compile-time features
  (CONFIRMED).

## Failure modes

1. **Feature gates would break the facade (medium, high blast radius).** Wrapping any one of the 15
   modules in `#[cfg(feature = ...)]` immediately breaks its unconditional `pub use` in lines
   25-29 and every intra-crate path (`crate::health`, `crate::service`); gating only a type inside a
   module leaves a dangling re-export. There is no `compile_error!` for impossible feature
   combinations and no build matrix, so the first feature work lands as a confusing compile error
   rather than a designed boundary (CONFIRMED code; GUESS until a feature build is attempted).
2. **Impossible runtime configurations are representable (high).** Public fields plus builders allow
   `billing_enabled = true` with no provider/secret, `messaging_enabled = true` with `slack = None`,
   or `vapid = Some(..)` with `endpoint_safety = None` (the syntactic-only SSRF fallback). Such a
   process starts, serves 200 on `/ready`, and then refuses every call — or checks endpoints too
   weakly. Nothing in `lib.rs`/`service.rs` offers a `validate()` seam (CONFIRMED code; runtime
   impact GUESS).
3. **Positional mis-wiring (medium).** With 45 structurally similar `Arc<dyn Store>` arguments,
   swapping two adjacent stores compiles and surfaces only as wrong data in a distant use case; the
   facade provides no typed or named construction (CONFIRMED signature; occurrence GUESS).
4. **Readiness invariant not owned (medium).** `is_empty()` says an empty probe set is a startup
   refusal, but only `crates/bin` enforces it. Any second embedder of `boma-app` (testkit, future
   worker binary) that forgets the check gets a permanently-green `/ready` — a false health signal
   (CONFIRMED doc and call site; GUESS for future embedders).
5. **Startup panic surface (low, currently well contained).** Lints + the grep above make a panic
   from this crate unlikely today; the residual risk is future regression: a panicking `Default` or
   `OnceLock::get_or_init` in a newly re-exported module, or config parsing added to a builder
   without a `Result` return. `push.rs` already parses VAPID via `Result`, which is the right shape
   (CONFIRMED; regression risk GUESS).
6. **Facade drift (low).** A renamed/moved module is caught by downstream `boma-api`/`boma-bin`
   imports only when those crates are rebuilt; there is no compile-time smoke test that imports the
   whole documented public surface (CONFIRMED absence).

## Concrete hardening (v0.4.0)

- **Make gating explicit and safe.** Add one Cargo feature per optional subsystem (keep all default
  on so the v0.3 build is unchanged), put matching `#[cfg(feature = ...)]` on both the `pub mod` and
  its `pub use`, and add `#[cfg(not(...))] compile_error!` only for meaningless combinations. CI:
  `cargo check --no-default-features`, `--all-features`, plus a per-feature check loop; full tests
  stay on default features to bound CI time.
- **Add `App::try_new(stores, services) -> Result<App, AppConfigError>`** (typed `thiserror`
  variant per broken invariant: billing enabled without provider/secret, messaging without Slack,
  VAPID without endpoint checker, board without tickets store). Keep `App::new` for fakes/tests but
  have the binary use `try_new`. Additive API only; no wire change.
- **Fold readiness into validation.** Add `Readiness::require_nonempty(self) -> Result<Self, ...>`
  and give `AppState`/the binary one `validate()` call so every embedder inherits the documented
  startup refusal. Test name carries REQ-053.
- **Replace positional construction.** Introduce `StoresBuilder` (named setters, `build() ->
  Result<Stores, MissingStore>`), retain `Stores::new` as a deprecated shim for one release, and
  migrate `crates/bin` + `crates/testkit` in a single PR so the two constructors never coexist
  partially.
- **Facade smoke test.** Add a test/doc-test that imports every item from lines 25-29
  (`App, Stores, Services, Actor, Failure, SendOutcome, BotWithThread, Worker, AppError,
  Readiness, Capabilities, ReadinessCheck, ReadinessReport, ...`) so gated or renamed modules fail
  CI with a clear message. Audit `OnceLock`/`Default` uses in re-exported modules to stay infallible.
- **Document the invariant in the crate docs** at the top of `lib.rs`: domain → app → infra, no
  concrete adapter, features are additive. This keeps future gating from inverting the dependency
  direction.

## Risks / trade-offs

- Default features must remain "all modules on"; otherwise a leaner build silently drops a route
  family — that is a DEVIATION needing `docs/PARITY.md` and approval. Keep gating compile-only.
- `try_new` makes previously "working but misconfigured" deployments fail fast. That is the point,
  but it can look like a startup regression: fix all flag combinations in the bin first, then
  promote claims from GUESS to CONFIRMED after running each combination.
- The test harness intentionally uses `endpoint_safety = None` and ephemeral secrets. Validation
  must allow that path explicitly (e.g. a named `Testkit` profile or an explicit
  `allow_insecure_endpoint_check` marker), never a silent production default.
- `StoresBuilder` touches every `Stores::new` call site; high churn and merge risk. Do it in one PR
  and keep the shim minimal.
- `compile_error!` guards can break downstream consumers mid-upgrade: restrict them to combos that
  cannot work at all, not to stylistic requirements.
- A feature matrix multiplies CI minutes; checks only (no test runs) per feature. Anything marked
  GUESS above is unverified until `cargo check --all-features` and `cargo clippy -D warnings` are
  run in this tree.
