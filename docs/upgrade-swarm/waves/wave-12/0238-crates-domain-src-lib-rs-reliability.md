# 0238 — `crates/domain/src/lib.rs` — reliability lens (v0.4.0)

Target: `crates/domain/src/lib.rs` (149 lines: crate docs, `#![deny(missing_docs)]`, 32
unconditional `pub mod` declarations, ~30 `pub use` re-export groups, two root free functions
`percent_encode`/`format_webhook`, one unit-test module) as the single public facade of
`boma-domain`. Downstream `app`/`infra`/`api` import it (`boma_domain::percent_encode`,
`boma_domain::format_webhook`, ids, errors). Workspace version is `0.3.0-rc5` → target v0.4.0.
Spec: AGENTS.md "Rust standards" (lints, typed errors, no panics in non-test code, property
tests for parsers); this crate's own doc promise "no IO, no clock, no async, provably correct".
Claims: CONFIRMED = read/searched in this tree this session; GUESS = inferred, needs a build/test.
Read-only review; this document is the only write.

## Current behaviour (read, not run)

- `boma-domain/Cargo.toml` has **no `[features]`**; a grep of `crates/domain/src` finds **no
  `cfg(feature = ...)`**. All 32 modules and every re-export are unconditional (CONFIRMED).
- The only crate-level lint is `#![deny(missing_docs)]`; everything else rides `[workspace.lints]`
  (`unsafe_code = forbid`; `unwrap_used`/`expect_used`/`panic`/`todo`/`unimplemented`/`dbg_macro`/
  `print_stdout` = deny; clippy all+pedantic = warn). No `missing_debug_implementations`,
  `unreachable_pub`, or `compile_error!` guard (CONFIRMED).
- Re-exports are explicit (no glob `pub use *`), one group per module, carrying validator fns,
  `MAX_*` constants and newtype ids (CONFIRMED).
- `percent_encode` is used across `infra` (oauth.rs, messaging_http.rs, model_http.rs) and `app`
  (usecases.rs — MCP/Slack OAuth) — correctness-critical and shared; it has exactly one unit test
  (`req_033_...`), no proptest (CONFIRMED).
- `format_webhook` silently drops all whitespace (`split_whitespace().collect()`); its only
  consumer is `infra/stores/destinations.rs:82`, which validates the *normalized* string for
  prefix/`https`/`@` before sealing the destination. It has no dedicated test (CONFIRMED).
- No test/doc-test imports the whole documented facade surface; no semver/API-diff gate (CONFIRMED).

## Failure modes

1. **Feature gating would fracture the facade (medium, high blast radius).** Wrapping a `pub mod`
   in `#[cfg(feature = ...)]` without its matching `pub use` leaves a dangling re-export and a hard
   compile error; gating only an item inside a module breaks the re-export list. With no feature
   matrix and no `compile_error!` for impossible combos, the first gating PR lands as an opaque
   build failure and a leaner build can silently drop a validator family (CONFIRMED structure;
   failure GUESS until a feature build).
2. **Export drift (medium).** Adding/refactoring a module but forgetting its root re-export — or
   renaming a re-exported item — is invisible to this crate's own tests; it only breaks downstream
   crates when they rebuild. A stale `pub use` likewise survives while the module still exists.
   32 modules + ~30 groups make this a realistic maintenance error (CONFIRMED absence of a surface
   test; occurrence GUESS).
3. **Compile-time invariants thinner than the doc claims (medium).** `missing_docs` is denied, but
   `missing_debug_implementations`, `unreachable_pub` and `must_use` coverage are not; a new public
   type can ship without `Debug` or an accidentally-public helper can become API. Nothing enforces
   "no IO/clock/async": a future `tokio` or `SystemTime` dep in `boma-domain/Cargo.toml` would
   compile silently (CONFIRMED; regression GUESS).
4. **Normalization/validation gap on `format_webhook` (medium, security-adjacent).** The helper
   removes whitespace, so the string validated in `destinations.rs` is not necessarily the string
   stored/sent; embedded tabs/newlines can be concatenated into a different URL than the operator
   typed, and prefix/`@` checks run on the normalized value only. A future caller reusing it for a
   different webhook type inherits a Slack-shaped assumption (CONFIRMED code path; exploitability
   GUESS).
5. **Shared encoder under-tested (low/medium).** `percent_encode` feeds OAuth/query assembly in at
   least five sites; a regression would corrupt auth redirects broadly. One happy-path test and no
   property test leave non-ASCII/control bytes uncovered (CONFIRMED).
6. **Facade has no pinned API contract (low).** Nothing pins the root surface between v0.3 and
   v0.4; an accidental removal or rename is caught only by a full downstream rebuild (CONFIRMED).

## Concrete hardening (v0.4.0)

- **Design feature gates, don't discover them.** Add `[features]` with every module feature
  default-on (v0.3 build unchanged); gate `pub mod` and its `pub use` in the same change, and add
  `#[cfg(not(feature = ...))] compile_error!` only for truly impossible combos. CI:
  `cargo check --no-default-features`, `--all-features`, plus a per-feature loop; full tests stay
  on defaults.
- **Facade smoke test / API pin.** Add a test importing *every* root re-export and both free
  functions (a `use boma_domain::{...}` list mirroring lines 45–111), so gated, renamed or removed
  items fail fast with a clear message. Optionally add `cargo public-api`/semver-checks to diff the
  v0.3 surface.
- **Tighten compile-time invariants.** Add `#![deny(missing_debug_implementations,
  unreachable_pub)]` to the crate root (keep both `#[must_use]` functions). Add a CI dependency
  check asserting `boma-domain` gains no `tokio`/IO deps beyond the agreed set, enforcing the
  "pure rules" promise.
- **Close the normalization gap.** Document `format_webhook` as a normalizer, add tests for
  whitespace/control bytes and for validated-vs-stored equality, and have `destinations.rs`
  validate the same string it persists (or parse with `url::Url`). Behavior changes are DEVIATIONs
  for `docs/PARITY.md`.
- **Property-test the encoder.** Add a `proptest` equivalence test for `percent_encode` over
  arbitrary byte strings against a reference RFC 3986 encoder; keep the REQ-named unit test.
- **Document the invariant at the top of `lib.rs`**: leaf crate of pure rules; root re-exports are
  the supported API; features are additive; gate a module and its re-export together.

## Risks / trade-offs

- Any feature defaulting off drops behavior silently — a DEVIATION needing `docs/PARITY.md` and
  approval. Keep all features default-on and compile-only in v0.4.0.
- `compile_error!` guards can break downstream consumers mid-upgrade; restrict them to combinations
  that cannot compile at all, not style preferences.
- `missing_debug_implementations`/`unreachable_pub` may force `Debug` derives on many types; stage
  as workspace warnings first, then deny, to avoid one huge PR.
- The facade smoke test must change in the same PR as any re-export or it becomes the thing that
  drifts.
- Changing `format_webhook` semantics or the stored webhook value is externally visible (data
  format); coordinate with `destinations.rs` and mark as DEVIATION if stored data changes.
- The dependency check needs an explicit allow-list (`chrono`, `serde`, `url`, ...) or it will
  false-positive; treat it as a guardrail, not a gate, until tuned.
- Feature matrix and extra checks multiply CI minutes: checks only, no full tests, per feature.
  Every GUESS above stays unverified until `cargo check --all-features` and `cargo test` are run
  in this tree.
