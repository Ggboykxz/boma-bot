# v0.4.0 architecture proposal — `migrations/0014_routines.sql` (worker #441, lens: architecture)

Status: proposal only (read-only review). **Additive only**: every change below is a NEW migration or a
NEW object; no existing migration, column, index, policy, constraint or row is dropped, renamed or
rewritten. Claims are CONFIRMED from `0014_routines.sql`, `0015_run_routine_id.sql`,
`0039_routines_github_enabled.sql`, `0041_routines_email_senders.sql`, `spec/02-data-model.md`,
`spec/01-requirements.md` (REQ-023/024/025), `crates/domain/src/routine.rs` and
`crates/infra/src/stores/routines.rs`, unless marked GUESS.

## Current structure (CONFIRMED)

`0014` creates `routines` (13 columns: identity + FKs to spaces/bots/threads/users, `name`, `prompt`,
`crons text[]`, `timezone`, `active`, `notify`, `last_run_at`, `next_run_at`, `account_failures`,
timestamps) plus `routine_runs` (`bigserial` PK, `outcome` CHECK over five values, `detail`,
`occurred_at`), two indexes on each table, and an RLS DO-loop that enables FORCE RLS and creates
`<table>_tenant_isolation` using `space_id = boma.current_space_id() OR boma.is_system()`. That
DO-loop is a repo-wide convention (0002/0004/0010/0013/…), not a defect of this file.

The entity is **fragmented**: 0015 adds `runs.routine_id`, 0039 adds `routines.github_enabled`,
0041 adds `routines.email_senders`; the spec's `Routine` row additionally lists `webhookEnabled`,
`messageProvider`, model fields and `watch`/`watchState` JSON that appear in no migration
(CONFIRMED by grep; GUESS: intentionally deferred or stored elsewhere).

Runtime shape (CONFIRMED): the dispatcher claims work with `WHERE active AND next_run_at IS NOT NULL
AND next_run_at <= $1::timestamptz ORDER BY next_run_at ASC, id ASC`; `active = false` is overloaded
for manual disable, account-failure pause and `@once` spent; domain validation (1–8 triggers, cron or
`@once`, IANA timezone) is stricter than the DB's single `cardinality(crons) >= 1` CHECK; `routine_runs`
has no natural key and no time index.

## Upgrade 1 — Give the fragmented schema a single ownership map (new `0060_routines_ownership.sql`)

Add `COMMENT ON TABLE`/`COMMENT ON COLUMN` recording that `routines` is owned by 0014 + 0039 + 0041 +
0015 (FK on `runs`) and that `routine_runs` is the append-only dispatch log. Decide explicitly whether
the missing in-spec fields are in scope; if yes, add them additively with safe defaults
(`webhook_enabled`, `message_provider`, `model_provider`, `model_id`, `thinking_level`, `watch jsonb`,
`watch_state jsonb`) in the same new migration. Do not edit 0014 itself.

- **Risk:** low for comments; medium/high for the columns — GUESS: the rebuild may deliberately omit
  some of them, and DB spelling (`snake_case` vs spec camelCase) is unpinned. Confirm against
  `spec/openapi.yaml` and `spec/golden/` first; unused columns are harmless but schema churn needs a
  `docs/STATE.md` note.

## Upgrade 2 — Tie a run to its routine's tenant with a composite FK (new `0061_routine_runs_tenant_fk.sql`)

Additively `ALTER TABLE routines ADD CONSTRAINT routines_id_space_key UNIQUE (id, space_id)` (the PK
already makes this cheap), then `ALTER TABLE routine_runs ADD CONSTRAINT routine_runs_routine_tenant_fk
FOREIGN KEY (routine_id, space_id) REFERENCES routines (id, space_id) ON DELETE CASCADE`. Today both
tables carry `space_id` but nothing proves they agree; this closes a cross-tenant write hole under a
system context.

- **Risk:** medium — the constraint fails to deploy if any existing row has a mismatched `space_id`
  (safe failure, but blocks the release). Ship a read-only verification query first; GUESS: all rows
  agree because every insert site binds the routine's space. Keep `ON DELETE CASCADE` identical to the
  existing single-column FK so deletion semantics do not change.

## Upgrade 3 — Align indexes with the actual queries (new `0062_routines_dispatch_indexes.sql`)

Add `routines_due_ordered_idx ON routines (next_run_at, id) WHERE active AND next_run_at IS NOT NULL`
so the dispatcher's `ORDER BY next_run_at ASC, id ASC` can be satisfied without a sort, and add
`routine_runs_space_time_idx ON routine_runs (space_id, occurred_at DESC)` for per-space history and
retention scans. Keep the existing four indexes untouched (purely additive).

- **Risk:** low — two extra indexes cost write amplification and disk on a hot log table; GUESS: the
  due backlog is small today, so measure with `EXPLAIN ANALYZE` before/after on a seeded DB. If the
  planner ignores the new due index, drop only the new one (the rest of the proposal stands).

## Upgrade 4 — Encode the `@once` invariant in the database (new `0063_routines_trigger_checks.sql`)

Add `CONSTRAINT routines_once_exclusive_check CHECK (NOT ('@once' = ANY(crons) AND cardinality(crons) > 1))`
and `CONSTRAINT routines_crons_cap_check CHECK (cardinality(crons) <= 8)`, mirroring domain rules
(`MAX_CRONS = 8`, `@once` is "the single one-shot value"). This is defense-in-depth for direct SQL
and imports; the app-layer validator stays the primary guard.

- **Risk:** medium — existing mixed `@once` + cron rows would block deployment (verify with a SELECT
  first), and rejecting a previously accepted combination is externally visible: record it in
  `docs/PARITY.md` with an ADR rather than treating it as a silent cleanup. GUESS: no caller produces
  such a row because `next_run_at` returns `None` the moment `@once` is present.

## Upgrade 5 — Make pause first-class instead of overloading `active` (new `0064_routines_pause_state.sql`)

Add nullable `paused_at timestamptz` and `pause_reason text` columns (no change to `active`, so all
current reads/writes keep working). Failure pause (`account_failures` threshold,
`crates/infra/src/stores/routines.rs:270`), manual disable and `@once` spent currently collapse into
`active = false`; the new columns let the owner notice "paused by failures" distinctly, as REQ-024
requires.

- **Risk:** low for schema, medium for semantics — additive columns are invisible until the app
  writes them; GUESS: no UI/RPC field exists yet, so exposing them would be a DEVIATION needing an ADR
  and owner approval. Keep `docs/PARITY.md` silent only while the columns remain internal.

## Out of scope / deferred (GUESS)

Replacing the DO-loop RLS pattern, backfilling any existing rows, and touching the `RoutineStore`
port or RPC surface are larger than one module. Track them in `docs/STATE.md` as separate proposals;
do not bundle them into v0.4.0 migrations.
