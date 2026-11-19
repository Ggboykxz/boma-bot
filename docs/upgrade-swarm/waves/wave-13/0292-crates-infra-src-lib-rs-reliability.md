# Worker #292 — `crates/infra/src/lib.rs` — Reliability (v0.4.0)

Scope: reliability hardening of the `boma-infra` crate root (module declarations, re-exports and
its role as adapter assembly point). Evidence is static inspection of `lib.rs`, `Cargo.toml`,
`db.rs`, `tenant.rs`, `stores/mod.rs` and `docs/STATE.md`; no runtime test was run by this worker.
Claims are CONFIRMED unless marked GUESS. Any externally visible change needs a `docs/PARITY.md`
DEVIATION awaiting owner approval.

## Failure mode 1 — No feature gates: every adapter compiles and links unconditionally

Evidence (CONFIRMED): `Cargo.toml` l.15-41 has **no `[features]` section**; heavy dependencies
`bollard 0.21`, `tokio-tungstenite`, `reqwest`, `aes-gcm`, `getrandom` and `url` are all
unconditional. `lib.rs` l.15-38 declares all 22 modules with no `#[cfg]`. Consequences: a defect,
CVE or build break in any single adapter or its transitive tree blocks *every* binary —
`boma migrate`, the automation CLI, `serve` and `worker` alike; there is no core-only build for
low-end devices (quality-bar criterion 4) and the runtime attack surface is always maximal.
`#![deny(missing_docs)]` (l.13) is crate-wide, so gated modules must keep doc coverage in every
combination.

Hardening:
- Introduce additive features with everything shipped today left **on by default**:
  `computer` (bollard), `mcp-stdio`/`cdp` (tokio-tungstenite), `providers` (reqwest/openai/oauth/
  stripe), `secrets` (aes-gcm/getrandom), `plugins` (GitHub fetcher). Gate both the `mod`/`pub mod`
  declaration and its matching `pub use` line together.
- Make optional dependencies `optional = true` with `dep:` feature syntax; keep versions in
  workspace deps and record in `docs/DEPENDENCIES.md` which feature pulls each one.
- Add a CI feature matrix: `cargo check --no-default-features` plus each single-feature build, with
  clippy and doc tests, so a half-gated module cannot pass locally on default features only.
- Add a compile-fail test proving a gated type is unreachable without its feature (guards leakage).

Risks: cfg on declarations *and* re-exports is easy to get half-right — a binary builds on a
developer machine and fails in a minimal CI job; the matrix multiplies CI time. A gated module
imported unconditionally by another module causes cfg-spread. Default-on preserves parity;
dropping anything from default is an externally visible build-interface change → DEVIATION. Run
cargo-deny per feature to confirm no gate reintroduces a banned/advisory transitive.

## Failure mode 2 — No explicit init-ordering contract at the crate root

Evidence (CONFIRMED): `lib.rs` is purely declarative — no `init`/bootstrap function — so the
config → pool → migrate → store/service → worker sequence is reconstructed by hand at each call
site (bin, testkit, api). `db.rs` offers eager `connect` (l.41), lazy `connect_lazy` (l.56) and
`migrate` (l.72), while `tenant::with_tenant` (l.51) assumes a live pool whose `search_path` was
pinned at connection time (db.rs l.94). `docs/STATE.md` records a *real* boot-chain failure of
exactly this shape on Windows: startup validated `DOCKER_HOST` even with `SANDBOX_PROVIDER=none`,
and a double-suffix health probe tore Postgres down 90 s later — both ordering/validation defects.

Hardening:
- Add a documented `Infra::init(config)` sequence in `lib.rs` (or a new `init` module re-exported
  here): (1) validate typed config, fail fast → (2) build pool (`connect_lazy` for `serve`) →
  (3) apply migrations where the binary owns them → (4) register readiness checks → (5) construct
  stores/services → (6) start workers. Make invalid transitions unrepresentable with a staged
  builder (`Unconfigured → PoolReady → Migrated → Ready`).
- Gate store construction / first tenant query on a `Migrated` marker, or assert schema version in
  readiness, so "query before migrate" cannot silently happen against a half-initialised schema.
- Document the `connect` vs `connect_lazy` choice at each call site and add a fresh-database boot
  integration test that asserts migrate-before-first-query and "DB down → 503, not crash loop".

Risks: a staged builder adds API surface and must track every new adapter — the same drift problem
as mode 3. Forcing migrations into the boot path can slow or break read-only/HA deployments that
migrate out of band; offer an explicit `skip_migrate` backed by a schema-version check, never a
silent bypass. Changing boot order changes startup logs/timing → DEVIATION if observable.

## Failure mode 3 — Export drift between module declarations and re-exports

Evidence (CONFIRMED structure, impact GUESS): two hand-maintained parallel lists exist — `pub mod`
declarations in `stores/mod.rs` (l.6-52) and 60+ `pub use` lines in `lib.rs` (l.40-104). Both are
split at arbitrary points (lib.rs l.75, stores/mod.rs l.22) with no grouping rule, and
`rekey_space_keys` is a function re-exported beside a type (l.96). Nothing mechanically guarantees
that every intended adapter is exported exactly once, that a private module (e.g. `space_keys`,
l.15) never leaks, or that a removed module's re-export is cleaned up. A stale/duplicate re-export
breaks downstream compiles; an accidental one widens the public surface and, under
`deny(missing_docs)`, can break docs CI.

Hardening:
- Group the re-export block by family (stores / services / tenant / config+errors), mirroring the
  crate doc at l.6-11; delete the arbitrary blank-line splits so additions have an obvious home.
- Add an API-surface golden (rustdoc-JSON or `cargo public-api` snapshot under `tests/`) asserting
  the exact exported-name set; CI fails on any delta until the snapshot is reviewed and the change
  is recorded in `docs/PARITY.md`.
- Add a CI grep guard, like ADR-002's tenant check: every `pub mod` in `stores/mod.rs` has a
  matching `pub use` in `lib.rs` and vice versa (with an explicit exception list).
- Keep `#![deny(missing_docs)]` and add `#![deny(unreachable_pub)]` to catch private-module leakage.

Risks: an API snapshot is deliberate maintenance on every legitimate addition — keep the review
cheap or people will disable the gate. `unreachable_pub` may surface existing warnings in
submodules; fix them or allow narrowly with a comment. A strict one-to-one mod↔use rule is wrong
for intentional cases (`space_keys` private; multi-type modules `cards`, `secrets`) — encode those
as explicit exceptions rather than weakening the check. Public-surface changes are externally
visible → DEVIATION per AGENTS.md.

## Priority for v0.4.0

1. Export-surface golden + grep guard + regrouped re-exports (cheap, no runtime behaviour change).
2. Documented init sequence, staged bootstrap and the boot-order integration test (prevents a
   repeat of the recorded Windows boot failure).
3. Additive feature gates with the CI matrix, all default-on for parity.

Each item needs REQ-tagged tests (module-level `req_*` names) and, for any externally visible
change, a `docs/PARITY.md` DEVIATION awaiting approval.
