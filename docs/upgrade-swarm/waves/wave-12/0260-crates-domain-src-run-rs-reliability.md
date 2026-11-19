# run.rs — reliability hardening proposal (v0.4.0)

Worker #260 · MODULE `crates/domain/src/run.rs` · LENS reliability · READ-ONLY audit.

Claims below are CONFIRMED when read from this tree in this session; inferred impact is GUESS.

## Current state (CONFIRMED)

- `RunStatus::can_transition_to` / `transition_to` (lines 98–134) encode the full table and return
  `DomainError::InvalidTransition`; the test at line 442 checks it against a hand-written `ALLOWED`.
- Grep across `crates/` shows `transition_to` is called **only inside run.rs tests**. No app, infra
  or api write path invokes it; `set_run_status` (`app/src/usecases.rs:6574`, doc: "refusing a move
  the state machine forbids") forwards directly to the store without checking the table.
- `infra/src/stores/runs.rs:399–409` writes `SET status = $2` guarded **only** by `lease_fence`;
  there is no `status NOT IN ('completed','failed','cancelled')` and no current-status predicate.
  By contrast `testkit/src/memory.rs:2555–2565` guards terminal status and fence, but not the table.
- `claim_next_queued` (`runs.rs:342–360`) reclaims `leased|running` rows whose `lease_expires_at`
  has passed with `FOR UPDATE SKIP LOCKED`, bumps the fence and sets `status='leased'`.
- `set_status` never clears `lease_owner`, `lease_fence` or `lease_expires_at` on completion
  (only `retry`, `runs.rs:447–453`, clears them). Finished rows keep stale lease fields.
- `holds_lease` (lines 354–361) compares owner + fence only; it has no clock and no status check.
- `is_terminal()` (lines 83–87) includes `Failed`, even though `Failed → Queued` (retry) is legal.
- Lifecycle fields (`started_at`, `completed_at`, `error`, lease clearing) are maintained by ad-hoc
  SQL and by the in-memory store independently; the domain owns none of that mutation.

## Failure modes

### FM-1 — illegal transitions are detected but never enforced (CONFIRMED mechanism)

The table is documentation plus unit tests, not a gate. SQL `set_status` accepts any `$2` from any
status; the memory store accepts any non-terminal → any status. Callers do read-then-write checks
(`pause_run`, `resume_run`, `cancel_run`, `usecases.rs:6452–6529`) with `fence: None`, so a second
request racing between `find` and `set_status` can drive e.g. `paused → cancelled → queued` or push
a run that just finished. The state machine's core guarantee exists only in `#[cfg(test)]`.

### FM-2 — terminal-state writes (CONFIRMED mechanism, GUESS impact)

- The SQL path has no terminal guard: a stray or delayed `set_status(... Completed/Running ...)` can
  overwrite `completed`/`cancelled`, re-opening a run whose reply/event was already emitted.
- Completion does not clear lease fields, so a finished row still satisfies `holds_lease(old_owner,
  old_fence)`; combined with the fence-only SQL guard, a stale worker can write to a terminal row.
- `set_status` uses `error = COALESCE($5, error)` (line 408), so a later failure reason cannot
  replace an earlier one; `completed_at` is set but never cleared by `set_status`, so a legal
  `Failed → Queued` done through this path would keep a stale finish timestamp.

### FM-3 — stale leases (CONFIRMED mechanism)

- `holds_lease` ignores `lease_expires_at`: until the reconciler reclaims, a worker past its TTL
  still "holds" the lease. The doc at line 356 assumes the fence was already bumped; that is an
  external precondition, not a domain invariant.
- The fence is a bare `i64` with `#[serde(default)]`; an omitted field deserialises to `0`, and
  `retry` also resets it to `0`, so fence identity is not globally monotonic (ABA risk if any old
  worker ever presents `0`; GUESS — no such caller found today).
- `claim_next_queued` reclaims a `running` worker that may still be alive and writing; correctness
  rests entirely on callers passing the current fence to every write.

## Concrete hardening (ordered, small steps)

1. Make the table the only write path: add `Run::transition(next, expected_fence: Option<i64>, at)
   -> Result<Run, DomainError>` that first calls `self.status.transition_to(next)`, then rejects
   terminal sources and lease mismatches, and updates `started_at`/`completed_at`/`error`/lease
   fields in one pure function. Have `set_run_status` build its write from this result.
2. Give the store a compare-and-set predicate: extend `set_status` SQL with the source status (and
   terminal exclusion) in the `WHERE` clause, and mirror it in the memory store; zero rows →
   `Conflict`. This closes the read-then-write race without a lock across `.await`.
3. Clear lease fields on every terminal transition in the domain helper and in SQL; keep `retry`
   as the sole exit from `Failed`. Add `is_immutable()` (true only for `Completed|Cancelled`) and
   keep `is_terminal()` for "not working" semantics, so the retry case stops being a footgun.
4. Add `Run::lease_state(owner, fence, now) -> LeaseState { Current, Expired, NotHeld }` with
   `holds_lease` delegating to `Current`; require callers to pass the clock value (domain stays IO
   free). Parse/compare timestamps via a shared typed `Timestamp`, not ad-hoc string ordering.
5. Replace the raw `i64` fence with a `LeaseFence` newtype (`#[serde(try_from = ...)]`, no silent
   `default`), or at minimum document why `0` is reserved and validate it at the boundary.
6. Tests (traceable names): `req_012_transition_to_is_the_only_write_path`,
   `req_012_concurrent_terminal_write_loses_the_race`, `req_012_retry_is_the_only_exit_from_failed`,
   `req_014_expired_lease_is_not_held`, `req_014_terminal_run_clears_lease_fields`, plus a
   `proptest` over all 81 status pairs asserting the store guard equals `can_transition_to`.

## Risks and mitigations

- **BREAKING (needs ADR/DEVIATION).** Clearing lease fields changes serialized JSON of finished
  runs, and rejecting previously accepted status pairs changes RPC behaviour. Mitigation: verify no
  fixture relies on stale lease fields, then record both in `docs/PARITY.md` before landing.
- Enforcing the table in SQL may reject a legitimate pair the table omits (GUESS: e.g. a holder
  releasing a `running` lease). Mitigation: dump the distinct `(old,new)` pairs from the running
  original first; add any missing edge to the table deliberately, never in SQL alone.
- A `Timestamp` newtype touches ids/time code owned by other workers: coordinate to avoid two
  competing time representations; otherwise pass an `expired: bool` computed by the caller.
- Extra `WHERE` predicates add negligible cost on an indexed `id` lookup; benchmark only if the
  run table grows large.

## Recommendation

Do steps 1, 2 and 6 together as one gated change: that is the smallest unit that actually makes the
state machine authoritative. Steps 3–5 follow once parity fixtures are checked; step 4 is the
highest-value standalone fix for worker-overrun safety.
