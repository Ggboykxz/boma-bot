# Worker 0291 — `crates/infra/src/lib.rs` architecture proposal (v0.4.0)

Lens: architecture. Scope: read-only review of the 104-line crate root, plus targeted reads of
`crates/infra/Cargo.toml`, `crates/infra/src/stores/mod.rs` (52 lines), `stores/rows.rs`, and a grep
for `net_safety` across `crates/infra/src/`. No product code changed; no tests run (read-only task).
"Current" claims are **CONFIRMED by named source**; forward-looking items are **GUESS** until
implemented, tested and approved.

## Current export structure (CONFIRMED)

- `:13` `#![deny(missing_docs)]`, workspace lints via `Cargo.toml:12-13`. Good baseline — do not
  weaken it.
- The root is a **flat facade**: one private `mod space_keys;` (`:15`), **22 `pub mod` declarations**
  (`:17-38`) and **64 `pub use` statements** (`:40-104`) that flatten adapter types into the crate
  root. Callers write `boma_infra::PostgresUserStore`, `boma_infra::HttpModelTransport`, etc.
- **Full store coverage:** 44 of the 64 re-exports (`:59-103`) mirror the 44 `pub mod` store
  declarations in `stores/mod.rs:6-52`. `storage` is not the gap.
- **The one real gap:** `net_safety` (`:31`) is the *only* public module with no root re-export. A
  grep finds exactly four in-crate callers, all namespaced: `connectors.rs:13`,
  `mcp_http.rs:15`, `endpoint_safety.rs:13`, `messaging_http.rs:95` (`crate::net_safety::...`).
  This is an internal SSRF guard leaking into the public module list, and it leaves two import
  spellings in the crate: flattened (21 modules) and namespaced (this one). Same defect class as
  the domain-root gap in wave-12/0237.
- **Stale overview:** the crate doc (`:1-11`) promises "three families" but names only
  `stores`/`tenant`/`crypto`/`provider`/`events`; 16 of the 22 modules are undocumented in it.
- **Vestigial grouping:** a stray blank line splits the store re-exports (`:75`) and the
  `stores/mod.rs` declarations (`:22`); `stores/mod.rs:42` declares `pub mod rows;` although
  `rows.rs` exposes only `pub(crate)` items (its sibling `computer_row` is correctly private at
  `:16`). Both are leaks in the declared surface.
- `Cargo.toml` has **no `[features]` section**: `bollard 0.21`, `tokio-tungstenite`, `reqwest`,
  `aes-gcm`, `argon2` are always compiled, even for callers that use only the pure services.
- Keeping both `boma_infra::PostgresUserStore` and `boma_infra::stores::users::PostgresUserStore`
  working is a parity constraint: do not remove either without a `docs/PARITY.md` entry.

## Guiding principle for v0.4.0

Keep the flat facade deliberately (it matches `boma-domain`, is ergonomic, and is the majority
convention); make the convention *consistent and documented* rather than replacing it. Prefer
**additive** changes — additions and doc edits are not DEVIATIONS; removing or renaming a public
path is (record it in `docs/PARITY.md` and wait for approval). Keep `lib.rs` declaration-only.

## Target structure (conceptual grouping via labelled comment blocks — no new subdirectories)

```
infra/src/lib.rs     crate docs + lints + module decls + 4 labelled, alphabetised re-export blocks
  ├─ core & config    artifact_files, cdp, computer, config, connectors, crypto, db, dispatch,
  │                   endpoint_safety, openai, plugin, provider, secret_box
  ├─ transports       events, mcp_http, mcp_stdio, messaging_http, model_http, oauth
  ├─ stores           unchanged 44-module facade in stores/mod.rs; root allow-list re-exports
  └─ tenant           REQ-043 context: TenantFuture, with_tenant
infra/src/net_safety.rs  internal pub(crate) helper (or root re-export if promoted to public API)
infra/Cargo.toml     [features] with a `default` set that preserves today's full surface
```

## Concrete upgrades (5, with risks)

### U1 — Resolve the `net_safety` gap and state the facade rule (do this first)

Preferred: demote `:31` to `pub(crate) mod net_safety;` — all four callers are in-crate (CONFIRMED),
and it is a security helper with no reason to be public API. Add one sentence to the crate doc:
"every public module is re-exported at the root except explicitly crate-private helpers."
*Risk:* changing `pub mod` → `pub(crate) mod` removes a public path; even though no in-workspace
caller uses it, that is formally a DEVIATION — log it and wait for approval, or take the additive
fallback `pub use net_safety::{check_outbound_url, NetSafetyError};` if an external consumer exists.
The fallback widens an already broad root namespace, so prefer demotion. *Benefit:* one import
convention, and the crate's public surface matches its documented intent.

### U2 — Rewrite the crate docs and guard intra-doc links

Replace the "three families" paragraph with an accurate map of all 22 modules across the four
groups above, with working [`...`] intra-doc links. Add `#![deny(rustdoc::broken_intra_doc_links)]`
next to the `missing_docs` deny so stale links fail CI instead of rotting.
*Risk:* deny-level link checking may surface pre-existing broken links in sibling module docs; run
`cargo doc -p boma-infra --no-deps` first and fix them in the same commit. Docs-only; no wire, API
or serialized-format change. Mark items CONFIRMED/GUESS per the workspace rule.

### U3 — Tidy the re-export block and close the small surface leaks

Sort the 64 re-exports into the four labelled blocks, remove the stray blank line at `:75` (and the
one at `stores/mod.rs:22`), and change `stores/mod.rs:42` to `mod rows;` (all items are `pub(crate)`,
CONFIRMED). Keep every existing name and both import spellings intact.
*Risk:* purely mechanical; a dropped `pub use` breaks callers at compile time, so verify with a
compile-only smoke test (U5) and `cargo check --workspace`. `rows` is unreachable outside the crate
already, so making it private changes no usable API. Large diff, zero behavior change — keep it as
one dedicated commit to preserve reviewability.

### U4 — Gate heavyweight adapters behind Cargo features

Add `[features]` with `default = ["docker", "websocket", "http-providers", "crypto"]` mapping to
`bollard`, `tokio-tungstenite`, `reqwest`-based transport/provider adapters and `argon2`/`aes-gcm`
respectively; cfg-gate the corresponding `pub mod` + `pub use` pairs in `lib.rs`. With all features
default-on, today's surface is unchanged (not a DEVIATION); turning features off is opt-in.
*Risk:* cfg-gating a module removes its root names under reduced builds, so CI must check both
`--all-features` and `--no-default-features` plus the real consumption combination; feature
combinations can hide type errors for months otherwise. Also use this pass to re-verify the pinned
non-workspace versions (`bollard 0.21`, `reqwest 0.13.5`, `secrecy 0.10.3`, `aes-gcm 0.11.1`,
`getrandom 0.4.3`) against the registry — do not assume they resolve. Requires an ADR only if
defaults ever stop preserving the current public surface.

### U5 — Add a compile-time export-contract test

Add a `#[cfg(test)]` module in `lib.rs` that names every re-exported type once (e.g.
`let _: Option<boma_infra::PostgresUserStore> = None;`) so a future accidental deletion fails the
build, plus a small `tools/` grep check in CI asserting "no `pub mod` without a matching root
re-export or an `/// internal` marker". Extend the existing store smoke coverage rather than
duplicating per-module tests.
*Risk:* a hand-maintained list can drift and is noisy to update; keep it generated from the
re-export block if practical, and never let the check weaken or replace an existing traceable test
(`tools/trace_check.py` counts declaration lines). Dev/CI-only, no runtime cost.

## Sequencing / cross-cutting risks

1. U1 + U2 first: small, doc-and-declaration-only, and they fix the two documented defects.
2. U3 next as one mechanical commit; U5 can ride alongside to lock the surface before refactoring.
3. U4 last — it touches `Cargo.toml`, CI and every feature-gated import; land with dual CI jobs.
4. After each step: `cargo fmt --check`, `cargo clippy --workspace -D warnings`,
   `cargo check --workspace`, `cargo doc -p boma-infra --no-deps`, and the workspace test suite.
5. No UI/i18n impact. All items preserve serialized formats, DB schema and wire behavior; any
   public-path removal (U1 demotion) must be approved in `docs/PARITY.md` before shipping.
