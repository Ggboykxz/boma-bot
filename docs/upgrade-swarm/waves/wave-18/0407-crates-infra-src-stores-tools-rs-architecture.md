# v0.4.0 architecture proposal — `crates/infra/src/stores/tools.rs` (per-run tool catalogue)

Worker #407 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no code changed, no tests run).
Claim status: "current state" is CONFIRMED by direct reads in this worktree on 2026-11-19
(`tools.rs`, `rows.rs`, `tenant.rs`, `ports.rs`, `usecases.rs`, `memory.rs`, `0005_tools.sql`,
`spec/01-requirements.md`); forward-looking items are proposals (GUESS until implemented and measured).

## Current structure (CONFIRMED)

- `PostgresToolStore { pool: PgPool }` is stateless: `const fn new`, no field beyond the pool, one
  `#[async_trait]` impl with a single method `catalogue_for` (`tools.rs:14-77`). Exported at
  `infra/src/lib.rs:102`, wired in `bin/src/commands.rs:188` and the test harnesses.
- The method opens **two sequential `tenant::with_tenant` transactions** (`:39` and `:56`), each with
  its own `BEGIN`/`set_config`/`COMMIT` (`tenant.rs:51-63, 100-140`): first
  `SELECT enabled_tools, space_id FROM bots WHERE id = $1` (`:41-48`), then
  `SELECT tool_key, description, kind FROM tool_installs WHERE space_id = $1 AND enabled ORDER BY
  tool_key ASC` (`:58-65`). Two round-trips and two snapshots for one logical read.
- The bot row is decoded into an anonymous **tuple** `(Vec<String>, String)` (`:41`), not a named row
  struct. This is the only store read that bypasses the invariant in `rows.rs:1-5` ("every read goes
  through one of these structs"); `BotRow` exists (`rows.rs:55-71`) but is unused here. A column-order
  change would break this runtime query silently (no offline compile-time query checking in this crate).
- The grant filter runs **in Rust**, not SQL (`:74`): `grant.is_empty() || grant.iter().any(...)`.
  Ordering comes from SQL and survives the filter; cost is O(grants × installs) per run start.
- Mapping lives in `rows.rs:475-481` (`ToolInstallRow`) and `rows.rs:510-519` (`tool_spec`): the mapper
  hardcodes `effect: ToolEffect::Write` because `tool_installs` has no effect column; a malformed stored
  `kind` surfaces as a domain rule error via `?`, while DB failures are flattened to
  `AppError::Unavailable` by `tenant::unavailable` (`:47,67`). Two unrelated failures, two error classes.
- Tenant authority is entirely RLS-based: the bot query is anchored by bot id, the install query by the
  `space_id` **read from the bot row**, not from `context`. For a member of another space RLS hides both
  rows; for `TenantContext::System` RLS is bypassed and the worker may read any bot. The memory adapter
  encodes the same rule explicitly via `visible(context, ...)` (`memory.rs:932-939`, System -> true), so
  the adapters agree today — but only the memory one states the rule in code.
- Missing bot returns `Ok(Vec::new())` (`:51-53`), matching `MemoryToolStore` (`memory.rs:4076-4081`);
  the higher `tool_catalogue` use case returns `NotFound` for a missing bot (`usecases.rs:8213-8219`),
  so the two layers disagree on "unknown bot" deliberately.
- No `#[cfg(test)]`, no `#[tracing::instrument]` and no REQ-016-named test in this module; it is covered
  indirectly by `crates/infra/tests/m1.rs`, `crates/api/tests/http_surface.rs` and the memory parity pair.

## Guiding principle for v0.4.0

Treat this adapter as a **pure projection**: one snapshot, one row shape, one mapping, one error policy.
Keep the emitted `Vec<ToolSpec>` identical (keys, descriptions, kinds, order and default `Write` effect)
until a deliberate, approved DEVIATION changes it — the catalogue is handed to the model and recorded in
the audit trail, so a silent shape change is a parity break. Structure first, semantics second; every
semantic change needs a dual-adapter conformance test and, if externally visible, a `docs/PARITY.md` entry.

## Upgrades (5 concrete items)

1. **Fold the two queries into one single-snapshot statement (HIGH value, MEDIUM risk).**
   Replace the second transaction with one CTE query anchored on the bot, e.g.
   `WITH bot AS (SELECT space_id, enabled_tools FROM bots WHERE id = $1) SELECT i.tool_key,
   i.description, i.kind FROM bot b JOIN tool_installs i ON i.space_id = b.space_id AND i.enabled
   WHERE cardinality(b.enabled_tools) = 0 OR i.tool_key = ANY(b.enabled_tools) ORDER BY i.tool_key ASC`.
   This also moves the grant filter into SQL, removes one transaction and makes the catalogue one read.
   *Risk:* `= ANY` on `text[]` with NULL/duplicate keys and the empty-grant branch must match the Rust
   semantics exactly; RLS must still scope both tables; "missing bot -> empty" must survive the join.
   *Mitigation:* dual-adapter conformance tests for empty grant, keyed grant, unknown grant key,
   disabled install, missing bot, and member-of-space / member-of-other / system contexts; compare
   Postgres and `MemoryToolStore` element-by-element; keep `ORDER BY` verbatim. No wire change.

2. **Give the bot read a named row (HIGH value, LOW risk).**
   Add a small `ToolGrantRow { enabled_tools: Vec<String>, space_id: String }` (or reuse `BotRow`) in
   `rows.rs`, with the column list spelled once, and decode with `query_as::<_, ToolGrantRow>`.
   *Risk:* a renamed or aliased column breaks `FromRow` at runtime because these are runtime queries.
   *Mitigation:* one integration test seeding a bot with a non-empty grant and asserting the decoded
   values; keep the SQL text otherwise unchanged. Restores the `rows.rs` invariant and removes a silent
   tuple-order coupling.

3. **Persist the tool effect class (MEDIUM value, HIGH blast radius — ADR + PARITY first).**
   Add an `effect` column to `tool_installs` (new migration, `NOT NULL DEFAULT 'write'`), widen
   `ToolInstallRow` and let `rows::tool_spec` map the stored value with `Write` as fallback for old rows.
   Today every connector install is forced to `Write` (`rows.rs:516-517`), so read-only connector tools
   can never be auto-approved under REQ-017. *Risk:* `ToolSpec.effect` is externally visible to the model
   and the approval gate; changing it for any existing install is a DEVIATION needing approval and a
   `docs/PARITY.md` entry; the change spans migration, `rows.rs`, both adapters and the dispatcher.
   *Mitigation:* ship the migration with the `'write'` default so existing rows are unchanged, keep the
   conservative `#[serde(default)]` fallback, add a REQ-016/REQ-017-named test proving an untouched
   install still yields `Write`, and get the ADR approved before reading any new value. GUESS until
   checked against the original: whether the spec expects any connector tool to default to non-Write.

4. **Split the error taxonomy at the adapter boundary (MEDIUM value, MEDIUM risk).**
   Keep `tenant::unavailable` for driver/connection failures, but classify constraint/permission
   denials and malformed stored `kind`/`effect` explicitly, with driver detail left in `tracing` only
   (REQ-054). *Risk:* the error shape is part of the stable API format and `MemoryToolStore` must yield
   the same `AppError` variants or golden tests drift. *Mitigation:* enumerate current outcomes
   (`Ok(empty)`, domain rule error, `Unavailable`) as a test table, add REQ-016-named cases for corrupt
   `kind` and a dead pool, and change nothing externally visible without a PARITY entry. Prepares, but
   does not require, the peer typed-error work in `pending_tool_calls.rs`.

5. **Make the hot path observable and pinned by tests (MEDIUM value, LOW/MEDIUM risk).**
   Add `#[tracing::instrument(skip_all, fields(bot = %bot_id))]` to `catalogue_for` (never log keys,
   descriptions or grant contents); add a partial index `(space_id, tool_key) WHERE enabled` only after
   `EXPLAIN` on a seeded space shows the `enabled` filter is selective; add REQ-016-named integration
   tests for built-ins + installs, empty grant, keyed grant, disabled install, cross-tenant isolation
   and ordering. *Risk:* spans add volume on every run start; index benefit is unmeasured (GUESS) and
   the PK `(space_id, tool_key)` already covers most of the scan; the index adds write cost.
   *Mitigation:* keep spans at debug with redacted fields, gate the index on `EXPLAIN` evidence, and put
   the REQ id in test names (`tools/trace_check.py` counts declarations, not comments).

## Cross-cutting risks / sequencing

- Sequence 2 -> 1 -> 4 -> 5 (structure, snapshot, errors, tests/observability); item 3 is independently
  shippable but needs an ADR and a PARITY decision before any stored effect value is consumed.
- Do not change the returned order, the empty-bot behaviour or the default `Write` effect without an
  approved DEVIATION. Keep ADR-002's single-tenant-setting invariant intact; no second tenant guard here.
- Gate each step with `cargo fmt --check`, `clippy -D warnings`, unit + `tests/` integration and the
  golden/contract suites; no new dependency is required for items 1, 2, 4 or 5.
