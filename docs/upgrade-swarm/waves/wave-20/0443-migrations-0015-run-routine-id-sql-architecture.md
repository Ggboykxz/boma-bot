# 0015_run_routine_id.sql — architecture upgrade proposal (v0.4.0)

Worker #443 · lens: architecture · REQ-024 (with REQ-025 interaction)
Scope: ADDITIVE ONLY — every upgrade is a new statement (`CREATE INDEX`, `ADD CONSTRAINT`,
`CREATE TRIGGER`, `COMMENT ON`, or a NULL-only `UPDATE`). Line 6 of 0015 is never rewritten or
dropped; sibling migrations are untouched.
Evidence status: CONFIRMED from the file, `migrations/0001–0059`, `spec/01-requirements.md`,
`crates/domain/src/run.rs`, `docs/STATE.md` and `docs/TRACEABILITY.md` read this session; GUESS
where inferred from conventions.

## Current structure (CONFIRMED)

- 0015 is one statement: `ALTER TABLE runs ADD COLUMN routine_id text REFERENCES routines (id)
  ON DELETE SET NULL;` with a 4-line header comment. No index, no CHECK, no catalog metadata.
- The 4-line rationale is the whole contract: only routine-woken runs carry the id; deleting a
  routine nulls it to keep history readable; `runs.trigger` remains the durable "was a routine"
  record. `runs.trigger` is a bare `text NOT NULL` with no CHECK (0002:197) — values come from
  `RunTrigger::as_str()` (`crates/domain/src/run.rs:199`, `routine => "routine"`).
- Attribution is stored **three ways**: `runs.trigger='routine'`, `runs.routine_id` (0015), and a
  `routine_runs` row (`routine_id NOT NULL`, `run_id FK ON DELETE SET NULL`, outcome enum,
  0014:33-44). `routine_runs` has both `(routine_id)` and `(run_id)` indexes; `runs.routine_id`
  has none (matches 0389:40, CONFIRMED).
- `runs` is FORCE-RLS on `space_id` (0002:260-268); `routines` is FORCE-RLS on its own `space_id`
  (0014:46-58). FK existence checks run as the table owner and bypass RLS, so the FK only proves
  the id exists, not that the routine is in the run's space.
- `find_open_routine` uses `routine_id` for REQ-024 coalescing (`docs/TRACEABILITY.md:44`,
  0389:18-19); the Postgres create path had to be fixed to actually persist `routine_id`
  (`docs/STATE.md:1252,1831-1832`). REQ-025 trial runs carry `routine_id` with `trigger='user'`
  (`docs/STATE.md:1837`) — the trigger→id implication is one-directional.

## Proposed upgrades (each independently additive)

### U1 — Index the attribution/coalescing access path
`CREATE INDEX runs_routine_idx ON runs (routine_id, created_at) WHERE routine_id IS NOT NULL;`
Rationale: every REQ-024 coalescing lookup and every "what did this routine do" audit query
currently scans `runs`; the sibling table already indexes both sides of the link (0014:43-44).
Adding the index here closes the asymmetry without changing any query.
Risk: LOW–MEDIUM. Predicate must match the store's `WHERE routine_id IS NOT NULL` verbatim or the
planner ignores it; verify with `EXPLAIN` on both a seeded queue and a queue of future `not_before`
rows. Two-column index adds write amplification on every status change. Keep it NON-unique: a
UNIQUE variant would change concurrent-create behavior (a DEVIATION), tracked separately in 0389 U3.

### U2 — Make the reference tenant-safe
Prefer an additive `BEFORE INSERT OR UPDATE OF routine_id, space_id` trigger on `runs` that
rejects a non-NULL `routine_id` whose `routines.space_id <> NEW.space_id`; keep the existing
single-column FK as the delete path.
Rationale: today a run in space A can point at a routine in space B — RLS hides the mismatch from
reads but FK checks bypass it, so the link is not "defense in fact". Same pattern as 0433 U3.
Risk: MEDIUM–HIGH. Do **not** naively add a composite FK `(routine_id, space_id) REFERENCES
routines (id, space_id) ON DELETE SET NULL`: `SET NULL` nulls *all* referencing columns, which
would violate `runs.space_id NOT NULL` and block routine deletion, contradicting 0015's stated
history goal. A composite FK with `NO ACTION`/`CASCADE` is worse (blocks delete / destroys runs).
A trigger preserves `ON DELETE SET NULL`. It needs a pre-flight drift query returning 0 rows, a
decision on SECURITY DEFINER vs system context (the lookup must see cross-space rows), and one
indexed PK probe per attributed insert.

### U3 — Capture the one-directional trigger invariant in the catalog
`ALTER TABLE runs ADD CONSTRAINT runs_routine_trigger_ck
CHECK (trigger <> 'routine' OR routine_id IS NOT NULL);`
Rationale: `trigger` is the record that survives a routine delete (`routine_id` becomes NULL), so
the invariant is `trigger='routine' ⇒ routine_id IS NOT NULL` at write time — never the reverse.
REQ-025 trial runs legitimately have `routine_id` with `trigger='user'` (CONFIRMED STATE.md:1837),
so an iff-check would break them.
Risk: LOW–MEDIUM. Run a pre-flight `SELECT` for `trigger='routine' AND routine_id IS NULL` first;
any row blocks the constraint. The domain writes arbitrary `RunTrigger` strings, so this CHECK is
the first DB-level guard on `trigger`. Do not add a `trigger IN (...)` enum CHECK here: that is a
wider, externally visible validation change outside this module's scope.

### U4 — Document the contract in the system catalog
`COMMENT ON COLUMN runs.routine_id IS 'REQ-024: routine that woke this run; NULL for '
'non-routine and trial (REQ-025) runs; ON DELETE SET NULL keeps history, trigger records origin';`
(mirror the header comments on `runs.trigger` and `routine_runs.run_id`).
Rationale: the non-obvious NULL semantics and the trigger-vs-id asymmetry currently exist only in
a SQL comment and Rust code; catalog metadata is discoverable to SQLx offline metadata and future
migrations without touching behavior. Precedent: 0003:8-9.
Risk: NONE at runtime — purely descriptive; keep wording in English per project convention.

### U5 — Backfill historical attribution (NULL-only)
One-shot `UPDATE runs r SET routine_id = rr.routine_id FROM routine_runs rr
WHERE r.id = rr.run_id AND r.routine_id IS NULL AND rr.run_id IS NOT NULL
AND r.trigger = 'routine';`
Rationale: runs created before 0015 cannot carry the id; `routine_runs.run_id` is the authoritative
crosswalk (0014:37). Filling only NULLs makes old history agree with new writes.
Risk: MEDIUM. Bound the statement (batch by `created_at`) to avoid a long lock on a large `runs`
table; pre-flight for `routine_runs` rows whose `run_id`/`routine_id` disagree with an existing
non-NULL `runs.routine_id` (report, never overwrite); confirm system/RLS context sees all spaces.
This changes data, not schema — keep it in its own forward-only migration, not inline in 0015.

## Explicitly out of scope (would break "additive only" or parity)

- UNIQUE/partial-UNIQUE on open routine runs — REQ-024 invariant change, DEVIATION + ADR; owned by
  0389 U3, not this migration.
- Making `routine_id` immutable after create — a `BEFORE UPDATE OF routine_id` trigger is tempting
  for audit integrity, but the REQ-025 "keep" flow may re-attribute a trial run; GUESS, needs the
  usecase confirmed first.
- Collapsing the three attribution records into one canonical column; rewriting `routine_runs`
  or the duplicated RLS `DO $$` boilerplate; narrowing `boma.is_system()`.

## Verification gate (for the implementing agent)

1. Apply 0015 plus each new statement on an empty DB and on a seeded DB; assert re-runnable where
   possible. 2. Pre-flight U2/U3/U5 queries return 0 rows (or an explicit, reviewed exception list).
3. `EXPLAIN` `find_open_routine` and an audit query before/after U1 on seeded data. 4. Replay the
   REQ-024 and REQ-025 tests and Postgres tests (`m1.rs`); add a test asserting a cross-space
   `routine_id` is rejected (U2) and a trial run keeps `trigger='user'` with a `routine_id` (U3).
5. `cargo sqlx prepare --check` (offline metadata still valid) and the migration harness.
