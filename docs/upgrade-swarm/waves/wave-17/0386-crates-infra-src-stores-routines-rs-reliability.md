# Upgrade proposal — v0.4.0 — reliability lens

Module: `crates/infra/src/stores/routines.rs` (`PostgresRoutineStore`, REQ-023/REQ-024)
Lens: reliability of scheduled dispatch. Scope: this store plus its immediate call site
`dispatch_routines` in `crates/app/src/usecases.rs` (the store cannot be hardened in isolation).

Evidence status: all code/schema claims below are CONFIRMED by reading `routines.rs`,
`crates/app/src/ports.rs`, `usecases.rs:5271-5339`, `tenant.rs`, `migrations/0014_routines.sql`,
`spec/01-requirements.md:189-203`. Deployment topology (single vs multiple dispatcher processes)
is a GUESS — the code makes no single-instance guarantee, so hardening must assume >1.

## Failure modes

1. Duplicate runs / stampede (highest). `due()` is a plain `SELECT ... WHERE active AND
   next_run_at <= $1` in a read-only system transaction (`tenant::as_system`). It neither locks
   nor claims nor advances `next_run_at`. Two overlapping ticks — or two processes — see the same
   rows. `dispatch_routines` then does `find_open_routine` → `runs.create` → `record_fired` as
   three separate transactions (each `with_tenant` call commits independently), so the coalescing
   check is a TOCTOU: both callers can observe "no open run" and both insert a `Run`. The DB has no
   unique key preventing a second open routine run (`0014_routines.sql` only indexes `run_id`);
   `routine_runs` has no idempotency key either, so a retried callback adds duplicate audit rows.
   `due()` also has no `LIMIT`, so one large tick holds many routines due while it works.

2. Missed / drifting schedules. `next_run_at` is advanced only *after* the run is created, in
   `record_fired`/`record_skip`, and is recomputed from the dispatch tick's `now`, not from the
   slot that was consumed. Consequence: downtime drops missed slots instead of catching up (a
   spec-visible behavior change if "reliable dispatch" is read literally); a slow tick drifts each
   fire later. Both `record_fired` and `record_skip` write `next_run_at` with unconditional
   `UPDATE ... WHERE id AND space_id`: a delayed callback from an older tick can overwrite a newer
   wake (last-write-wins), regressing or re-arming a schedule. A `@once` skip/fired sets
   `next_run_at = NULL` and `active = false` with no compare on the expected due value, so a
   concurrent re-activation/update can race into an inconsistent active/next pair.

3. State races and feedback errors. `record_failure` increments with `account_failures + 1` and
   pauses at the threshold; `record_fired`/`record_completed` reset to 0. Each statement is atomic
   in isolation and the two statements of each method share one transaction (`tenant.rs:116-141`),
   which is good — but cross-method ordering is unprotected: a late `record_completed` can reset a
   counter after a genuine failure, and two failure callbacks for one run double-count. `run_id` is
   `Option<&RunId>` and is inserted as NULL when absent, so "fired" audit rows can be orphaned;
   `record_completed` does not verify the run it is completing. `create()` binds
   `routine.space_id` while every other method uses `context.space_id()` — a mismatched NewRoutine
   relies on RLS to reject rather than being checked in code (defensive inconsistency).

## Concrete hardening (v0.4.0)

- A1. Turn claiming into one atomic DB operation: change `due()` to
  `SELECT ... FOR UPDATE SKIP LOCKED ... ORDER BY next_run_at, id LIMIT $n` and advance
  `next_run_at` (or set a `claimed_at`/`lease_until` + `claim_token`) inside the *same* transaction.
  Pass the consumed due value / claim token into `record_fired` and `record_skip`.
  Risk: needs a migration and SKIP LOCKED semantics; a lease needs clock-skew care and timeout
  tuning. Without A2 the lease alone only narrows, not closes, the duplicate window.

- A2. Compare-and-swap in the recorders: `UPDATE ... WHERE id = $1 AND space_id = $2 AND
  next_run_at = $expected` (or `= $claimed`), returning 0 rows as a benign stale-callback result
  instead of overwriting. Add the same guard to pause/`@once` deactivation. Risk: a genuine retry
  whose claim was lost now returns `NotFound`; the caller must re-read and decide skip vs re-drive,
  so this must be a documented, tested contract change (DEVIATION check in `docs/PARITY.md`).

- A3. Enforce coalescing in the database, not across calls: a unique partial index on open
  routine runs (pattern already used by `computer_runs_one_open` in `0017_computers.sql`) and/or a
  SQL function that atomically checks for an open run and either creates it or records a
  `coalesced` skip. Add a unique index on `routine_runs (routine_id, run_id) WHERE run_id IS NOT
  NULL` to make callbacks idempotent. Risk: a unique index may reject a legitimate retry after a
  run is considered closed — define the "open" predicate from the `runs` state machine carefully;
  function-based logic moves behavior out of Rust and needs its own tests.

- A4. Bound and observe the tick: `LIMIT` on `due()`, plus metrics for due-lag (`now - next_run_at`),
  skips/coalesces, duplicates prevented, and paused routines. Risk: a limit defers some routines to
  the next tick — acceptable and testable, but changes per-tick throughput.

- A5. Fix time semantics: compute the next wake from the scheduled slot with a capped catch-up loop
  (e.g. at most N backfills per fire), and reject a computed `next_run_at <= now` as invalid.
  Risk: catch-up changes externally visible scheduling (MUST-flagged "reliable dispatch") — file
  as a DEVIATION with approval before shipping; a cap trades completeness for bounded work.

- A6. Data hygiene: make `run_id` required for `fired` rows (or record an explicit reason), verify
  the run in `record_completed`, and align `create()` on `context.space_id()` with an explicit
  mismatch check. Risk: schema nullability change needs a migration/backfill for existing rows.

## Sequencing / verification

Order A1+A4 → A2 → A3 → A6 → A5 (A5 last because it is the only clearly spec-visible change).
Add REQ-024-named tests in `crates/infra/tests/`: two concurrent `due()` callers claim disjoint
rows; CAS rejects a stale callback; a duplicate open run is refused; a retried callback inserts one
audit row; downtime catch-up respects the cap. Assume single-instance is NOT guaranteed until the
operator confirms it in `docs/STATE.md` under "Decisions needed"; hardening holds either way.
