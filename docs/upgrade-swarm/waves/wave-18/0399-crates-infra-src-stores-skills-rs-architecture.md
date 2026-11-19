# v0.4.0 architecture proposal — `crates/infra/src/stores/skills.rs`

- Worker: 399 · Lens: architecture · Scope: `PostgresSkillStore` (infra adapter for `SkillStore`, REQ-035)
- Status: PROPOSAL. Code facts marked CONFIRMED are from reading the module, `tenant.rs`, `ports.rs` (2100–2196), `plugin_installs.rs`, and `infra/tests/m1.rs` in this tree. No tests were run by this worker.

## 1. Current structure (CONFIRMED)

- One struct `PostgresSkillStore { pool: PgPool }`, `const fn new`, `#[async_trait]` impl of the four `SkillStore` methods: `list`, `get`, `put`, `remove`.
- Every method wraps its SQL in `tenant::with_tenant(...)`, which opens a transaction, sets `boma.space_id` / `boma.system_reason` with `is_local = true`, and relies on RLS policies for isolation. The store issues **no explicit `space_id` predicate** except `put`'s `INSERT`.
- `list`/`get` use positional `query_as::<_, (String, …)>` tuples and hand-map them into `SkillSummary`/`SkillRecord`; both format time with the DB-side function `boma.rfc3339(created_at)` into `String` fields.
- `put` is a single `INSERT … ON CONFLICT (space_id, name) DO UPDATE`; it rejects `TenantContext::System` with `NotFound` and binds `space_id` from the member context. `created_at` comes from the caller, `updated_at = now()` from the DB.
- `remove` is `DELETE FROM skills WHERE name = $1` and returns `rows_affected() > 0`.
- A fifth write against `skills` lives in a different adapter: `PostgresPluginInstallStore::remove_plugin_skills` (`plugin_installs.rs:104`) issues `DELETE FROM skills WHERE origin='plugin' AND plugin_source=$1`.
- Integration coverage (`m1.rs:1892`) checks round trip, RLS isolation and taught-skill replacement; it does **not** check the plugin-conflict rule declared in the port.

## 2. Concrete upgrades

### U1 — Uniform, defensive tenant scoping (CONFIRMED asymmetry)
Change: extract `space_id` once in every method and add `AND space_id = $n` to `list`, `get`, `remove` (keeping RLS as the second barrier). Reject `TenantContext::System` at the top of *all* four methods, as `put` already does and as `MemorySkillStore` does for `list`/`get`.
Rationale: `with_tenant` maps System to `as_system`, which lifts RLS; today a System-context `remove(name)` can delete that name in every space and `list` can read every space's skills. The Postgres and memory adapters disagree on System behaviour.
Risk: LOW–MED. The explicit predicate is redundant under RLS but must use the same space id. GUESS: no current caller passes System to skills methods; verify with a grep over `usecases.rs`/`service.rs` before changing, then add a cross-tenant integration test so the gate is test-enforced.

### U2 — Honour the port's `Conflict` contract (CONFIRMED divergence)
Change: make `put` refuse to overwrite a plugin skill with a different origin (a matching plugin reinstall still replaces). Prefer one statement — e.g. `INSERT … ON CONFLICT … DO UPDATE SET … WHERE skills.origin = EXCLUDED.origin OR skills.origin <> 'plugin'` — or a `SELECT … FOR UPDATE` inside the existing transaction; return `AppError::Conflict` on refusal.
Rationale: `ports.rs:2186` promises Conflict for a same-name plugin skill, and `MemorySkillStore` enforces it; the Postgres adapter silently upserts, so a taught skill can replace a plugin skill and the two adapters disagree.
Risk: MED. Must stay atomic (the transaction is the natural unit); check-then-insert without a lock is racy. Externally visible error change: record in `docs/PARITY.md` as a contract fix and get approval before shipping. Add REQ-035-named integration tests for both adapters.

### U3 — Replace positional tuples with `FromRow` row structs (CONFIRMED fragility)
Change: define private `SkillListRow` / `SkillRecordRow` with `#[derive(sqlx::FromRow)]`, select explicit aliased columns, and map to the port types in one small `impl From<…>` pair. Keep `boma.rfc3339` out of the SQL if U4 lands.
Rationale: four adjacent `String` columns (`name`, `description`, `origin`, `plugin_source`) are interchangeable to the tuple decoder, so a column reorder compiles and silently corrupts data; field-name binding removes that class of bug and localises future column changes.
Risk: LOW. Internal only; aliases must match field names exactly, and offline SQLx metadata must be refreshed after the query change. Verify with the existing round-trip test.

### U4 — One clock, one timestamp formatter (CONFIRMED mixed sources)
Change: use a single time source for both `created_at` and `updated_at` (`statement_timestamp()` or the caller's value for both); select raw `timestamptz` and format with one Rust RFC3339 helper instead of the `boma.rfc3339` DB function. Longer term propose typed `DateTime<Utc>` in the port types.
Rationale: the row currently mixes app-supplied and DB-supplied clocks, and reads depend on a schema function that must exist in every migration environment; the stringly-typed port lets malformed timestamps travel.
Risk: MED. The Rust formatter must be byte-identical to the DB function or golden/wire tests break — pin this with a test comparing both during the transition. Changing port field types is a DEVIATION requiring approval; do not bundle it into v0.4.0.

### U5 — Co-locate skill writes and add bounded reads + tracing (CONFIRMED split)
Change: move the `DELETE FROM skills …` SQL of `remove_plugin_skills` behind a shared internal `skill_queries` helper in this module (keep the `PluginInstallStore` signature and its `with_tenant` transaction). Add `#[tracing::instrument(skip(skill, …))]` logging only `space_id` and `name` — never `instructions` or `NewSkill`. Give `list` a bounded `LIMIT` now; propose cursor pagination as a separate port change.
Rationale: the `skills` table has two writers in two modules, so any future column or soft-delete policy must be applied twice; unbounded `list` grows with skill count; the adapter emits no spans despite the tracing standard.
Risk: LOW for the refactor and traces; the `LIMIT` changes result sets for large spaces (DEVIATION risk — cap high enough and document it). Skipping `instructions` is mandatory: it is model-facing content and must never reach logs.

## 3. Sequencing and gates (recommended)

1. U1 + tests (security barrier first), 2. U2 (contract), 3. U3, 4. U4, 5. U5.
- After each step: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test -p boma-infra`; REQ id in every new test name.
- Add a two-adapter conformance test (Postgres vs `MemorySkillStore`) for System-context behaviour and the plugin-conflict rule; this is what keeps U1/U2 from regressing.
- U2 (error) and any `LIMIT`/port-type change are externally visible: they need entries in `docs/PARITY.md` and the operator's approval before shipping. No new product features are proposed here.
