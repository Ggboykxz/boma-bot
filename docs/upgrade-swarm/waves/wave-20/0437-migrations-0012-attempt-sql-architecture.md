# Worker #437 — `migrations/0012_attempt.sql` — architecture (v0.4.0)

- Module: `migrations/0012_attempt.sql` (6 lines, REQ-012); lens: architecture / migration structure.
- Mode: READ-ONLY proposal; no code changed, no tests run. Shape CONFIRMED by reading this session
  (migration, spec data model, golden, domain type, store write path, sibling migrations). Upgrades and
  risks are GUESS (design inference). Additive only: every upgrade adds a new migration object or a new
  test/doc file; nothing rewrites `0012`. No wire/API/data-format change by default; anything externally
  visible is flagged as a DEVIATION needing `docs/PARITY.md` + owner approval.

## 1. Current shape (CONFIRMED)

- The whole migration is one statement: `ALTER TABLE runs ADD COLUMN attempt integer NOT NULL DEFAULT 1
  CHECK (attempt >= 1)` (L6). Prose (L1-4) defines the semantics: one at birth, +1 per retry, same run
  row, one conversation/history, no duplicate runs.
- Column lives on the existing tenant table `runs` (`0002_m1.sql:190-209`), so it inherits that table's
  RLS automatically. No new policy, no index, no backfill statement — safe for a large table.
- The counter is already atomic at the write site: `stores/runs.rs:445-457` does a single guarded
  `UPDATE runs SET status='queued', attempt=attempt+1, ... WHERE id=$1 AND status='failed' AND space_id=$2
  RETURNING ...`; a concurrent retry sees zero rows. Domain type is `Run.attempt: u32` (`domain/run.rs:269-270`),
  mirrored in `api/wire.rs:229` and pinned by `spec/golden/run-retry.json:18` (`attempt: 2`).

## 2. Architecture findings

1. **Count vs. history are conflated (CONFIRMED tension).** `spec/02-data-model.md:59` defines an
   `Attempt` entity (`runId, fence, status, error, startedAt, finishedAt`, "one row per execution
   attempt; fence matches the run's lease"), and REQ-013 requires "attempt history intact" across a
   deploy. No migration creates an attempts table and no store writes one: the durable model is only a
   scalar. GUESS: this is the highest-value structural gap in this module.
2. **Fence is reused, not per-attempt (CONFIRMED).** Retry resets `lease_fence = 0` (`runs.rs:452`) and
   claims grow it on the same row; so an attempt's fence exists only transiently. A future history table
   must snapshot the fence while the attempt runs, not derive it later.
3. **Invariant lives in one code path (CONFIRMED).** `CHECK (attempt >= 1)` guards the domain but not the
   transition rule (only `failed → queued` may increment). Any future backfill, admin tool, or second
   writer can bump the counter silently. The atomicity today is by convention in one store method.
4. **Two different caps, one column (CONFIRMED, unresolved).** `domain/run.rs:325` sets
   `MAX_ATTEMPTS = 3` for routine-woken runs (REQ-024, `not_before` in `0016`), while interactive retries
   (REQ-012) have no documented cap. The column and its CHECK make no distinction. GUESS: the policy
   belongs in the domain/store, not in this column's constraint.
5. **Spec field with no column (CONFIRMED).** The data model lists `retryOfRunId` on `Run` but no
   migration adds it; do not mistake `attempt` for a retry-link. Resolving that is a separate module.

## 3. Concrete upgrades for v0.4.0 (additive, with risks)

**U3 — Document and lock the invariant with tests (do first, zero schema risk).** Add a new migration
with `COMMENT ON COLUMN runs.attempt` citing REQ-012 and the "one at birth, +1 per failed→queued
retry" rule; extend `crates/infra/tests/m1.rs:1143-1170` with `req_012_*`-named assertions that a retry
increments by exactly 1, is refused from `completed`/`cancelled`, and that claim/lease paths never
change `attempt`; add a property test over the transition table. Risks: (a) comment-only DDL still needs
a version bump; (b) golden `run-retry.json:18` is byte-pinned — keep `attempt: 2`; (c) never weaken a
gate to make a test green.

**U1 — Materialize the spec `Attempt` entity as a new additive table.** New migration (e.g.
`0060_run_attempts.sql`): `run_attempts(run_id FK→runs ON DELETE CASCADE, space_id, attempt_no, fence,
status, error, started_at, finished_at, created_at, PRIMARY KEY(run_id, attempt_no))` with the same
RLS loop as `0010`/`0011`. Keep `runs.attempt` as the denormalized counter the RPC already returns —
the table is the history behind it. Risks: (a) dual write must share the `retry` transaction and the
claim/finish updates, or the counter and history diverge; (b) GUESS when a row is born (on first lease
vs. on requeue) — record the choice in `docs/STATE.md`; (c) backfilling old runs with `attempt > 1` has
no fence/outcome — seed best-effort rows with NULLs or leave old runs counter-only; (d) record the fence
while the attempt is live (finding 2); (e) internal only, no DEVIATION, but app writes are coordinated.

**U2 — Add a defensive DB guard in a new migration (after U1's invariant is agreed).** A `BEFORE UPDATE`
trigger on `runs` that permits `attempt` to change only by exactly +1 and only when status goes
`'failed' → 'queued'`, and rejects every other modification of the column. Risks: (a) must allow
`boma.is_system()` maintenance paths or define an explicit bypass, or legitimate backfills fail;
(b) triggers also fire for admin tools — cover with integration tests and the memory twin; (c) stable
error code/message, no internal details leaked to clients; (d) if retry ever becomes two statements the
guard breaks — document the invariant next to `domain/run.rs:269`; (e) optional for v0.4.0, defer if the
store invariant test (U3) is considered sufficient.

**U4 — Make the cap policy explicit; gate any DB cap on a decision.** Additive doc/ADR work only for
v0.4.0: state that `MAX_ATTEMPTS = 3` applies to routine runs and interactive retries stay unlimited.
If a DB assertion is later wanted, use an additive CHECK scoped by `routine_id` (`0015`), e.g.
`attempt <= 3 OR routine_id IS NULL`. Risks: (a) a cap changes externally visible error behavior —
DEVIATION + `docs/PARITY.md` + owner approval, do not ship silently; (b) confirm `usecases.rs:5070`
today before adding any CHECK; (c) GUESS about an interactive cap — safe default is "unlimited,
documented", not a constraint.

## 4. Ordering, invariants, non-goals

Order: **U3 → U1 → U2 → U4**. Invariants: `attempt >= 1`; exactly +1 only on `failed → queued`; counter
matches history when U1 lands; fence snapshotted per attempt; golden `attempt: 2` stable; RLS on `runs`
never weakened; new table tenant-scoped; `req_012` in test names for the trace gate. Non-goals: no edit
to `0012`, no type change (`integer`→`bigint`), no `retry_of_run_id` work, no new dependency, no
feature outside the spec, no ADR/spec/gate edit by this worker.

## 5. Evidence & tests to add

- CONFIRMED (read this session): `0012_attempt.sql:1-6`; `0002_m1.sql:190-213`; `0010`, `0011`, `0016`;
  `spec/02-data-model.md:58-59`, `01-requirements.md:99-105`, `07-glossary.md:12`, `golden/run-retry.json`;
  `domain/run.rs:269-270,325`, `api/wire.rs:229`, `stores/runs.rs:445-457`, `infra/tests/m1.rs:1143-1170`.
- GUESS: attempt-row birth time; backfill strategy for old runs; whether a trigger guard is wanted;
  interactive-retry cap appetite; whether goldens pin any error string beyond `attempt: 2`.
- Suggested names: `req_012_retry_increments_attempt_by_exactly_one`,
  `req_012_only_failed_runs_retry`, `req_012_claim_never_changes_attempt`,
  `req_012_run_attempts_history_matches_counter` (U1), `req_012_attempt_guard_rejects_backfill` (U2).

Line budget: <120. Status: proposal only; READ-ONLY worker, no code changed and no tests run.
