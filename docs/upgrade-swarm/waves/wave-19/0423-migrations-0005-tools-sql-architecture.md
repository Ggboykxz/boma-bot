# v0.4.0 architecture proposal — `migrations/0005_tools.sql` (tool catalogue schema)

Worker #423 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no code changed).

Status of claims: every "current state" statement below is CONFIRMED by direct reads of
`0005_tools.sql`, sibling migrations (`0003`, `0029`, `0034`), `spec/01-requirements.md` (REQ-016/017),
`spec/02-data-model.md`, and the consumers `crates/infra/src/stores/tools.rs`,
`crates/infra/src/stores/rows.rs`, `crates/app/src/usecases.rs` (`builtin_tools`,
`tool_catalogue`, `connector_catalogue_tools`) and `crates/domain/src/card.rs` (`ToolKind`,
`ToolEffect`). All forward-looking items are proposals (GUESS until implemented and tested).

## Current structure (CONFIRMED)

- `0005_tools.sql:6-14` creates `tool_installs(space_id, tool_key, description, kind, enabled,
  created_at)` with composite PK `(space_id, tool_key)`; `enabled` has no constraint, `kind` is an
  unconstrained `text`, and there is no `updated_at`, unlike every later catalogue table
  (`connector_installs` 0034, `plugin_installs` 0035, `mcp_servers` 0029).
- `:17` adds `bots.enabled_tools text[] NOT NULL DEFAULT '{}'` — a free-form key array with empty
  meaning "every enabled install"; no grammar or referential integrity is enforced in the database.
- `:19-34` enables/filters RLS for exactly one table through a `DO $$ ... FOREACH` loop whose policy
  is `space_id = boma.current_space_id() OR boma.is_system()`. Later migrations use direct
  `ENABLE / FORCE / CREATE POLICY` statements; the loop is a one-off idiom, not a pattern.
- The hot read path is `PostgresToolStore::catalogue_for` (`tools.rs:30-77`): one bot lookup, then
  `SELECT ... FROM tool_installs WHERE space_id = $1 AND enabled ORDER BY tool_key ASC`, filtered in
  Rust by the grant. The PK covers `(space_id, tool_key)` but `enabled` is unindexed.
- Hydration (`rows.rs:511-516`) parses `kind` via `ToolKind::parse` (vocabulary: builtin, connector,
  mcp, skill, memory) and hardcodes `ToolEffect::Write` for every install — the schema carries no
  effect class, so read-only installs are conservatively approval-gated and spend capability cannot
  be expressed.
- REQ-016 composes four sources: built-ins (code) + this table + MCP tools (0032) + connectors
  (0034). `tool_catalogue` (`usecases.rs:8210-8241`) merges them; a bad row or kind makes the whole
  catalogue call fail, so malformed data is an availability defect, not just a data-quality one.

## Guiding principle for v0.4.0

Additive only: ship a new forward-only migration (e.g. `0060_tool_catalogue_hardening.sql`) that
`ADD`s columns/constraints/indexes and leaves `0005_tools.sql` and all existing row values untouched.
Constrain at write time what the Rust domain already assumes at read time, align this table with its
sibling catalogue tables, and never change a public interface: a behaviour delta is a DEVIATION to
record in `docs/PARITY.md` before shipping (AGENTS.md). `sqlx::migrate!` wraps migrations in a
transaction (`crates/infra/src/db.rs:20`), so DDL that needs its own transaction must be split.

## Upgrades (5 concrete items)

1. **Pin the `kind` vocabulary at the boundary.** Add
   `CHECK (kind IN ('builtin','connector','mcp','skill','memory')) NOT VALID`, audit
   `SELECT DISTINCT kind`, then `VALIDATE CONSTRAINT` in a later step of the same release. This
   mirrors `ToolKind::parse` (`card.rs:139-168`) so a typo fails on write instead of breaking every
   catalogue read. *Risk:* existing rows with other strings (testkit seeds arbitrary kinds) make
   VALIDATE fail; a CHECK also blocks future kinds until migrated. *Mitigation:* keep `NOT VALID`
   until the audit is clean, derive the literal list from the enum in a test, and add a `COMMENT ON
   CONSTRAINT` marking it a REQ-016 invariant.

2. **Store the effect class instead of hardcoding it.** Add `effect text NOT NULL DEFAULT 'write'
   CHECK (effect IN ('read','write','spend'))` with a column comment; `rows::tool_spec` currently
   returns `ToolEffect::Write` for all installs. Default `'write'` preserves today's approval
   behaviour exactly; callers can opt in later. *Risk:* a misclassified `'read'` would bypass REQ-017
   approvals for a side-effecting tool, and `SELECT` lists that use `*` could change shape.
   *Mitigation:* keep the explicit column list in `tools.rs:59-62` unchanged, add a domain test that
   a read install still maps to `ToolEffect::Read` and a write/spend install to the gated variants,
   and gate any behavioural use behind a follow-up so this migration stays additive.

3. **Add `updated_at` with a minimal trigger.** `ADD COLUMN updated_at timestamptz NOT NULL DEFAULT
   now()` plus a `BEFORE UPDATE` trigger that only sets `NEW.updated_at`, matching 0029/0034/0035 and
   making install changes auditable. *Risk:* under `FORCE ROW LEVEL SECURITY` a trigger function with
   a lax `search_path` can resolve the wrong objects; a hidden write could surprise a system-context
   update. *Mitigation:* `SECURITY INVOKER` with `SET search_path = pg_catalog`, no cross-table
   writes, `IF NOT EXISTS` on a re-runnable companion migration, and an integration test that an
   update bumps only this column.

4. **Constrain tool-key grammar in both places.** Add `CHECK (tool_key ~
   '^[a-z0-9]+([._-][a-z0-9_-]+)+$')` and an array-level `bots` check rejecting empty/malformed
   elements of `enabled_tools`, both `NOT VALID` first. Real keys are dotted (`memory.read`,
   `delegate.helper`, `mcp.<server>.<tool>` per `usecases.rs:8264-8268`), so the grammar can be
   validated against observed values. *Risk:* a legitimate external tool name with uppercase or
   unusual punctuation would be rejected, and a malformed grant currently fails silently (no match)
   rather than loudly. *Mitigation:* audit existing keys and grants with regex queries before
   VALIDATE, document the grammar from the built-in key list, and keep the regex permissive on
   separators while capping length to bound abuse.

5. **Index the per-run catalogue read selectively.** Add
   `CREATE INDEX CONCURRENTLY tool_installs_enabled_key_idx ON tool_installs (space_id, tool_key)
   WHERE enabled` so disabled rows do not enter the working set of a query executed on every run.
   *Risk:* the PK already covers `(space_id, tool_key)`, so the index may add write cost with no gain;
   `CONCURRENTLY` cannot run inside the transaction `sqlx::migrate!` uses. *Mitigation:* prove the
   need with `EXPLAIN (ANALYZE, BUFFERS)` on a seeded space with many disabled installs, drop the
   idea if the planner ignores it, and either use a plain `CREATE INDEX` (accept the brief write
   lock at deploy) or split this item into its own no-transaction migration.

## Cross-cutting risks / sequencing

- Sequence 1 → 4 → 3 → 2 → 5: prove data hygiene first, then add metadata, then the optional
  effect field, then measure the index. Each item is an independent `ALTER`/`CREATE`, so partial
  application is safe.
- Keep the RLS contract unchanged: `boma.current_space_id() OR boma.is_system()` is the intended
  escape (DEC-05/ADR-002). Pin it with `COMMENT ON POLICY` and a regression test asserting the
  policy still exists, rather than editing the `DO` loop in `0005` (which cannot be re-issued
  idempotently).
- All claims here are structural; no migration has been applied or benchmarked. Verify each step on
  a scratch copy: apply the migration to a seeded database, run `cargo fmt --check`, `clippy -D
  warnings`, `tests/` integration plus the REQ-016/017 tests (`tools/trace_check.py` counts
  declaration lines), and replay golden fixtures before claiming success.

## Out of scope for this module

The Rust catalogue composition (`usecases.rs`, `stores/tools.rs`), connector/MCP/plugin tables
(0029/0032/0034/0035), approval semantics (REQ-017) and the RLS helper functions are other
workers' modules; this proposal only hardens the structure of the tools migration and its schema.
