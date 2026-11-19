# v0.4.0 architecture proposal — `crates/domain/src/lib.rs` (crate root & module exports)

Worker #237 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no product code changed).

Status of claims: every "current state" statement below is CONFIRMED by direct reads in this
worktree of `crates/domain/src/lib.rs`, all 32 sibling module files and `crates/domain/Cargo.toml`.
Forward-looking items are proposals (GUESS until implemented, tested and approved).

## Current export structure (CONFIRMED)

- `lib.rs:10` sets `#![deny(missing_docs)]`; workspace lints apply via `Cargo.toml [lints]`. Good
  baseline — do not weaken it.
- The root is a **flat facade**: 32 `pub mod` declarations (`:12-43`) plus 27 `pub use` groups
  (`:45-111`) that flatten most items into the crate root. Callers write `boma_domain::Bot`,
  `boma_domain::RunStatus`, `boma_domain::validate_page_size` — one namespace, no grouping.
- **Coverage gap:** five modules are declared public but have *no* root re-export:
  `acp`, `compaction`, `external`, `notification`, `signin` (all improvement-program rules:
  REQ-059/060/061/064/069). Two import styles therefore coexist: flattened (`boma_domain::Bot`)
  and namespaced (`boma_domain::acp::AcpAgent`). Newer code is not following the established
  convention — this is the central architecture defect in v0.3.0-rc5.
- Those five modules reach back through the facade hub: `use super::{BotId, SpaceId}` in
  `acp.rs:8`, `external.rs:3`, `signin.rs:3`, `notification.rs:3`. Siblings disagree on the same
  question: `card.rs:12` imports `crate::{RunId, SpaceId, ThreadId}` (root), while `computer.rs:13`
  uses `crate::ids::{BotId, SpaceId}` and `error.rs:4` uses `crate::run::RunStatus` (defining
  modules). The root is effectively a cycle hub (root → module → root) even though Rust compiles it.
- `opaque_id!` is `#[macro_export]` in `ids.rs:9`, so it is a *public crate-root API item invisible
  from lib.rs*. Its expansion names `DomainError`, `Serialize` and `Deserialize` unqualified; the
  only in-workspace callers (`card.rs:15`, `computer.rs:15`) must import all three or expansion
  fails. That is an implicit hygiene contract, not an enforced one.
- Two free functions sit in the root: `percent_encode` (`:118`, RFC-3986 rule) and `format_webhook`
  (`:132`, documented as used by "the store's inline shape check" — an infra concern leaking into
  the pure facade). The only root-level test is `req_033_...` (`:137-149`).
- **Purity holds today:** grep found no `::now()`, `SystemTime`, `std::fs`, `std::net`, `tokio` or
  `sqlx` in `crates/domain/src/`. But `Cargo.toml` enables `chrono`'s `clock` feature in a crate
  documented as "no IO, no clock" — a latent purity regression waiting to happen.

## Guiding principle for v0.4.0

Keep the flat facade deliberately (it is ergonomic and already the majority convention); make the
convention *consistent* rather than replacing it. Prefer additive changes: `pub use` additions and
internal import rewrites are not DEVIATIONS, while removing or renaming any public path is — record
it in `docs/PARITY.md` and wait for approval. Keep `lib.rs` declaration-only.

## Upgrades (5 concrete items)

1. **Close the re-export gap for the five namespaced modules.** Add root `pub use` groups for
   `acp`, `compaction`, `external`, `notification` and `signin` so every public module follows the
   same facade rule; keep the module paths intact (`pub mod` stays) so both spellings work.
   *Risk:* a name collision (e.g. two `Status`-style types) would be a compile error, and the wider
   root namespace could shadow a caller's import. *Mitigation:* let `cargo check` enumerate any
   collision; the change is purely additive, so no wire/API breakage. Alternative — namespace *all*
   modules instead — breaks every existing caller and is a DEVIATION requiring approval.

2. **One internal import convention.** In all submodules import types from their *defining* module
   (`crate::ids::{BotId, ...}`, `crate::run::RunStatus`), never from the root facade; drop the
   `use super::{BotId, SpaceId}` hub imports in `acp/external/signin/notification`. Leave
   `use super::*` inside `#[cfg(test)]` blocks alone.
   *Risk:* mechanical but touches 4+ files; a missed import produces a confusing cycle error.
   *Mitigation:* `cargo check -p boma-domain` + `clippy --workspace -D warnings`; zero public API
   change. Benefit: `ids` becomes relocatable and the root stops being a dependency cycle hub.

3. **Make `opaque_id!` hygienic and self-documenting.** Qualify expansions as
   `$crate::error::DomainError` and `serde::{Serialize, Deserialize}` paths, preserving the macro
   name and call syntax. Add a doc comment on the macro in `ids.rs` stating it is exported at the
   crate root. Optionally list it explicitly in a `lib.rs` comment for discoverability.
   *Risk:* the macro is public; a changed expansion could break an out-of-tree caller that relies
   on importing `DomainError` itself — but in-workspace grep shows only `card.rs`/`computer.rs` use
   it. *Mitigation:* add a compile-only test that invokes `crate::opaque_id!` in a module without
   `DomainError` in scope; confirm with `cargo test -p boma-domain`.

4. **Enforce the "no clock" purity claim.** Drop the `clock` feature from `chrono` in
   `crates/domain/Cargo.toml` (keep only the date/time *types* the rules need) and add a CI guard
   that fails if domain sources introduce `::now()`, `SystemTime`, `std::fs`, `std::net`, `tokio`
   or `sqlx`. Document that the app layer passes "now" into domain functions as a parameter.
   *Risk:* a rule may currently rely on `chrono::Local/Utc` from the `clock` feature; grep says it
   does not (CONFIRMED), so the risk is low. *Mitigation:* diff `cargo tree -p boma-domain -e
   features` before/after and run the full test suite.

5. **Reduce `lib.rs` to declarations + a real utility module.** Move `percent_encode` and
   `format_webhook` (and the `req_033` test) out of the root into a dedicated `text`/`sanitize`
   module, then re-export both from `lib.rs` so `boma_domain::percent_encode` still compiles.
   Reconsider whether `format_webhook` belongs in the pure domain at all ("store's inline shape
   check" suggests boundary normalization).
   *Risk:* changing the defining module alters rustdoc paths for contributors; moving
   `format_webhook` to `app`/`api` would be a public-path removal (DEVIATION). *Mitigation:* keep
   the root `pub use` alias and the `req_033_` test name intact so traceability is preserved.

## Verification checklist for whoever implements

`cargo fmt --check` · `cargo clippy --workspace -D warnings` · `cargo test -p boma-domain` ·
`cargo tree -p boma-domain -e features` diff (item 4) · `tools/trace_check.py` still sees `req_033`.
No item changes serialized formats or wire behavior; all API-visible additions must be logged as
non-breaking in `docs/PARITY.md`.
