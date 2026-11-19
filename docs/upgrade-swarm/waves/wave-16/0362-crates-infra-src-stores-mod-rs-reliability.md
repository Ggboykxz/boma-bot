# v0.4.0 reliability proposal — `crates/infra/src/stores/mod.rs` (worker #362, lens: reliability)

Scope: the 52-line barrel that declares 45 `pub mod` store adapters plus one private
`mod computer_row`, and its relationship to the 44 facade re-exports in `crates/infra/src/lib.rs`
(lines 59–103). Read-only analysis; no source was changed. Claims about file structure are
CONFIRMED by inspection; runtime impact is GUESS unless a test is named.

## What already protects us (CONFIRMED)

- `#![deny(missing_docs)]` sits at the crate root (`lib.rs:13`), so a new `pub mod` here without a
  `//!` doc comment fails the build — documentation drift is caught, export drift is not.
- Removing/renaming a `pub mod` breaks its `pub use stores::…` at compile time. Only the *missing
  re-export* direction is silent.
- `computer_row` is correctly private; `rows` is `pub mod` but exposes only `pub(crate)` items, so
  no public type escapes the barrel by accident today.
- No `#[cfg]`, no `feature` token here, and no `[features]` table in any workspace `Cargo.toml`:
  every store is unconditionally compiled, so there is no existing gate to drift against yet.

## Failure modes

### 1. Export drift

- **E1 — two hand-maintained lists (CONFIRMED structure, GUESS impact).** `mod.rs` declares 45
  public store modules; `lib.rs` re-exports 44. The one declared-but-not-re-exported module is
  `rows`, which has no public items (harmless *today*). Adding a new store here without a matching
  `pub use` compiles cleanly: the adapter is reachable only as `boma_infra::stores::x::…`, never as
  the facade `boma_infra::PostgresXStore`. Downstream wiring then fails late with an unresolved
  import instead of a clear "forgot to export" error.
- **E2 — multi-type modules hide a dropped export (CONFIRMED).** `cards` exports two types
  (`PostgresCardStore`, `PostgresEffectStore`) and `secrets` exports `PostgresSecretStore` plus
  `rekey_space_keys` (`lib.rs:67,96`). A routine edit that re-exports only the primary store type
  silently removes the second from the facade; nothing in the barrel or crate root detects it.
- **E3 — visibility inconsistency (CONFIRMED).** `rows` (line 42) is `pub mod`, yet all its items
  are `pub(crate)` and ~20 sibling modules import it as `super::rows`. It is an empty public
  namespace beside the correctly private `computer_row` (line 16); reviewers may mistake it for a
  supported surface.
- **E4 — non-uniform grouping masks omissions (CONFIRMED).** Declarations run in two blocks with
  `signin` at line 21 while `lib.rs` re-exports it at line 74, out of sequence with its neighbours;
  there is no one-to-one visual correspondence between the lists. GUESS: a reviewer checking
  "one `pub mod` ⇔ one `pub use`" will miss a new unbalanced entry.

### 2. Init ordering

- **I1 — declaration order is not initialization order (CONFIRMED gap).** The barrel is purely
  declarative: listing a module says nothing about when its pool, tenant context or DEK is ready.
  Its own doc comment says every tenant-data adapter must go through `crate::tenant` (REQ-043), but
  nothing enforces it — a new store can call the pool directly, compile, and bypass RLS scoping.
  GUESS: one such adapter turns a cross-tenant bug into a data leak no compile-time check catches.
- **I2 — `rekey_space_keys` has no ordering guard (CONFIRMED export, GUESS flow).** `lib.rs:96`
  exposes it alongside ordinary store types. Rewrapping space DEKs is safe only after config
  validation has established a *stable* master key; called earlier it can rewrap under an ephemeral
  key and permanently lock out every sealed credential (worker #354, S1). The barrel signals no
  such prerequisite.
- **I3 — no static port assertion (CONFIRMED).** The barrel declares modules; the facade names
  concrete `Postgres*Store` types; nothing asserts `PostgresXStore: ports::XStore`. A missing method
  — or a port default method silently taken — surfaces at app-wiring time or not at all.

### 3. Feature gates

- **F1 — any future gate must be tri-synchronous (CONFIRMED absence).** There is no `[features]`
  table; heavy deps (`bollard`, `tokio-tungstenite`, `reqwest`, `aes-gcm`) are unconditional. If
  v0.4.0 adds `docker`/`stdio`/`messaging` features to trim a minimal build, `#[cfg(feature = …)]`
  must be mirrored in `Cargo.toml`, this file's `pub mod`, and `lib.rs`'s `pub use`. Gating only the
  manifest leaves the module compiled and the dependency linked; gating `pub mod` but not the
  facade export breaks the default build.
- **F2 — mis-gated shared helpers break the default build (CONFIRMED dependency).** `computer.rs`
  imports `super::computer_row::{COMPUTER_COLUMNS, ComputerRow, computer}` and ~20 modules import
  `super::rows::…`. Marking either helper `#[cfg(test)]`, or gating it with a feature its consumers
  do not share, fails normal compilation, not just tests.
- **F3 — gating shifts the `missing_docs` surface (CONFIRMED logic, GUESS impact).** A gated-off
  `pub mod` removes its docs from the build, so a doc comment decaying behind a rarely-built feature
  escapes the crate-root lint; only `--all-features` CI sees it.

## Concrete hardening (with risks)

1. **H-E1/E3:** change `rows` to private (`mod rows;`) to match `computer_row`, and add a
   compile-time facade-completeness test enumerating every `pub mod` in `stores` and asserting a
   matching facade re-export exists. Safe now because `rows` has no `pub` items. Risk: a
   name-based test can drift from real type names — keep an explicit table reviewed alongside any
   module addition.
2. **H-E2:** split the two multi-type re-exports into per-type lines and doc-comment each facade
   item with its source module, so a dropped second type is visible in review. Risk: cosmetic
   `lib.rs` churn, no API change.
3. **H-E4:** reorder both lists identically (alphabetical, helpers last). Risk: large diff with no
   behavior change — keep it as its own commit so it cannot hide a real change.
4. **H-I1:** add a crate-private assertion that each re-exported `Postgres*Store` implements its
   `boma_app::ports::*Store` (`fn assert_port<T: ports::XStore>() {}`), and state in the barrel doc
   that `with_tenant` is mandatory. Risk: needs an explicit type→port map; default port methods
   still cannot prove a *body* uses `with_tenant`, so pair with a CI grep per store.
5. **H-I2:** keep `rekey_space_keys` off the crate root until config validation guarantees a stable
   key, gating it at the call site via `SecretBox::stable_key()`. Risk: moving a public item is a
   DEVIATION — record in `docs/PARITY.md` and seek approval; else keep the export and document the
   ordering invariant in rustdoc.
6. **H-F1:** if any feature appears, build `--no-default-features` and `--all-features` in CI and
   assert the same facade symbols are present in both. Risk: feature combinatorics multiply CI time
   — start with `default = []` and gate one adapter at a time.
7. **H-F2/F3:** keep `rows`/`computer_row` unconditionally compiled (private, no heavy deps) and
   run clippy/docs with `--all-features`. Risk: that may enable Docker-dependent tests needing a
   daemon — use `--no-run` or testcontainers per AGENTS.md.

## Suggested REQ-tagged tests

`req_043_every_public_store_module_has_a_facade_export`, `req_043_rows_helper_is_private`,
`req_043_each_postgres_store_implements_its_port`, `req_043_no_store_bypasses_with_tenant`,
`req_043_rekey_space_keys_requires_stable_key`, `req_043_all_store_modules_compile_under_all_features`.
