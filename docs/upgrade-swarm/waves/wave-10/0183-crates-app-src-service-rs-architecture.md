# v0.4.0 architecture proposal — `crates/app/src/service.rs` (application composition root)

Worker #183 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no code changed).

Status of claims: all "current state" statements below are CONFIRMED by a direct read of the
621-line `service.rs` in this worktree. Forward-looking items are proposals (GUESS until
implemented and tested).

## Current structure (CONFIRMED)

- The module is the sole composition root: one `App { stores, services }` owns every port; the
  binary builds it from adapters, tests from fakes, nothing else constructs one (lines 1-4, 537-552).
- `Stores` has 45 public `Arc<dyn ...Store>` fields (24-116) and `Stores::new` takes 45 *positional*
  arguments under `#[allow(clippy::too_many_arguments)]` (124-171). Constructor order is not the
  field order (`effects`/`tools`/`tickets` move), so same-typed adapters can be silently swapped.
- `Services` (227-322), under `#[allow(clippy::struct_excessive_bools)]`, mixes four concerns: pure
  services (`hasher`, `tokens`, `ids`, `clock`, `models`), external transports, feature bools, and
  secrets/options (`secrecy::SecretString`, URLs, timeouts). `Services::new` (328-377) takes 12 args
  and sets safe defaults (all flags off, `Option`s `None`, `metrics` always allocated).
- 17 consuming `with_*` builders (384-534) configure the rest. `with_scim`/`with_scim_option`
  (427-437) are near-duplicates; `with_automation_option` (441-455) clamps timeout and maps `*`
  to `None`; `with_billing` (517-527) alone groups its flag, provider and secret in one call.
- Each feature is a flag + adapter/config *pair* that can drift: `board_enabled` has no adapter;
  `messaging_enabled`, `billing_enabled`, SCIM, metrics, notifications, computers and connectors
  each have an `Option` or bool beside their transport. Nothing enforces "enabled implies wired".
- There is no validation or `Result` anywhere: `App::new` (549-551) is a trivial struct literal, so
  an incoherent deployment fails at request time as a 404/no-op instead of at startup.
- All fields are `pub` and the types derive `Clone`, so any in-crate caller can mutate one port and
  bypass invariants. `From<AppError> for Failure` (603-621) is exhaustive but copies
  `error.to_string()` into `Failure.detail` while docs promise no PII.

## Guiding principle for v0.4.0

Make the composition root a *validating* boundary: construction is named (not positional),
configuration is applied once from a typed value, flags cannot outlive their prerequisites, and one
feature is one cohesive unit. Prefer making invalid states unrepresentable over runtime checks
sprinkled downstream.

## Upgrades (5 concrete items)

1. **Named construction for `Stores`.** Replace the 45-positional-arg `new` with a builder
   (`Stores::builder().accounts(a)… .build()`), or group into sub-structs (`CoreStores`,
   `CollabStores`, `ExtensionStores`). `build()` returns a `Result` listing any missing adapter.
   *Risk:* a mechanical but wide change across the binary and every test fake; a field swap during
   migration is silent because adapters share `Arc<dyn Store>` types. *Mitigation:* keep a deprecated
   `new` shim for one wave, migrate call sites field-by-field, and add a wiring test asserting each
   accessor resolves to its intended fake before removing the shim.

2. **Typed, coherent feature configuration + fail-fast validation.** Model each feature as
   `Feature<T> { Disabled, Enabled(T) }` (or `Option<T>` with one accessor) so `messaging_enabled`
   cannot be `true` without its transport/credentials; add `Services::validate()` and make
   `App::new(...) -> Result<App, ConfigError>` call it once at startup. Cover REQ-038/045/048/051/
   054/064 gates. *Risk:* every downstream `if app.services.*_enabled` read must change, and a wrong
   mapping could expose a route that must answer 404. *Mitigation:* keep accessors mirroring the old
   field names, add one test per flag for both on/off paths, and keep the "absent, not degraded"
   404 behavior from the spec.

3. **One config-application pipeline.** Collapse the 17 `with_*` setters into a single typed
   `DeploymentConfig` applied once (`Services::from_config(config)`), delete the duplicate
   `with_scim`/`with_scim_option`, and follow `with_billing`'s example of grouping a flag with its
   settings. *Risk:* precedence bugs (a later setter silently overriding an earlier one) and a
   large call-site diff in the binary's boot path. *Mitigation:* define explicit precedence in one
   place, keep `#[must_use]` builders as thin deprecated wrappers, and boot the binary in a test
   with a fully populated config to assert the resulting `Services`.

4. **Split `Services` by concern.** Extract `DeploymentSettings` (flags, URLs, timeouts, secrets)
   and `Transports` (MCP, messaging, model, OAuth, billing, computer, connector) as private
   sub-structs, leaving `Services` as pure domain services; give it a redacted `Debug` and private
   fields with accessors. *Risk:* touches every `app.services.*` path and risks leaking a secret if
   the settings struct is ever logged wholesale. *Mitigation:* keep accessors that mirror old paths,
   derive `Debug` only with `secrecy`'s redaction, and add a test that no debug/log output contains
   a token or webhook secret.

5. **Protect invariants / unit-of-work.** Make `Stores` and `Services` fields private behind typed
   accessors so no caller can substitute a single port after construction; explore a transaction
   port that groups the two or three stores written by one use case. *Risk:* `Arc<dyn Store>` does not
   compose into a transaction naturally, and a broad visibility change can break adapter tests.
   *Mitigation:* ship accessors first (behavior-preserving), keep transaction grouping behind a new
   port with a no-op/fake adapter, and prove atomicity with an integration test rather than assuming
   it.

## Cross-cutting risks / sequencing

- Sequence 1 → 3 → 2 → 4 → 5: named construction and one config pipeline must land before a
  validating `App::new` can assert coherence; encapsulation and transactions follow once fields are
  stable.
- No behavior change is intended; if any boot error or route availability becomes externally
   visible, record it as a DEVIATION in `docs/PARITY.md` and wait for approval (AGENTS.md).
- Keep REQ ids in test names (`tools/trace_check.py` counts declarations, not comments).
- Verification gate per step: `cargo fmt --check`, `clippy -D warnings`, unit + `tests/` integration,
  and the golden/contract suites; mark outcomes CONFIRMED only after running them.

## Out of scope for this module

Use-case logic in `usecases.rs`, adapter implementations in `infra`, and `AppError`'s variant design;
this proposal only restructures the composition root and the configuration it validates.
