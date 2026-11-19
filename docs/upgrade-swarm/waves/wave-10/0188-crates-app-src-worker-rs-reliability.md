# v0.4.0 reliability proposal — `crates/app/src/worker.rs` (module 188, lens: reliability)

Scope: REQ-006, REQ-012, REQ-013, REQ-014, REQ-019, REQ-021, REQ-024. Status tags follow
AGENTS.md: **CONFIRMED** = verified by reading this file, its use cases, stores and driver this
session; **GUESS** = inferred, needs a test or command before shipping.

## 1. Failure modes

### FM-1 — Agent crash / deploy recovery (REQ-013, REQ-014)
- **CONFIRMED** `step()` claims and flips to `Running` (88-108), then keeps all progress in locals
  (`text`, `passes`, `tool_passes`, 115-117). A crash or dropped task discards every streamed chunk;
  `claim_next_queued` will not hand the run out again until the 1 h TTL lapses
  (`infra/src/stores/runs.rs:26,348-350`). Recovery latency is up to an hour.
- **CONFIRMED** Recovery is at-least-once, not exactly-once as REQ-014 requires. Chunks are
  ephemeral `message-update` events (407-423); the durable reply is written only at the end
  (166-174) with `client_nonce: None` (`usecases.rs:6631`). Crash after `write_reply` but before
  `set_status(Completed)` re-queues the run and posts a second reply.
- **CONFIRMED** The worker driver (`bin/src/commands.rs:335-362`) has no shutdown signal: it loops
  forever and `?` kills the process on any `Err` (355-358). SIGTERM abandons the in-flight run to
  the TTL; REQ-013's "hands its runs to another process" is not implemented here. (**GUESS** that
  the `serve` command's graceful shutdown does not cover the worker command.)

### FM-2 — Retry loops / amplification (REQ-021, REQ-024)
- **CONFIRMED** Auto-retry covers only `RunTrigger::Routine` + `AppError::Unavailable` (436-463;
  `error.rs:49-51`). Interactive runs with a retryable provider failure fail permanently — allowed
  by REQ-021, but worth an explicit decision.
- **CONFIRMED** The failed push notice is sent inside `fail_run` (690-700) *before* the retry
  decision, so a routine that is immediately re-queued up to `MAX_ATTEMPTS` (`domain/src/run.rs:325`)
  emits a "failed" notice each time. Operators see failure spam for a healthy backoff chain.
- **CONFIRMED** `retry_delay_seconds` is a pure function of `attempt` (`run.rs:339-344`) with no
  jitter: routines that fail together retry in lockstep at the same `not_before` — a thundering
  herd against the backend that just failed (**GUESS** on real-world impact).
- **CONFIRMED** Per-run caps exist (`MAX_STEERING_PASSES=4`, `MAX_TOOL_CALLS=8`, 18-29, 494-511) and
  are non-retryable `Rule` errors, so there is no infinite loop. But up to 12 passes rebuild the
  catalogue, memory, skills and compaction each time (295-365) with no per-run wall-clock deadline.

### FM-3 — Cancelled runs (REQ-006, REQ-019)
- **CONFIRMED** `still_live` (652-665) only tests "status not terminal" — no fence, no owner.
  `cancel_run` writes `Cancelled` with fence `None` (`usecases.rs:6514-6529`) and the SQL
  `set_status` has no terminal guard (`infra/src/stores/runs.rs:399-409`). A cancel landing after
  `still_live` but before `write_reply` lets a late reply be written and the run flip to
  `Completed`, erasing the member's stop.
- **CONFIRMED** Cancellation is checked between streamed chunks (402-405) but **not** inside
  `dispatch_tool_calls` (483-574): read tools keep executing after a stop until the turn ends,
  violating REQ-006's "no further tool call starts".
- **CONFIRMED** A helper cancelled at the post-loop `still_live` (162-165) returns early *without*
  `helper_finished`, so its parent stays suspended forever. The `PassOutcome::Cancelled` branch
  (150-158) does call it — the two cancellation paths disagree.
- **CONFIRMED** No worker-level timeout wraps `provider.complete()` (378); a hung provider holds a
  slot and a lease. Bounds are adapter-specific (ACP 600 s, OpenAI HTTP 60 s), not a run deadline.

### FM-4 — Stale leases (REQ-012, REQ-014)
- **CONFIRMED** The lease is set once at claim (`runs.rs:359`) and never renewed. A legitimate run
  over 1 h becomes reclaimable while still executing. `Run::holds_lease` (`run.rs:359-361`) exists
  but the worker never calls it; `still_live` is the only guard.
- **CONFIRMED** Fence is enforced only on `set_status` and `set_steering_consumed`
  (`runs.rs:409,278`). `write_reply`, event append, tool-result append, helper completion and
  routine counters carry no fence — a displaced worker can still land side effects.
- **CONFIRMED** On fence mismatch `set_status` returns `Conflict` (`runs.rs:423-427`) and `?`
  propagates out of `step()` (100-108, 175-183, 678-689); the driver's `?` then kills the daemon.
  There is no `run.lease_lost` event or metric, and no clean `Ok(true)` abandonment.
- **CONFIRMED** The in-memory store guards terminal rows (`testkit/src/memory.rs:2555-2559`) but
  SQL does not; `can_transition_to` is never consulted on these writes. Test and production
  backends disagree about what a late write may do.

## 2. Concrete hardening
1. Contain failures in the driver: map `Conflict`/transient `Unavailable` from `step()` to
   "yield, keep polling" and count `boma_worker_step_errors_total{reason}`. One run must never end
   the process. Add a SIGTERM flag that stops claiming and drains in-flight steps.
2. Add `renew_lease(owner, fence)` (CAS) to the runs port and call it each tool round, steering
   pass and N stream chunks; shrink the TTL to minutes, refreshed, instead of 1 h. Renewal failure
   is a cancellation signal, not a silent continue.
3. Put `holds_lease(owner, fence)` into a single `guard()` before every write; thread the fence
   into store APIs. Add a stable idempotency key `(run_id, attempt)` to `messages.insert` so a
   recovered run cannot post a duplicate reply.
4. Add `WHERE status NOT IN ('completed','failed','cancelled')` (and transition validation) to
   SQL `set_status`, matching the memory store; make the final reply + completion one CAS so a
   cancel racing completion deterministically wins.
5. Check `still_live` before **each** read tool call and before parking; always route every
   early-return path (including 162-165) through `helper_finished`, idempotently.
6. Checkpoint the accumulated answer and pass index per pass; on recovery resume rather than
   restart, and skip re-streaming when a reply for this `(run_id, attempt)` exists.
7. Wrap `provider.complete()` in a configurable `tokio::time::timeout` and fail legibly
   (`Failure::from(AppError::Unavailable)` keeps REQ-021 wording). Bound total passes by a deadline.
8. Move the routine "failed" notice after the retry decision (`retry_scheduled` on the retry path)
   and add deterministic jitter from `run_id` to `not_before` to break lockstep retries.
9. Emit `run.lease_lost`, `run.retry_scheduled`, `run.cancelled_mid_turn` with fence/attempt/owner.

## 3. Risks / deviations
- R1 — Idempotency key changes `client_nonce` semantics visible in messages: **DEVIATION**, needs
  approval and golden updates (docs/PARITY.md).
- R2 — Moving the routine failure notice changes chat text for retryable failures: **DEVIATION**.
- R3 — A short TTL with slow renewal can cause premature takeover under load; benchmark p99 and
  tune before shipping — the opposite error is duplicate execution.
- R4 — Terminal CAS may break tests that write to terminal rows; audit before enabling.
- R5 — Provider deadline may truncate long generations; make it configurable, measure first.
- R6 — New store methods touch ports, infra and testkit: no external deviation, but three backends
  plus a migration.
- R7 — `helper_finished` on all paths can double-post unless it too is idempotent.

## 4. Acceptance tests (REQ id in name)
- `req_014_crashed_worker_run_is_reclaimed_without_duplicate_reply`
- `req_013_shutdown_hands_in_flight_run_to_another_worker`
- `req_012_stale_fence_worker_cannot_write_reply_or_status`
- `req_012_lease_is_renewed_during_a_long_run`
- `req_006_cancel_during_tool_round_stops_further_tools`
- `req_006_cancel_racing_completion_leaves_run_cancelled`
- `req_019_cancelled_helper_always_notifies_parent`
- `req_024_routine_retry_has_jitter_and_a_single_notice`
- `req_021_hung_provider_fails_run_with_a_deadline_reason`
