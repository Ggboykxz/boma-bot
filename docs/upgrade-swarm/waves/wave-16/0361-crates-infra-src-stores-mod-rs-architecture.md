# Worker #361 — `crates/infra/src/stores/mod.rs` — architecture (v0.4.0)

- Scope: the stores facade only (52-line `mod.rs` + its re-exports in `crates/infra/src/lib.rs`
  and the composition sites that consume them). Lens: architecture. READ-ONLY proposal; no code
  changed, no tests run. CONFIRMED = read this session; GUESS = design inference. No wire/API/data
  change proposed by default; anything externally visible is a DEVIATION needing `docs/PARITY.md`
  and owner approval (AGENTS.md).

## 1. Current facade (CONFIRMED)

- `mod.rs` is a bare list: 46 `pub mod` declarations plus one private `mod computer_row;`, and a
  4-line module doc asserting "one adapter per port family, all talking to Postgres" and that every
  tenant-data adapter goes through `crate::tenant` (REQ-043). No re-exports, no grouping, no facade
  type.
- The list is two alphabetically sorted runs: lines 6-21 end at `groups; signin;`, lines 23-52
  restart at `destinations;`. `signin` is out of global order (belongs between `sessions` and
  `skills`). Looks append-only; cause GUESS.
- `lib.rs:59-103` maintains a second, parallel list of ~44 `pub use stores::x::PostgresXStore;`
  lines, plus `pub use stores::secrets::{PostgresSecretStore, rekey_space_keys};`. Two lists, no
  check that they stay in sync.
- A plain `pub use stores::*;` at the crate root would collide with existing `pub mod computer`,
  `pub mod events`, `pub mod connectors` (`lib.rs:19,25,21`) — the facade cannot be flattened by
  glob today.
- `boma_app::Stores` (`crates/app/src/service.rs:25+`) is a 46-field struct with a positional
  `Stores::new(...)`. `crates/bin/src/commands.rs:174-224`, `crates/infra/tests/common/mod.rs`,
  `crates/infra/tests/m1.rs` and `computers.rs` each hand-write the same long chain of
  `Arc::new(PostgresXStore::new(pool.clone()))`; the facade exports no constructor for it.
- Secret injection is inconsistent at the composition site: vault-backed stores get
  `secrets_vault()` (`commands.rs:190,193,195,201,210`), but
  `PostgresNotificationDestinationStore` gets `SecretBox::ephemeral()` (`commands.rs:220-223`).
  GUESS: likely a stopgap; whether its ciphertext must survive restart needs a spec/STATE decision.
- Visibility is uneven: `rows.rs` is public with a "every read goes through one of these structs"
  convention (per sibling proposal #323), `computer_row.rs` is crate-private with its own typed
  row; `admin.rs` reads inline tuples. The facade documents none of this.
- `lib.rs:8-11` says stores are "one per port family, all Postgres" and `tenant` is the only place
  setting tenant context; sibling review #323 found `as_system` use inside `admin.rs`, so the
  facade doc understates the trust-boundary variety (two scopes, not one).

## 2. Architecture findings (CONFIRMED unless marked)

1. **Two sources of truth.** Module declarations and crate-root re-exports are maintained by hand
   in different files; adding a store can silently miss one half. GUESS: highest-value low-risk fix
   is to move all type re-exports into `stores/mod.rs` and keep one explicit line in `lib.rs`.
2. **Composition leakage.** The knowledge "which 46 adapters make a production `Stores`" lives in
   the binary and is copied into tests. That is application wiring logic sitting outside the
   adapter facade; drift there breaks boot or, worse, wires a test double into production.
3. **Hidden trust/secret decisions.** Tenant scope (`with_tenant` vs `as_system`) and vault choice
   (persistent vs ephemeral secret box) are chosen method-by-method and call-site-by-call-site; the
   facade offers neither a table nor a constructor that fixes them consistently.
4. **Row-mapping policy is implicit.** `rows.rs`, per-store `*_row.rs` and inline tuples coexist;
   a facade-level sentence plus a uniform private-module rule would prevent further drift.
5. **Doc contract is slightly wrong.** REQ-043 is about tenant context, but not every store method
   is tenant-scoped; the module doc should say "tenant data via `tenant`, deployment-wide data via
   `as_system`" rather than implying a single path.

## 3. Concrete upgrades for v0.4.0 (with risks)

**U1 — Canonicalize the declaration list (internal, do first).** Merge the two runs into one strict
alphabetical list, move `signin` after `sessions`, and extend the module doc with three short
sections: scope rule (REQ-043 tenant vs deployment), row-mapping rule (`rows.rs` shared, `*_row.rs`
private), and the fact that this module is the adapter inventory, not the composition root.
Risks: ordering-only, zero behavior; `cargo fmt --check` and clippy `-D warnings` still required;
do not rename a module — public paths (`boma_infra::stores::x`) must stay byte-identical.

**U2 — Single re-export surface.** Add one `pub use accounts::PostgresAccountStore; ...` block in
`stores/mod.rs`, then reduce `lib.rs:59-103` to `pub use stores::{PostgresAccountStore, ...};`
(types only). Risks: (a) **do not** use `pub use stores::*;` — name collisions with crate modules
`computer`, `events`, `connectors` (finding 2); (b) crate-root names must not change, or downstream
imports break (DEVIATION); (c) `rekey_space_keys` must remain exported; (d) check that no type name
clashes inside the `stores` namespace before merging.

**U3 — Add a facade constructor for the production `Stores`.** Define
`pub fn postgres_stores(pool: PgPool, secrets: &SecretBox) -> boma_app::Stores` in `stores/mod.rs`
(or an `infra::stores::bootstrap` submodule), moving the 46 `Arc::new(...)` lines out of
`commands.rs:174-224` into it; then use it in the binary and, where they want real adapters, in the
test harnesses. Risks: (a) `SecretBox` clone/ownership and the `secrets_vault()` error context must
be preserved; (b) the ephemeral destination-store choice must be decided explicitly — if the
facade makes it persistent, that is an externally visible DEVIATION; default to keeping the current
ephemeral behavior and record it as GUESS in `docs/STATE.md`; (c) tests that inject fakes/partial
`Stores` must keep `Stores::new` and must not be forced through the facade; (d) this adds an
infra→app dependency, already present (stores implement app ports), so no new crate edge.

**U4 — Make the trust boundary explicit and testable.** Correct the module doc to name both
`tenant::with_tenant` (tenant rows) and `tenant::as_system` (deployment-wide actions); add a table
in the doc listing which store families use which scope. Add one integration test that
`postgres_stores` yields all 46 ports and that a tenant-scoped call cannot see another space's rows
(REQ-043), naming it `req_043_facade_stores_respect_tenant_scope`. Risks: doc/test-only; no RLS or
`tenant.rs` change; do not assert on system-reason strings that sibling proposal #323 may change.

**U5 — Normalize row surfaces (only after U1/U2).** Keep `rows.rs` for cross-store mappers and make
every single-store row a documented `mod foo_row;` (private, typed `sqlx::FromRow` + one mapper);
port `admin.rs`'s inline tuples to `Admin*Row` in a separate change, coordinated with #323.
Risks: broad mechanical diff across files outside this module's lens — out of scope for this worker
and GUESS about maintainer appetite; each mapping must stay byte-identical to preserve error
strings and goldens. Defer beyond v0.4.0 unless U1/U2 are merged early.

## 4. Ordering, invariants, non-goals

Order: **U1 → U2 → U3 → U4**, U5 deferred. Invariants: public paths and crate-root type names
stable; REQ-043 tenant routing unchanged; no RLS/secret handling weakened; memory twins stay in
lockstep; each step its own Conventional Commit with the relevant REQ ids. Non-goals for this
worker: no source edits, no ADR/spec/gate edit, no dependency added, no change to `Stores::new`'s
signature, no port split.

## 5. Evidence & tests

- CONFIRMED — `stores/mod.rs:1-52`; `crates/infra/src/lib.rs:17-38,59-104`;
  `crates/app/src/service.rs:1-75`; `crates/bin/src/commands.rs:174-224`;
  `crates/infra/tests/common/mod.rs`, `tests/m1.rs`, `tests/computers.rs`; `stores/accounts.rs:1-30`;
  `stores/computer_row.rs:1-50`; sibling wave-14/16 architecture proposals.
- GUESS — why the list is split; whether the ephemeral destination vault is intentional; how much
  row-mapping cleanup is wanted in v0.4.0; whether CI pins offline SQLx metadata (relevant only if
  a later change edits queries).
- Suggested test names: `req_043_facade_stores_respect_tenant_scope`,
  `req_043_facade_constructor_wires_every_port`.

Line budget: <120. Status: proposal only; READ-ONLY worker, no code changed and no tests run.
