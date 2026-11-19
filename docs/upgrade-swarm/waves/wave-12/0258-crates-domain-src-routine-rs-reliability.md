# routine.rs — reliability hardening proposal (v0.4.0)

Worker #258 · MODULE `crates/domain/src/routine.rs` · LENS reliability · READ-ONLY audit.
Claims are CONFIRMED from this tree in this session unless marked GUESS. Requirements: REQ-023/024/025.

## Failure modes

### FM-1 — Invalid schedules are accepted, mislabelled, or silently overridden (CONFIRMED)

- `parse_trigger`, `validate_timezone` and `next_run_at` all return `DomainError::InvalidName` for
  malformed crons, unknown zones and unparsable `now`; a caller cannot tell which field is wrong,
  and the text echoes raw input (`format!("`{entry}`...")`) into a domain error.
- `@once` short-circuits at lines 132–140 **before** `validate_timezone` (141), so a garbage zone is
  accepted whenever `@once` is present. `routines_create` hides this only because it validates the
  zone separately (`usecases.rs:4946`); any other caller of `next_run_at` is unprotected.
- `@once` mixed with crons is accepted: the once branch ignores every cron, then the routine is
  deactivated. A member who believes they have "daily + one-shot" silently loses the recurrence.
  `validate_triggers` (106–116) also accepts duplicates (`["@once","@once"]`, repeated crons).
- An impossible-but-parseable cron (e.g. `0 0 30 2 *`) passes `parse_trigger`; it fails only at
  `find_next_occurrence` (151–153). `routines_create` skips `next_run_at` when `active=false`
  (`usecases.rs:4960-4964`), so it can be stored and surface only at first wake (no enable/update
  RPC found in `rpc.rs:465-476`; GUESS about an operator-CLI path).
- `instant + Duration::minutes(5)` (138) is unchecked: a value near `DateTime::MAX` can panic the
  process instead of returning `Err` (GUESS that no current caller supplies one).
- DST: `find_next_occurrence` works on local time; nonexistent/ambiguous times around a
  spring-forward or fall-back may be skipped or duplicated. No test covers it (GUESS impact).

### FM-2 — Duplicate wakes / duplicate runs (CONFIRMED mechanism)

- `next_run_at` returns a bare `String` with no wake identity. `dispatch_routines`
  (`usecases.rs:5280`) recomputes it from dispatch `now`, not the persisted due instant: a retry or a
  late dispatcher produces a different next wake and silently drops missed occurrences (no catch-up
  policy is stated in the spec).
- `@once` is `now + 5 minutes`, so a retried dispatch keeps moving the fire time forward. A crash
  after `runs.create` but before `record_fired` leaves the routine due; the next sweep fires again —
  a duplicate one-shot with no way to recognise the first.
- The domain cannot express "terminal wake": callers re-derive it by raw string compare
  (`usecases.rs:5277`). With a plain `SELECT` in `routines::due` (routines.rs:169-183, no
  `FOR UPDATE SKIP LOCKED`) and in `runs::find_open_routine` (runs.rs:303-321), two dispatchers can
  both see "due + no open run" and both create a run. Coalescing is post-hoc, not a claim.
- Audit noise: one wake writes two `fired` rows — `record_fired` (routines.rs:236) and
  `record_completed` (routines.rs:334) — which reads as a duplicate fire in `routine_runs`.

### FM-3 — State transitions are implicit and unguarded (CONFIRMED)

- Lifecycle is three loose fields (`active: bool`, `next_run_at: Option<String>`,
  `account_failures: u32`). `record_failure` sets `active=false` but keeps a stale `next_run_at`
  (routines.rs:269-271); re-enabling without recomputing yields an immediately-due routine. Nothing
  forbids `active && next_run_at = NULL`, or re-arming a spent `@once`.
- No domain transition function exists (unlike `RunStatus::transition_to` in run.rs). Every write is
  ad-hoc SQL, with no `InvalidTransition` error and no single authority to test.
- `record_failure` increments `account_failures` with no dedup on `run_id` (routines.rs:270) and
  `routine_runs` has no unique `(routine_id, run_id, outcome)`; a repeated failure notice for the
  same run double-counts toward the 5-failure pause threshold (`usecases.rs:5042,5208-5214`).

## Concrete hardening (ordered, test-first)

1. Validate everything before dispatch: in `next_run_at`, parse timezone and schedule once, reject
   `Once` mixed with `Cron` and duplicate entries in `validate_triggers`, then branch; replace the
   unchecked `+ 5 min` with a checked add returning `Err`. Keep the 5-minute default.
2. Add typed `DomainError::{InvalidCron, InvalidTimezone, InvalidInstant, NoNextRun}` (mirror
   `req_040`: never echo raw input) and map them to the existing API error shape at the boundary.
3. Return a `Wake { at: UtcInstant, terminal: bool }` (or reuse the dead `RoutineTriggers` type) and
   make `next_run_at(crons, zone, anchor)` take the *persisted due instant* as anchor, so reprocessing
   the same wake is deterministic. Replace the caller-side `@once` string scans with `wake.terminal`.
4. Add a `RoutineState` enum (`Active { next }, Paused { failures }, Spent`) with
   `transition(event) -> Result<_, DomainError::InvalidTransition>`; `Spent` is terminal,
   `activate` recomputes `next`, `record_failure` is idempotent per `run_id`. Drive store writes from
   it and add a CAS `WHERE next_run_at = <old>` (mirroring the run-fence pattern) so a late dispatcher
   loses.
5. Tests (REQ-named, test-first): `req_023_once_cannot_be_mixed_with_crons`,
   `req_023_invalid_timezone_rejected_even_for_once`, `req_023_extreme_now_does_not_panic`,
   `req_024_same_due_instant_yields_same_wake`, `req_024_spent_routine_cannot_reactivate`,
   `req_024_repeated_failure_for_one_run_counts_once`, plus proptest over parser inputs and DST cases
   for `Europe/Paris` / `America/New_York`.

## Risks and mitigations

- **DEVIATION risk:** rejecting `once+crons`, duplicates and bad zones changes accepted inputs. Audit
  fixtures/logs first, record in `docs/PARITY.md`, ask the owner before enforcing (GUESS today).
- New error variants touch the `InvalidName → 422` mapping site (location GUESS — verify) and any
  golden wire text; keep accepted-input strings byte-stable.
- DST/checked-add fixes can change `next_run_at` output near boundaries: a parity surface — check the
  original before changing, never silently.
- CAS on `next_run_at` and a unique run-id index need a migration; stage after steps 1–3 and
  coordinate with the run-state worker (0260) to reuse one error/timestamp vocabulary.
- "Skip missed wakes" vs "catch up" is a policy the spec does not state (GUESS): write it in
  `docs/STATE.md` and keep current skip behaviour as the default.

## Recommendation

Ship 1 + 2 + 5 first (smallest unit that stops invalid schedules and panics), then 3 + 4 behind a
CAS migration; leave the catch-up policy as an explicit open question.
