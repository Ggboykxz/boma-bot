# 0444 — `migrations/0015_run_routine_id.sql` — reliability (v0.4.0)

Worker #444 · lens: reliability · target: v0.4.0 · READ-ONLY review of `before-m5`. Proposals
only; no product code changed. English. **Additive only**: new statements in new files or new
tests; `0015` is never edited in place. Claims from source only; unrun checks marked GUESS.

## 1. Ground truth (CONFIRMED by reading the tree this session)

- `0015` is three comment lines plus `ALTER TABLE runs ADD COLUMN routine_id text REFERENCES
  routines (id) ON DELETE SET NULL;` — nullable, inline single-column FK, no index, no name,
  no backfill.
- `0002`: `runs` has `id text PK`, `space_id text NOT NULL ... ON DELETE CASCADE`, `trigger text
  NOT NULL`, `ENABLE`+`FORCE ROW LEVEL SECURITY` with `runs_tenant_isolation` on `space_id`.
  `0014`: `routines` (`id text PK`, `space_id ... ON DELETE CASCADE`, same RLS pattern) and
  `routine_runs` (`routine_id NOT NULL ... ON DELETE CASCADE`, `run_id ... ON DELETE SET NULL`,
  non-unique `routine_runs_run_idx`).
- `crates/infra/src/stores/runs.rs:295-328` (`find_open_routine`, REQ-024 coalescing) runs
  `WHERE routine_id = $1 AND status NOT IN (...) ORDER BY created_at ASC LIMIT 1` under the
  tenant path; no index on `runs.routine_id` exists in `migrations/` (`0008` indexes only
  `parent_run_id`).
- `crates/app/src/worker.rs:342-355` looks the routine up with a `Member` context scoped to
  `run.space_id` and falls back to history on a miss — a NULL after deletion degrades safely.
- `crates/infra/src/db.rs` uses `sqlx::migrate!("../../migrations")` with no `-- no-transaction`
  directive: one transaction per migration, and editing an applied file changes its checksum
  and blocks startup. `0051` shows the house backfill style (plain `UPDATE` in its own file).

## 2. Keep, do not re-propose

Nullable column + `ON DELETE SET NULL` preserves run history while `routine_runs` rows cascade
away; `trigger` is the surviving provenance hint. DDL only means existing rows become NULL and
are not FK-checked, so applying `0015` to a large `runs` table is cheap. One inline FK, no
trigger, no redundant constraint.

## 3. Failure modes → concrete additive hardening

### F1 — FK: no tenant alignment (cross-space links)

The FK checks only that `routines.id` exists, not `routines.space_id = runs.space_id`. A bug can
attach a space-A run to a space-B routine. PostgreSQL FK checks run as the referencing table's
owner and are **not** RLS-filtered (GUESS until §5's test runs), so the bad link is accepted
even though the tenant cannot `SELECT` that routine; `worker.rs` then silently falls back.

- **P1.** New migration (number at merge; `0060` is claimed by wave-19):
  `ALTER TABLE routines ADD CONSTRAINT routines_space_id_id_key UNIQUE (space_id, id);`
- **P2.** Same file: `ALTER TABLE runs ADD CONSTRAINT runs_routine_tenant_fk FOREIGN KEY
  (space_id, routine_id) REFERENCES routines (space_id, id) ON DELETE SET NULL (routine_id);`
  The explicit column list is essential: bare `SET NULL` would NULL `space_id` (NOT NULL) and
  make every routine deletion fail. MATCH SIMPLE skips the check while `routine_id` is NULL.
  Keep the legacy FK; dropping it is a later, separately-approved cleanup.

### F2 — FK/delete path: unindexed referencing column

PostgreSQL does not index the referencing side of an FK. Deleting a routine scans all `runs` to
apply `SET NULL`, and `find_open_routine` degrades to seq scan + sort as `runs` grows.

- **P3.** `CREATE INDEX runs_routine_id_created_at_idx ON runs (routine_id, created_at) WHERE
  routine_id IS NOT NULL;` — the prefix serves the delete action, the second key serves the
  `ORDER BY ... LIMIT 1`, status stays a residual predicate. One index for both readers.

### F3 — RLS: a privileged action mutates rows under another tenant

`ON DELETE SET NULL` executes as the table owner, bypassing `runs`'s FORCE RLS. If F1's link
exists, deleting a space-B routine nulls `routine_id` on a space-A run: a silent cross-tenant
write. `routine_runs`'s cascade is safe because its own `space_id` matches its policy.

- **P4.** P1+P2 are the only structural fix: cross-space links become unrepresentable, so the
  privileged action can only touch same-space runs. Do **not** add a `runs` policy keyed on
  `routine_id` — policies cannot constrain FK actions and give false assurance.
- **P5.** REQ-024 regression test: create a space-B routine, attempt a space-A run with that
  `routine_id` (assert reject), delete the routine, assert no space-A row changed. Mark the
  RLS-bypass claim CONFIRMED only when it passes.

### F4 — Backfill: none exists; ad-hoc versions are easy to get wrong

`0015` backfills nothing. `routine_runs.run_id` is **not unique**, so a naive correlated
`UPDATE` can pick an arbitrary routine when coalescing recorded several events on one run. A
backfill under a role subject to FORCE RLS without a system reason silently updates zero rows
and still reports success.

- **P6.** Second new migration, idempotent and deterministic: `UPDATE runs r SET routine_id =
  src.routine_id FROM (SELECT DISTINCT ON (rr.run_id) rr.run_id, rr.routine_id FROM routine_runs
  rr ORDER BY rr.run_id, rr.occurred_at, rr.id) src WHERE r.id = src.run_id AND r.routine_id IS
  NULL AND r.space_id = (SELECT space_id FROM routines WHERE id = src.routine_id);`
  `DISTINCT ON` picks the earliest ledger event; the space predicate refuses bad links even
  before P2. Re-running is a no-op.
- **P7.** Guard it: count rows matching the same predicate before the update and
  `RAISE EXCEPTION` if leftovers remain after it — a silent RLS no-op then fails the deploy.
  Ops must run this through the system path (`boma.is_system()`), never a member connection.
- **P8.** Runbook note on what is **not** recovered: runs whose routine was deleted before the
  backfill, and runs with no `routine_runs` row (`runNow` probes). `trigger` stays their only
  provenance; invent no links.
- **P9.** Land P1–P8 only in new numbered files; record `DROP CONSTRAINT` / `DROP INDEX`
  downgrades per spec 04 §4.9. Never edit an applied migration (ADR-002 habit).

## 4. Risks and trade-offs

- **R1.** P1's UNIQUE and P3's index take `ACCESS EXCLUSIVE` locks for the migration
  transaction; `CREATE INDEX CONCURRENTLY` cannot run inside SQLx's transaction. Mitigate with
  a low-traffic window or a separate concurrency-safe runbook step — record it, do not weaken
  a gate.
- **R2.** Two FKs double the insert-side check: one indexed probe per routine-woken run.
  Acceptable; remove the legacy FK only via a later ADR.
- **R3.** `SET NULL (column-list)` needs PostgreSQL 15+ (pinned server version unverified,
  GUESS). Confirm before landing; otherwise use a `BEFORE DELETE` trigger with an ADR.
- **R4.** P6's deterministic pick may differ from a naive first-row attribution for coalesced
  runs. Attribution data only; note in `docs/PARITY.md` if a golden asserts it.
- **R5.** P7 fails the deploy on leftover rows — intentional: a silent partial backfill is
  worse. P9 adds no API/wire change, so no DEVIATION is expected (GUESS until contract test).

## 5. Verification for v0.4.0

- `cargo test -p boma-infra`: REQ-024-named tests for P2 (cross-space reject), P3 (`EXPLAIN`
  uses the index), P5 (delete touches only same-space runs), P6 (run twice = idempotent).
- `SELECT count(*) FROM runs r JOIN routines rt ON rt.id = r.routine_id WHERE
  rt.space_id <> r.space_id;` must be 0 after P6; seed an RLS-masked row to exercise P7.
