# 0469 — `migrations/0028_skills.sql` — architecture (v0.4.0)

- Worker #469 · Lens: architecture · Target: v0.4.0 · Read-only review; no code was changed.
- Module: `migrations/0028_skills.sql` (23 lines) at `before-m5` (v0.3.0-rc5).
- Read for context (CONFIRMED by direct reads): `crates/infra/src/stores/skills.rs`,
  `migrations/0035_plugin_installs.sql`, `spec/01-requirements.md` REQ-034/REQ-035,
  `spec/02-data-model.md:128–143`, `AGENTS.md`, and wave-17 `0373-...plugin-installs-rs-architecture.md`.

## Current structure (CONFIRMED)

One table `skills`: `id text PRIMARY KEY`, `space_id text NOT NULL REFERENCES spaces(id) ON DELETE
CASCADE`, `name`, `description`, `instructions` (all `text NOT NULL`), `origin text NOT NULL CHECK
(origin IN ('taught','plugin'))`, nullable `plugin_source`, `created_at`/`updated_at timestamptz
NOT NULL DEFAULT now()`, and `UNIQUE (space_id, name)`. RLS is enabled and forced with a single
policy `skills_tenant_isolation` whose USING and WITH CHECK are both
`space_id = boma.current_space_id() OR boma.is_system()`.

There is no secondary index beyond the UNIQUE btree (its `space_id` prefix covers space lookups),
no DB-maintained `updated_at`, no shape/length constraints on free text, no constraint tying
`plugin_source` to `origin`, and no enablement/join table. The store (`skills.rs`) upserts on
`(space_id, name)` setting `updated_at = now()`, reads with `ORDER BY name ASC`, deletes by `name`
only, and rejects system-scope writes with `AppError::NotFound`. The plugin-install worker proposes
a new `SkillStore::remove_by_source(plugin_source)` (wave-17 0373, upgrade 1), whose
`DELETE ... WHERE origin = 'plugin' AND plugin_source = $1` has no matching index today.

## Guiding principle for v0.4.0

Additive only: every change below is `CREATE INDEX`, `ADD COLUMN ... DEFAULT`, `ADD CONSTRAINT
... NOT VALID` + `VALIDATE`, a function/trigger, or a new table with its own RLS policy. No dropped
or renamed columns, no type changes, no replaced policy, no wire/ORDER BY change. Preserve column
names, the `UNIQUE (space_id, name)` identity, and the exact RLS predicate — any externally visible
change is a DEVIATION requiring `docs/PARITY.md` (AGENTS.md). Inferred claims are marked GUESS.

## Concrete upgrades (5), each with a risk

1. **Index the plugin-source revocation path.** Add
   `CREATE INDEX skills_origin_source_idx ON skills (space_id, origin, plugin_source);` to serve
   the planned `remove_by_source` and plugin reinstall. *Risk:* `CREATE INDEX CONCURRENTLY` cannot
   run inside a transaction block, so it may break a migration runner that wraps files in a txn.
   *Mitigation:* use a plain `CREATE INDEX` (table is small — GUESS, no size evidence found) and
   note the write amplification; keep it additive and reversible via `DROP INDEX`.

2. **Enforce `updated_at` in the database.** Add `boma.touch_updated_at()` and a
   `BEFORE UPDATE` trigger on `skills`. *Risk:* `skills.rs::put` already sets `updated_at = now()`;
   an unconditional trigger could override an explicitly supplied timestamp and disturb goldens,
   and it introduces a new schema object. *Mitigation:* make the trigger fire only when
   `NEW.updated_at = OLD.updated_at`, so callers that set it explicitly win; add a REQ-035 test
   that two `put` calls advance `updated_at`.

3. **Add the `plugin_source` ↔ `origin` invariant.** Add
   `CHECK ((origin = 'plugin') = (plugin_source IS NOT NULL)) NOT VALID`, then `VALIDATE
   CONSTRAINT`. This encodes the header comment ("omission report can name it; taught skills carry
   NULL") and `spec/02-data-model.md`. *Risk:* `VALIDATE` fails on rows written by older versions
   (plugin skill with NULL source, or taught skill with a stray source); no canonical constraint
   exists today. *Mitigation:* keep `NOT VALID` first so new writes are guarded, audit with a
   `SELECT ... WHERE ... IS DISTINCT FROM ...`, clean/backfill, then validate; call it CONFIRMED
   only after that audit.

4. **Bound the free-text columns.** Add `NOT VALID` CHECKs — `length(btrim(name)) > 0`,
   `length(name) <= 200`, `length(description) <= 2000` (optionally an `instructions` cap) — then
   `VALIDATE`. *Risk:* legacy long/empty values block validation, and a hard cap changes caller
   behavior (a DEVIATION if it rejects previously accepted input). *Mitigation:* audit max lengths
   first, choose generous caps or skip the `instructions` cap, and keep name identity byte-for-byte
   (no case folding — a case-insensitive UNIQUE would be a separate DEVIATION).

5. **Add an enablement join table aligned with the data model.** `spec/02-data-model.md:137` names
   `AgentSkillSpace` join rows for per-bot/per-space enablement; none exists. Add
   `bot_skills (space_id, bot_id, skill_id, enabled bool NOT NULL DEFAULT true, UNIQUE(bot_id,
   skill_id))` with FKs to `skills`/bots `ON DELETE CASCADE` and an RLS policy identical to the
   `skills` one. *Risk:* a table with no store/API consumer is dead weight, and per-bot enablement
   may exceed REQ-035 (SHOULD) — feature additions need operator approval (AGENTS.md).
   *Mitigation:* land it only with the REQ owner's agreement, record it as a proposal in
   `docs/STATE.md`, and keep the RLS predicate byte-identical to `skills_tenant_isolation`.

## Cross-cutting risks / sequencing

- Sequence 1 → 2 → 3/4 → 5: index and trigger are local; constraints need a data audit before
  `VALIDATE`; the new table last. All steps stay additive and individually revertable.
- Freeze public surface while doing this: column names, `UNIQUE (space_id, name)`, the RLS
  predicate, `skills.rs` queries and `ORDER BY name ASC`, plus the JSON/SkillSummary shape.
- RLS grants `boma.is_system()` read access to all skills while `skills.rs::put` rejects system
  writes; keep that asymmetry unchanged.
- Traceability: no new REQ id is introduced (context is REQ-035/REQ-034); `tools/trace_check.py`
  is unaffected. Gate with `cargo fmt --check`, `clippy -D warnings`, the REQ-035 skill round-trip,
  the REQ-034 plugin-install round-trip, and goldens. No new dependency is needed.

## Out of scope for this module

Rewriting/backfilling existing rows, case-insensitive identity, dropping or renaming columns or
the RLS policy, the plugin-install ledger (`0035`), API/RPC summary shape, and the teaching/install
orchestration in `usecases.rs`. Those belong to other workers/modules; this proposal only strengthens
the `skills` table structure additively.
