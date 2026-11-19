# v0.4.0 architecture proposal — `migrations/0011_snoozes.sql` (per-member snooze schema)

Worker #435 · MODULE=migrations/0011_snoozes.sql · LENS=architecture · target v0.4.0 ·
READ-ONLY analysis (no code changed). "Current" claims are CONFIRMED by reads in the `before-m5`
worktree this session; forward-looking items are proposals (GUESS until applied and tested).

## Current structure (CONFIRMED)

- `0011_snoozes.sql:8-16` creates `thread_snoozes(thread_id, space_id, user_id, snoozed_until,
  created_at, updated_at)` with PK `(thread_id, user_id)` and three `ON DELETE CASCADE` FKs;
  `snoozed_until` is unconstrained `timestamptz NOT NULL`.
- `:18-33` is the one-table `DO $$ FOREACH` idiom reused from `0010`/`0009`: ENABLE + FORCE RLS and
  one policy `space_id = boma.current_space_id() OR boma.is_system()` for both USING and WITH CHECK.
  The helpers read GUCs (`0002_m1.sql:27-42`).
- The stated design (`:1-6`) is "snooze as filter": expiry is a read-path `snoozed_until > now()`
  predicate with no job, and clearing deletes the row. Confirmed in the only reader,
  `crates/infra/src/stores/roster.rs:45-61` (LEFT JOIN with the deadline predicate), and writer
  `:75-124` (guarded upsert, unguarded delete). Requirement: REQ-009 (`spec/01-requirements.md:74-80`).
- No index beyond the PK, no `COMMENT ON`, no deadline/triplet-consistency constraint, no retention
  mechanism. Expired rows are invisible but persist until re-snoozed or cleared.
- Migrations are embedded via `sqlx::migrate!` (`crates/infra/src/db.rs:20`), checksummed in
  `_sqlx_migrations`, and run in a transaction each. Existing `0011` must therefore never be edited.

## Constraint: additive only

Ship one new forward-only migration (e.g. `0060_snooze_hardening.sql`) plus optional follow-ups.
No edits to `0011`, its policy, or any row value; no `DROP`/rewrite of existing objects. Because
`sqlx::migrate!` wraps each file in a transaction, `CREATE INDEX CONCURRENTLY` needs a separate
non-transactional step. Any externally visible behaviour delta is a DEVIATION for `docs/PARITY.md`
and owner approval (AGENTS.md); all items below are internal hardening only.

## Upgrades (5 concrete items)

1. **Index the actual access pattern.** Add
   `CREATE INDEX thread_snoozes_user_deadline_idx ON thread_snoozes (user_id, snoozed_until)`.
   The PK `(thread_id, user_id)` serves the per-thread join in `roster.rs:56-57`, but the roster
   scans by `user_id` across all threads; a future "snoozed shelf" list has the same shape.
   *Risk:* write cost and disk on a small table with no measured gain; a partial
   `WHERE snoozed_until > now()` index is **illegal** (`now()` is not IMMUTABLE). *Mitigation:*
   prove with `EXPLAIN (ANALYZE, BUFFERS)` on a seeded space; drop the index if the planner ignores it.

2. **Make the denormalized `space_id` trustworthy.** Add a `BEFORE INSERT OR UPDATE` trigger rejecting
   a row unless `EXISTS (SELECT 1 FROM threads WHERE id = NEW.thread_id AND space_id = NEW.space_id)`.
   RLS keys on this copied column, so a bad value hides a snooze from its rightful tenant or exposes
   it on a system path; today only the Rust upsert's `WHERE EXISTS` guards it.
   *Risk:* a trigger function with a lax `search_path` under FORCE RLS can resolve the wrong objects,
   and a strict check could reject legitimate cross-space repair writes. *Mitigation:* SECURITY
   INVOKER with `SET search_path = pg_catalog`, fully-qualified names, no other table writes; test
   that a mismatched-space direct INSERT fails while the existing `(thread,user)` upsert still works.

3. **Pin the contract with comments and a regression test, not an edit.** Add `COMMENT ON TABLE
   thread_snoozes`, `COMMENT ON COLUMN thread_snoozes.snoozed_until` ("deadline on the database
   clock; expiry is a read-path now() comparison; no cleanup job") and `COMMENT ON POLICY
   thread_snoozes_tenant_isolation` (REQ-009, ADR-002), plus a test asserting the policy definition
   in `pg_policies` is unchanged. *Risk:* comments are free text and can drift from code; a catalog
   test couples to PostgreSQL's `pg_policies` text. *Mitigation:* keep the canonical policy string in
   one fixture shared with sibling tables; never `DROP POLICY` merely to rename it.

4. **Bound table growth without adding a job.** Add a callable, unscheduled
   `boma.prune_expired_snoozes(retention interval DEFAULT interval '30 days')` that deletes rows with
   `snoozed_until < now() - retention`, usable only in system context. Expired rows are already
   invisible, so pruning is pure space reclaim and preserves the no-timer invariant.
   *Risk:* retention is a GUESS (no spec value); a future history reader would lose rows, and an
   unscheduled function helps only if an operator calls it. *Mitigation:* default to a no-op
   (`retention <= 0`) until a decision is recorded in `docs/STATE.md`; leave scheduling out of v0.4.0.

5. **Constrain the deadline shape without rejecting past deadlines.** Add a `CHECK` rejecting
   non-finite values (`snoozed_until <> 'infinity'::timestamptz AND snoozed_until <>
   '-infinity'::timestamptz`), `NOT VALID` first, then `VALIDATE CONSTRAINT`. Keep past deadlines
   legal: `roster.rs` deliberately treats them as "not snoozed" and the writer may use one to expire.
   *Risk:* a CHECK using `now()` is impossible (non-IMMUTABLE), so a "no far-future" cap would need
   a trigger and a behaviour change; VALIDATE fails if any seeded row has an infinite timestamp.
   *Mitigation:* audit with `SELECT count(*) FROM thread_snoozes WHERE NOT isfinite(snoozed_until)`,
   keep `NOT VALID` until clean, and log the outcome in `docs/STATE.md`.

## Cross-cutting risks / sequencing

Order 3 → 1 → 2 → 5 → 4: pin the contract, measure the index, enforce tenant truth, then data
hygiene, then optional retention. Each item is an independent `CREATE`/`ALTER`, so partial
application is safe. Verify on a scratch DB with the REQ-009 tests (`crates/infra/tests/m1.rs:974-1088`,
`crates/infra/tests/tenancy.rs:172-209`) and the golden before claiming success. Wave-17's
`roster.rs` proposal owns the read/write code; this file only hardens the migration and its schema.
Nothing here weakens RLS, the trace ratchet, the checksum ledger, or the no-job expiry model.

## Out of scope

No schema change to `threads`/`spaces`, no RLS-helper refactor shared across migrations, no
snooze-history table, no cleanup scheduling, and no wire/DTO change (any such change is a DEVIATION
requiring owner approval). All forward-looking statements above are GUESS until implemented.
