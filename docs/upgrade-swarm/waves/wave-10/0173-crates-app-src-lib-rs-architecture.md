# v0.4.0 architecture proposal — `crates/app/src/lib.rs` (crate root / module exports)

Worker #173 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no code changed).

Status of claims: every "current state" statement below is CONFIRMED by direct reads of
`crates/app/src/lib.rs`, the 14 module files, `crates/app/Cargo.toml`, the workspace `Cargo.toml`
(`version = "0.3.0-rc5"`, `edition = 2024`) and `grep`s of `boma_app::` usage across sibling crates
(`api`, `infra`, `bin`, `conformance`, `testkit`) in this worktree. Forward-looking items are
proposals (GUESS until implemented and verified).

## Current structure (CONFIRMED)

- `lib.rs` is 29 lines: a crate doc plus `#![deny(missing_docs)]`, 14 flat `pub mod`
  declarations (lines 10-23) and 4 `pub use` lines (25-29) re-exporting 16 symbols at the root.
  Every module is public; there is no `pub(crate)`, no feature gate, no `prelude`, and no
  `#![deny(unreachable_pub)]`.
- Module weight is extremely asymmetric: `usecases.rs` is ~351 KB / 232 public items / 168
  top-level `pub fn`s; `ports.rs` ~151 KB / 61 `pub trait`s / 160 public items; everything else is
  small (`worker` 34 KB, `service` 25 KB, `account_export` 26 KB, `push` 18 KB, `acp`/`acp_provider`
  ~12 KB each, `accounts` 14 KB, `health` 8 KB, `agui_provider` 7 KB, `local_models` 5 KB,
  `metrics` 4 KB, `error` 3.5 KB). Two grab-bag modules hold ~85% of the public API.
- The root re-exports are mostly dead: across sibling crates only `AppError` (162 uses), `App`
  (10), `Stores` (9), `Services` (7) and `Capabilities` (3) are used as `boma_app::<sym>`.
  `Worker`, `Actor`, `Accounts`, `SessionGrant`, `SignIn`, `SignUp`, `SendOutcome`, `Failure`,
  `BotWithThread`, `Readiness`, `ReadinessCheck`, `ReadinessReport` have **zero** root-path uses.
- Consumers instead go through module paths: `boma_app::usecases` (643 uses), `boma_app::ports`
  (489), `push` (32), `worker` (27), `account_export` (13), `acp_provider` (12). Modules
  `acp`, `agui_provider`, `metrics` and `health` (as a module) are never referenced externally —
  they are implementation details that the flat `pub mod` list exposes as public API anyway.
- Symbols are exported twice: `SignIn`/`SignUp` are re-exported from `accounts` both at the root
  (lib.rs:25) and inside `usecases` (`usecases.rs:25`); `request_password_reset`/`reset_password`
  are likewise re-exported in `usecases.rs:60`. The `sign_in` orchestration fn (usecases.rs:67) is
  distinct from the `SignIn` input struct (accounts.rs:31), so the namespace mixes two layers.
- The crate doc promises "it never names a concrete adapter", yet `Cargo.toml` pulls in `reqwest`,
  `web-push`, `zip`, `base64`, `hmac`, `sha2`, and modules `acp`/`acp_provider`/`agui_provider`
  implement concrete wire protocols. The stated boundary and the actual dependency graph disagree.

## Guiding principle for v0.4.0

Turn the crate root into a *deliberate, small facade*: keep a stable public surface that consumers
already use, make internals private, split the two giant modules along cohesive seams without
breaking existing `boma_app::usecases::*` / `boma_app::ports::*` paths, and enforce the
"no concrete adapter in `app`" claim with lints and an ADR-backed dependency move.

## Upgrades (5 concrete items)

1. **Shrink and document the public root.** Keep the genuinely used facade (`AppError`, `App`,
   `Services`, `Stores`, `Capabilities`) plus typed error/principal exports, group them in a
   `pub mod prelude`, and make the zero-use re-exports either `pub(crate)` or deprecated aliases.
   Add `#![deny(unreachable_pub)]` and a rustdoc module map in the crate doc so ownership is
   explicit. *Risk:* downstream breakage if a symbol is used outside the scanned crates (e.g. a
   future `testkit` path). *Mitigation:* the scan shows 0 uses; keep deprecated `pub use` aliases
   for one release and run `cargo check --workspace --all-targets` after pruning.

2. **Split `usecases.rs` behind a stable `usecases` facade.** Move the 168 fns into cohesive
   submodules (`usecases::accounts`, `::bots`, `::messaging`, `::runs`, `::pages`, `::connectors`,
   `::admin`, …) and re-export them from `usecases.rs` so `boma_app::usecases::<name>` keeps
   resolving. Do one area per commit. *Risk:* a 351 KB mechanical move is easy to get half-right;
   intra-module private helpers and doc links (`[`fn`]`) can break. *Mitigation:* compile-check each
   slice, keep a `pub use area::*` facade, and let rustdoc catch broken intra-doc links.

3. **Split `ports.rs` by responsibility.** Group the 61 traits into `ports::stores`,
   `ports::providers`, `ports::gateways`, `ports::primitives` (Clock/IdGenerator/TokenGenerator/
   PasswordHasher) behind the existing `ports` facade. *Risk:* trait-method cross-references and
   `use crate::ports::*` globs in `service.rs`/`usecases.rs` may create ambiguity or cycles.
   *Mitigation:* move definitions, not signatures; keep one `pub use` block; verify with clippy
   `-D warnings` and the `testkit` fakes that implement these traits.

4. **Enforce the adapter-free boundary.** Move the concrete protocol clients (`acp*`,
   `agui_provider`, `push`, `zip` export, `reqwest`/`web-push` deps) behind their ports into
   `infra`, leaving `app` with use cases + traits only, matching its own crate doc. *Risk:* largest
   blast radius — port signatures, error mapping and test fakes may need changes, and any wire
   behaviour is a potential DEVIATION. *Mitigation:* stage per adapter, add an ADR for the
   dependency relocation (AGENTS.md requires `cargo add` + docs.rs checks), keep `ModelProvider`
   and friends as the seam, and record any externally visible change in `docs/PARITY.md`.

5. **Remove the duplicate export paths.** Give `SignIn`/`SignUp` (and the password-reset helpers)
   one canonical home, with a single re-export; make `usecases` and the root point at it. *Risk:*
   two import paths currently coexist, so a consumer could depend on either. *Mitigation:* both
   paths have non-zero use only via `usecases`, so keep `usecases::*` stable, deprecate the root
   copy, and keep REQ ids (040/041) in test names for `tools/trace_check.py`.

## Cross-cutting risks / sequencing

- Sequence 1 → 3 → 2 → 5 → 4: the lint/facade setup first, then the lower-risk `ports` split,
  then the larger `usecases` split, de-duplication, and finally the dependency move (which needs its
  own ADR and approval).
- Only item 4 can change externally visible behaviour; treat any such change as a DEVIATION in
  `docs/PARITY.md` and wait for approval. Items 1-5 are otherwise import-path-internal.
- Gate every step with `cargo fmt --check`, `clippy -D warnings`, unit + `tests/` integration, and
  the golden/contract suites; add doc tests for the new `prelude` and module map.
- `#![deny(missing_docs)]` must remain in force — every newly public facade item needs rustdoc.

## Out of scope for this module

Business logic inside the use cases, the shape of individual ports, and HTTP/DTO mapping (`api`
crate) are out of scope; this proposal only restructures the crate root and its module/export
surface.
