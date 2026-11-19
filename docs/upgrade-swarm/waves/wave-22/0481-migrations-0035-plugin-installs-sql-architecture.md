# 0481 — `migrations/0035_plugin_installs.sql` — architecture (v0.4.0)

- Worker #481 · Lens: architecture · Target: v0.4.0 · Read-only review; only this proposal file was written.
- Module: `migrations/0035_plugin_installs.sql` (24 lines) at `before-m5` (v0.3.0-rc5).
- Read for context (CONFIRMED by direct reads): `migrations/0034_connector_installs.sql`,
  `migrations/0027_plugin_submissions.sql`, `migrations/0028_skills.sql`, `migrations/0059_acp_agents.sql`,
  `crates/domain/src/plugin.rs`, `spec/01-requirements.md` REQ-034, `spec/02-data-model.md:128–143`,
  `spec/05-known-defects.md` D-06, wave-17 `0373-...plugin-installs-rs-architecture.md`,
  wave-21 `0469-...skills-sql-architecture.md` and `0463-...bot-secrets-sql-architecture.md`, `AGENTS.md`.

## Current structure (CONFIRMED)

One table `plugin_installs`: `id text PRIMARY KEY`, `space_id text NOT NULL REFERENCES spaces(id) ON
DELETE CASCADE`, `source text NOT NULL`, `resolved_sha text` (nullable), `name text NOT NULL`,
`parts jsonb NOT NULL DEFAULT '{}'::jsonb`, `skipped jsonb NOT NULL DEFAULT '[]'::jsonb`,
`created_at`/`updated_at timestamptz NOT NULL DEFAULT now()`, and `UNIQUE (space_id, source)`.
RLS is enabled and forced with one policy `plugin_installs_tenant_isolation` whose USING and WITH
CHECK are both `space_id = boma.current_space_id() OR boma.is_system()`.

Notable absences: no index beyond the UNIQUE btree (its `space_id` prefix covers space reads, but the
store's `ORDER BY created_at DESC, id ASC` still sorts), no shape/format CHECK on `source`,
`resolved_sha`, `parts` or `skipped`, no DB-maintained `updated_at`, no `status`/revocation column,
and no link to the reviewed `plugin_submissions` row (D-06). `source` is the canonical key
`url@ref#subdir` from `domain/src/plugin.rs` (max 2048 chars, HTTPS, no credentials) — the DB stores
it verbatim. The store upserts on `(space_id, source)` and resets `created_at` on reinstall, so the
column means "last replaced", not "first installed" (wave-17 0373, finding 4).

## Guiding principle for v0.4.0

Additive only: recorded migrations are never edited, so every change below is a new, forward-only
migration — `CREATE INDEX`, `ADD COLUMN ... DEFAULT NULL`, `ADD CONSTRAINT ... NOT VALID` +
`VALIDATE`, a function/trigger, or a new table with its own RLS policy. No dropped/renamed columns,
no type changes, no replaced RLS predicate, no wire/ORDER BY change. Column names, the
`UNIQUE (space_id, source)` identity, the `parts`/`skipped` JSON shape and the exact RLS predicate
are the compatibility contract; any externally visible change is a DEVIATION requiring
`docs/PARITY.md` (AGENTS.md). Inferred claims are marked GUESS.

## Concrete upgrades (5), each with a risk

1. **Index the store's list ordering.** Add `CREATE INDEX plugin_installs_space_created ON
   plugin_installs (space_id, created_at DESC, id ASC);` to serve `plugin_installs.rs::list`
   (`ORDER BY created_at DESC, id ASC`) under RLS. *Risk:* `CREATE INDEX CONCURRENTLY` cannot run in
   a transaction block and may break a txn-wrapping migration runner; the table is small (GUESS — no
   size evidence) so a plain `CREATE INDEX` is fine. *Mitigation:* use plain `CREATE INDEX`, note the
   write amplification on the upsert-heavy reinstall path, keep `DROP INDEX` as the rollback.

2. **Give `parts`/`skipped` a DB-enforced shape (NOT VALID first).** Add
   `CHECK (jsonb_typeof(parts) = 'object')` and `CHECK (jsonb_typeof(skipped) = 'array')` plus
   key-presence guards (`parts ? 'skills'`, `parts ? 'mcp_servers'`) as `NOT VALID`, audit with a
   `SELECT ... WHERE NOT (...)`, then `VALIDATE`. This pins REQ-034's "skipped parts named with
   reasons" contract at rest. *Risk:* legacy rows written by older versions may carry a different
   shape and block validation; a strict check can reject previously accepted input = DEVIATION.
   *Mitigation:* keep checks permissive (types + key presence, not exact value types), default
   missing keys to `{}` / `[]` as the store already does, and call it CONFIRMED only after the audit.

3. **Bound and describe the source provenance columns.** Add `CHECK (length(btrim(source)) > 0 AND
   length(source) <= 2048)` and a SHA-format `CHECK (resolved_sha IS NULL OR resolved_sha ~
   '^([0-9a-f]{40}|[0-9a-f]{64})$')` as `NOT VALID`, then `VALIDATE`; add a nullable
   `resolved_at timestamptz DEFAULT NULL` for when a ref was resolved. *Risk:* the domain parser's
   2048/HTTPS rules are app-side, so legacy `source` values or unpinned/non-git sources with no SHA
   can fail validation; a new column needs store write changes. *Mitigation:* audit first, keep
   `resolved_at` nullable, and never derive identity from `resolved_sha` (identity stays `source`).

4. **Enforce `updated_at` in the database.** Add `boma.touch_updated_at()` and a `BEFORE UPDATE`
   trigger on `plugin_installs`, firing only when `NEW.updated_at = OLD.updated_at` so the store's
   explicit `now()` wins. *Risk:* an unconditional trigger could override a caller-supplied
   timestamp and disturb the `created_at DESC` ordering or goldens, and it introduces a new schema
   object. *Mitigation:* conditional firing as in the skills proposal (wave-21 0469), and add a
   REQ-034 test that a reinstall advances `updated_at`.

5. **Share the tenant-RLS boilerplate for future tenant tables.** Define `boma.tenant_isolation(text)`
   STABLE returning the exact `space_id = boma.current_space_id() OR boma.is_system()` predicate,
   plus an apply helper that does `ENABLE`/`FORCE` RLS and an idempotent
   `DROP POLICY IF EXISTS ...; CREATE POLICY ...`; use it in new migrations, leave 0035's bytes
   unchanged. *Risk:* the predicate must preserve the `boma.is_system()` escape hatch and FORCE
   semantics exactly or system migrations break; a `SECURITY DEFINER` helper needs a pinned
   `search_path`, and one centralized bug would affect every tenant table. *Mitigation:* pure STABLE
   SQL function (no definer), and a REQ-034 RLS test asserting tenant A cannot see tenant B's install.

## Cross-cutting risks / sequencing

- Sequence 1 → 2/3 → 4 → 5: index and shape checks are local; format checks need a data audit before
  `VALIDATE`; the trigger and shared predicate last. All steps are additive and individually revertible.
- Freeze the public surface while doing this: column names, `UNIQUE (space_id, source)`, the
  `parts`/`skipped` JSON shape, the exact RLS predicate, and `plugin_installs.rs` queries and
  `ORDER BY created_at DESC, id ASC`.
- The `created_at` "last replaced" semantics and the missing uninstall/status concept are real
  architecture debt (0373 finding 4), but fixing either is externally visible — record as a
  DEVIATION in `docs/PARITY.md` and get owner approval before shipping, not in an additive migration.
- D-06 (unauthenticated cross-origin POST) concerns `0027_plugin_submissions`, not this ledger; the
  install/submission linkage belongs to that module and the review path, out of scope here (GUESS on need).
- Traceability: no new REQ id (context is REQ-034); `tools/trace_check.py` unaffected. Gate with
  `cargo fmt --check`, `clippy -D warnings`, the REQ-034 infra round-trip and marketplace goldens,
  and the REQ-034 RLS test. No new dependency is needed.

## Out of scope for this module

Rewriting/backfilling rows, changing `created_at` semantics or adding a revocation `status`, editing
`0035`, the review/submission table (`0027`), the skill table (`0028`), the canonical source grammar
(`domain/src/plugin.rs`), acquisition (`infra/src/plugin.rs`), `marketplace_install` orchestration
and the RPC view of `PluginInstallSummary`. Those belong to other workers/modules; this proposal only
strengthens the `plugin_installs` table structure additively.
