# 0187 — `crates/app/src/worker.rs` — architecture (v0.4.0)

- Worker: 187 · Lens: architecture · Target: v0.4.0 · Read-only review; no product code edited.
- Module: 846-line `Worker` (`step` + 8 helpers) + `PassOutcome` + 3 unit tests. Spec anchors: REQ-024
  (dispatch / retry / coalescing — `spec/01-requirements.md:196`), REQ-001/003/012/019/020/021,
  `spec/06-decisions-needed.md:42` (paused states vs queue), `spec/TRACE.md:41`.
- Read here: `worker.rs`; `usecases.rs` (`claim_run`, `set_run_status`, `write_reply`,
  `park_tool_calls`, `auto_retry_routine_run`, `record_routine_completed/failed`, `dispatch_routines`);
  `crates/domain/src/run.rs` (status table); `crates/infra/src/stores/runs.rs` (`set_status`, `retry`);
  `error.rs` proposal 0169. CONFIRMED = read in this session; GUESS = inferred.

## The run loop today (CONFIRMED)

`step()` (lines 86–206, `#[allow(clippy::too_many_lines)]`) owns the whole lifecycle inline:
`claim_run` → `set_run_status(Running)` → `announce_run_started` → `loop` over `provider_pass` →
a six-step success tail (`still_live`, `write_reply`, `set_run_status(Completed)`, push, metrics,
`record_routine_completed`, `complete_helper_run`). `PassOutcome`'s five variants terminate
asymmetrically: `Answered` may re-loop or fall into the tail, `ToolsDispatched` re-loops, and
`Suspended`/`Cancelled`/`Failed` each early-return `Ok(true)` from inside the match. There is no
single "one terminal transition per run" choke point; only the fence and the caller's discipline
prevent two writes.

**Transition authority is split four ways.** `claim_next_queued` enforces `queued→leased` in SQL
(`WHERE status='queued'`); `step` goes through `usecases::set_run_status`, a thin pass-through to
`stores.runs.set_status`; `fail_run` (line 679) calls the store **directly**; `retry` (runs.rs:445)
hard-codes `failed→queued` as a SQL literal; `park_tool_calls` reaches `WaitingInput` via
`create_waiting_card`. Critically, the domain table `RunStatus::can_transition_to` / `transition_to`
(run.rs:100,128) has **no production caller** (grep: definition + tests only), and the store's
`set_status` guards only the lease fence — it does not check current status. So the comment at
worker.rs:98 ("the state machine decides, the store only applies") is aspirational: today the
caller decides and the store applies whatever it is told, provided the fence matches.

**REQ-024 is spread across three boundaries.** Coalescing lives at enqueue in
`dispatch_routines` (`find_open_routine` + `record_skip`, usecases.rs:5260); the retry/backoff decision
lives in the `provider_pass` `Err` arm (`trigger == Routine && retryable && auto_retry_routine_run`,
worker.rs:457); the failure counter and pause notice live in `record_routine_failed`; the success
reset lives in the `step` tail. The worker knows trigger names and thresholds but owns no single
routine-run policy, and `step_started: Instant` is threaded through `provider_pass` and both
`announce_*` helpers, mixing observability into control flow.

**Non-atomic, non-idempotent completion (CONFIRMED; impact GUESS).** `write_reply` inserts with
`client_nonce: None` (usecases.rs:6641) even though the messages store dedupes by `(thread, nonce)`.
A reply written before a failed `Completed` transition — or a lease-expired run re-claimed after
`retry` — can append a second bot message with no key to collapse it.

## Concrete upgrades for v0.4.0 (each with a risk)

1. **Make `PassOutcome` a pure pass report; add one terminal choke point.** Introduce
   `enum RunTerminal { Completed(String), Suspended, Cancelled, Failed(Failure) }`; the loop maps
   each pass to either "continue" or one terminal, then a single `finish_run(terminal)` writes
   exactly one terminal status plus its notices/push/metrics. Bundle `step_started` into a small
   `RunAttempt` context passed by reference. *Risk:* `Suspended` currently reaches `WaitingInput`
   inside `park_tool_calls`, not in `step`, and push/metric text is externally visible — freeze
   every status/notice/event with golden tests first; keep the suspension write where it is and
   have `finish_run` treat it as "already terminal" rather than duplicating it.

2. **Route every transition through the domain table.** Have `usecases::set_run_status` re-read the
   run and call `RunStatus::transition_to` under the fence, then use that usecase from `fail_run`,
   `park_tool_calls` and the tail; delete the direct store call at worker.rs:679. Keep the atomic
   SQL precondition in `retry` but add a test cross-checking it against `can_transition_to(Failed)`
   so the two sources cannot drift silently. *Risk:* a re-read can observe a state newer than the
   claimed one (cancel racing a completion) — return `AppError::Conflict` and let the loop notice
   via `still_live` rather than forcing the move; do not weaken the fence check.

3. **Give REQ-024 one owner: a `RoutineRunPolicy` injected into `Worker`.** Group
   `auto_retry_routine_run` / `record_routine_failed` / `record_routine_completed` behind one
   `on_terminal(&run, outcome)` method so the loop never branches on `RunTrigger` or thresholds
   itself; keep `dispatch_routines` coalescing at enqueue but expose the same policy type there.
   *Risk:* coalescing decides *which* wake wins at a different epoch from the run's outcome — moving
   it could change that decision, so only share the policy interface, not the `find_open_routine`
   query; verify the `coalesced` audit entry and `@once` deactivation stay byte-identical.

4. **Bound the whole run, not just its pass counts.** Wrap `step()`'s loop in a wall-clock deadline
   (`tokio::time::timeout`) and pass a remaining-deadline into `provider_pass`/`complete`; on expiry,
   fail with a retryable `Unavailable` so REQ-024 backoff applies and the lease is released cleanly.
   *Risk:* today only `MAX_STEERING_PASSES=4` and `MAX_TOOL_CALLS=8` bound the loop, so a timeout is
   an externally visible behavior change — ADR + `docs/PARITY.md` entry, generous configurable
   default, and prove the terminal write happens before the timer future is dropped (no stranded
   lease, no reply after failure).

5. **Idempotent reply write.** Derive a stable `client_nonce` from the run id (e.g.
   `run-reply:<id>`) in `write_reply` and let the existing store dedupe collapse a retried write.
   *Risk:* `client_nonce` is part of the message shape and goldens may assert `null` — first confirm
   the store's nonce path returns the existing row rather than erroring, and check every golden and
   wire expectation; if the identity change is visible, file it as a DEVIATION before shipping.

## Explicit non-goals

No change to the claim/lease protocol (REQ-012), fence values, status vocabulary, cron/coalescing
semantics, `PassOutcome` chunk streaming (REQ-020), helper reporting (REQ-019), or the privacy of
failure details (REQ-021). No new trigger or status outside `run.rs::ALL`, no second scheduler, no
crate-boundary shuffle — this is loop structure and transition ownership only. Verify with: the
REQ-012/019/020/021/024 test names + `dispatch_routines` coalescing test (`testkit/build.rs:2812`),
`cargo clippy -D warnings`, and a golden diff of run status/event traces.
