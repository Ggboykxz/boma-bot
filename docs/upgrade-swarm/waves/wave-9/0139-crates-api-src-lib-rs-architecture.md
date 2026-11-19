# v0.4.0 architecture proposal — `crates/api/src/lib.rs` (crate root / module exports)

Worker #139 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no product code changed).

Claim status: every "current state" statement is CONFIRMED by direct reads of `lib.rs`,
`crates/api/Cargo.toml`, `wire.rs`, `error.rs`, `router.rs`, the workspace `Cargo.toml`, and a
workspace-wide grep for `boma_api::` in this worktree. Forward-looking items are proposals (GUESS
until implemented and verified).

## Current structure (CONFIRMED)

- The file is 37 lines: a crate doc comment stating the layering rule (this crate alone knows HTTP
  status codes, calls `boma-app`, never `boma-infra`), `#![deny(missing_docs)]`, 18 flat `pub mod`
  declarations, then a flat list of `pub use` re-exports.
- All 18 modules are equally public: `auth`, `authcap`, `authroutes`, `automation`, `client`,
  `config`, `correlation`, `error`, `health`, `openapi`, `pages`, `public`, `realtime`, `router`,
  `rpc`, `screen`, `state`, `wire`. There is no `pub(crate)`, no `#[doc(hidden)]`, no grouping and
  no prelude. Handler plumbing is indistinguishable from intended API, both to readers and to the
  compiler.
- The root re-exports 14 names: `AuthContext`, `AuthCapabilities`, `SsoProvider`, `HttpConfig`,
  `HttpConfigError`, `ApiError`, `ErrorBody`, `ErrorCode`, `router`, `AppState`, and nine wire types
  (`BotReply`, `EventView`, `InvitationReply`, `MessageView`, `MessagesReply`, `RunView`,
  `RunsReply`, `SendReply`, `SpaceReply`).
- `wire.rs` defines **100+ public DTO types** (grep returned 100 matches, truncated). The nine
  re-exported are an arbitrary, stale subset: newer views such as `GroupMessageView`,
  `GroupGetReply`, `BoardTicketView`-family shapes live only under `boma_api::wire::*`, so a client
  cannot consistently import DTOs from the crate root.
- Every real external consumer needs only a thin facade (CONFIRMED by the 10 `boma_api::` call sites):
  `crates/bin` uses `AuthCapabilities`, `HttpConfig`, `HttpConfigError`, `AppState`, `router`;
  `crates/conformance` uses `AppState`, `AuthCapabilities`, `HttpConfig`, `router` and
  `boma_api::openapi::document()`; `crates/api/tests/*` use the same trio plus `AuthCapabilities`.
- `[workspace.lints.rust]` sets `unsafe_code`, `missing_docs`, `unused_must_use`, `rust_2018_idioms`
  — there is no `unreachable_pub`, so a `pub fn` inside an implementation module is not flagged.
- Several modules already carry good `//!` docs (`router.rs`, `error.rs`, `wire.rs`), so narrowing
  visibility does not require writing documentation from scratch.

## Guiding principle for v0.4.0

Make the crate root a deliberate, minimal, compiler-enforced facade: public = configuration,
principal, state, errors, DTOs, router and the OpenAPI document; everything else is an
implementation detail. Keep every existing external path working and change no wire byte.

## Upgrades (5 concrete items)

1. **Narrow module visibility to `pub(crate)`.** Keep `pub mod` only for `config`, `error`, `openapi`,
   `state` and `wire`; demote `auth`, `authcap`, `authroutes`, `automation`, `client`, `correlation`,
   `health`, `pages`, `public`, `realtime`, `rpc`, `screen` to `pub(crate) mod`. Re-exporting a public
   item from a private module is legal, so all 14 current root names and the
   `boma_api::openapi::document()` path survive unchanged.
   *Risk:* any consumer importing a demoted module path directly breaks. *Mitigation:* the grep
   above lists the finite set of call sites; compile-check `bin`, `conformance` and all `tests/`
   before landing. Do this as one mechanical commit with no behavior change.

2. **Turn the boundary into a lint: `unreachable_pub`.** Add it under `[workspace.lints.rust]`
   (or `#![deny(unreachable_pub)]` in `lib.rs`). After (1), any leftover `pub` item in a demoted
   module is a compile error, so the facade cannot silently widen again.
   *Risk:* this is a rustc lint, not clippy — `clippy.toml` allowances do not apply, and CI's
   `-D warnings` would fail on the first sweep. It will surface many items. *Mitigation:* land it at
   `warn` first, sweep `pub` → `pub(crate)` module by module, use
   `#[cfg_attr(test, allow(unreachable_pub))]` only on test-only modules, then flip to `deny`.
   Gate each step with `cargo clippy --workspace --all-targets -D warnings`.

3. **Replace the arbitrary DTO subset with a complete, coherent facade.** Keep `pub mod wire` and add
   `pub use wire::*;` so every view/reply is reachable from the root, plus a small `pub mod prelude`
   grouping the bootstrapping trio (`AppState`, `HttpConfig`, `router`) and the error envelope for
   new consumers. Delete the hand-maintained nine-name list.
   *Risk:* a glob re-export can collide with `error`/`config` names, widens rustdoc's public surface,
   and newly visible DTOs could be mistaken for a wire change. *Mitigation:* verify with
   `cargo doc --no-deps` and the conformance contract test; if collisions appear, fall back to an
   explicit `pub use wire::{...}` block generated from the source; assert golden + contract suites
   stay byte-identical. All re-exported items already satisfy `deny(missing_docs)`.

4. **Add a facade contract test (`crates/api/tests/public_api.rs`).** Bind each intended root export
   in compile-time assertions (`let _: fn(AppState) -> _ = router;`, type-annotated `use` of
   `ApiError`, `HttpConfig`, `AuthCapabilities`, a representative `wire::*` type) so accidental
   removal of a re-export fails the build. Add a second test that scans `crates/api/src/*.rs` for
   `boma_infra::` and fails if found, converting the doc comment's layering rule into a check.
   *Risk:* a snapshot test drifts when exports legitimately grow, and a source scan is brittle.
   *Mitigation:* make assertions additive (presence checks, never an exact-count snapshot); scan
   only the `boma_infra::` token, which comments spell as `boma-infra` and therefore never match.

5. **Make the root a real map, and freeze exports as additive-only for v0.4.0.** Rewrite the crate
   doc as a two-section map — "Public facade (consumers: bin, conformance, tests)" and "Internal
   modules (crate-only)" — naming the 10 external call sites so a future maintainer knows what a
   root change breaks. For v0.4.0, never remove a current root name: mark future removals
   `#[deprecated]` and record any visible change in `docs/PARITY.md` with owner approval, per
   AGENTS.md.
   *Risk:* additive-only leaves stale names; docs can drift. *Mitigation:* deprecation attributes
   keep the compiler warning, and test (4) keeps the documented facade honest.

## Cross-cutting risks / sequencing

- Sequence: (1) → (2) → (3) → (4) → (5). Lint enforcement only pays off after visibility is
  narrowed; the facade test only stabilises a decided export set.
- No wire-format, endpoint or CLI change is intended. If any step forces one, it is a DEVIATION in
  `docs/PARITY.md` and waits for the owner (AGENTS.md), not a silent refactor.
- Keep REQ ids in test names where these tests touch REQ behaviour (`tools/trace_check.py` counts
  declarations); the facade test itself is architectural and needs no REQ id.
- Verification gate for every step: `cargo fmt --check`, `cargo clippy --workspace --all-targets
  -D warnings`, `cargo test --workspace --locked`, plus the golden and OpenAPI contract suites.

## Out of scope for this module

The internals of authentication (`auth.rs`), routing (`router.rs`), the error envelope (`error.rs`)
and the DTO shapes (`wire.rs`) are covered by their own proposals; this one changes only how the
crate root exposes them. No dependency additions are proposed here.
