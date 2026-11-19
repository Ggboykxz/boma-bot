# v0.4.0 reliability proposal — `crates/infra/src/stores/runs.rs`

Lens: reliability. Worker #390. Read-only analysis in `before-m5`. Code shapes below are CONFIRMED by
reading `runs.rs`, `rows.rs`, `crates/domain/src/run.rs:100-133`, `crates/app/src/ports.rs:883-1032`,
`crates/app/src/usecases.rs:6452-6594`, `crates/app/src/worker.rs:98-180,668-702`,
`crates/testkit/src/memory.rs:2486-2574`, `crates/infra/tests/concurrency.rs:255-325`,
`migrations/0002_m1.sql:190-213`. Runtime outcomes are GUESS until the barrier tests run.
No code was changed.

## Failure modes

### F1 — Transition enforcement is domain-only (CONFIRMED)
`RunStatus::can_transition_to`/`transition_to` (`run.rs:100-133`) is exhaustive, but `set_status`
(runs.rs:386-429) sets `status = $2` with *only* the fence predicate (`WHERE id = $1 AND (fence…)`,
line 409): it never reads the current status, and the port has no expected-status argument
(`ports.rs:1011-1019`). `set_run_status` (`usecases.rs:6574-6594`) is documented as "refusing a move
the state machine forbids" but simply delegates — no check anywhere. `pause_run`/`resume_run`/
`cancel_run` (6452-6565) do a `find`, check in Rust, then write with `fence = None`: a classic
read-then-write TOCTOU, and the store's `fence IS NULL` branch makes the write unconditional.
The schema has no status CHECK (`0002_m1.sql:196`: `status text NOT NULL`). Consequences (GUESS):
illegal moves (`completed → running`, `failed → completed`, `running → paused`) can be stored, and a
status changed between a caller's `find` and its write is silently overwritten.

### F2 — Terminal writes are accepted (CONFIRMED, diverges from memory store)
The Postgres `set_status` has no `status NOT IN ('completed','failed','cancelled')` guard. The memory
store refuses *any* write to a terminal row (`memory.rs:2555-2559`). A cancellation (`fence = None`)
does not bump the fence, so a late worker still carrying the old fence can flip `cancelled →
completed`/`failed`; `fail_run` (worker.rs:679-688) with `Some(fence)` can overwrite a cancel.
`retry` (runs.rs:445-457) clears timestamps and lease but never clears `error`, and `set_status`
uses `error = COALESCE($5, error)` (line 408), so a retried run can display a stale failure reason
(REQ-021). Terminal rows are effectively re-openable; only `failed → queued` should ever leave the
terminal set.

### F3 — Stale lease: no renewal, long TTL, fence protects only the runs row (CONFIRMED shape)
`LEASE_TTL_SECONDS = 3_600` (runs.rs:26); the only writes to `lease_expires_at` are the claim
(runs.rs:359) and the retry reset (runs.rs:453). No renewal call site exists in the crate tree, and
`claim_next_queued` can reclaim a `leased`/`running` row whose expiry passed (343-360). A turn that
streams or runs tools longer than an hour is therefore reclaimed while its worker is alive (GUESS
outcome). The fence refuses that worker's *runs-row* writes, but tool calls, messages and events live
in other stores with no fence: both workers can emit external effects. Reclaim bumps the fence and
owner but does not clear the steering window or increment `attempt`, so redelivery looks like attempt
one and consumes stale steering seqs. Mixed clocks: expiry uses DB `now()`, while `started_at`/
`completed_at` are app-supplied strings. Runs in `waiting_input`/`waiting_takeover`/`paused` are never
reclaimable, so a worker that dies while waiting on a card wedges the run; `find_active_for_thread`
(lines 216-221) counts only `leased`/`running`, so a new turn can start on the same thread while the
old one is wedged.

### F4 — Stale status reads and silent no-ops (CONFIRMED)
`set_steering_from` (234-261) ignores `rows_affected` and returns `Ok(())` even on zero rows — a
steering window is silently dropped for a terminal or missing run; `set_steering_consumed` (289-291)
correctly returns `Conflict`. With no status CHECK, an unparseable status string makes
`rows::run`'s `RunStatus::from_str` (rows.rs:355) fail, so one bad row breaks `find` and poisons the
whole `list` page. `find_active_for_thread` treats an expired-but-not-yet-reclaimed lease as active.
`find_by_nonce` repeats `steering_from_seq, steering_consumed_seq, attempt` twice (lines 171-172 and
178-179): currently benign under sqlx name mapping (GUESS), but copy-paste drift on the REQ-002
replay path. The global oldest-first claim under `as_system` (332-352) can head-of-line block one
tenant behind another (GUESS).

## Concrete hardening (v0.4.0)

1. Enforce transitions at the write boundary: add an `expected` status to `set_status`; SQL becomes
   `UPDATE … WHERE id=$1 AND status=$2 AND (fence…) RETURNING …`, `Conflict` on zero rows, with
   allowed pairs derived from the domain table. Callers already know the status (claim, or their
   `find`). Keep same-status idempotent no-ops explicit (`paused→paused`, `cancelled→cancelled` are
   relied on by REQ-006/REQ-067). Risk: port ripples to memory store/testkit; pair list must stay in
   sync with the domain.
2. Make the invariant a DB invariant: add `runs_status_valid` CHECK plus a transition trigger for the
   domain pairs, and make terminal rows immutable except `retry`'s `failed → queued`. Risk: needs an
   ADR (alternatives: app-only, DB-only); backfill/repair dirty rows before enabling; extra write cost.
3. Close the terminal-write gap immediately (before the migration): add
   `AND status NOT IN ('completed','failed','cancelled')` to `set_status` and both steering writes;
   have `cancel_run` pass/read the fence (or bump it on cancel) so late workers are fenced out and
   treated as benign aborts. Preserve "cancel of a terminal run is a no-op" (REQ-006).
4. Lease liveness: add `renew_lease(id, fence) -> new_expiry` (`UPDATE … WHERE id AND fence AND
   status IN ('leased','running','waiting_input','waiting_takeover') … RETURNING`), called by the
   worker at each chunk/tool boundary; shorten the TTL to minutes; require the fence on every
   worker write, including steering. On reclaim, clear the steering window and add `attempt` so
   redelivery is visible. Use DB `now()` for all run timestamps. Risk: extra write per heartbeat;
   renewal `Conflict` must abort the turn; `waiting_*` needs an explicit expiry policy.
5. Deduplicate run-driven side effects: thread the fence (or a run-attempt id) into message/event/
   effect writes, or reuse the `client_nonce` idempotency key where the spec has one, so a reclaimed
   worker cannot double-send. Risk: cross-store port refactor; new keys are a DEVIATION entry.
6. Stop stale reads: `set_steering_from` returns `Conflict`/`NotFound` on zero rows; add the status
   CHECK (item 2) or skip-and-report unknown statuses in `rows::run` so one row cannot poison `list`;
   add `AND (lease_expires_at IS NULL OR lease_expires_at > now())` to the leased/running branch of
   `find_active_for_thread`; remove the duplicate columns in `find_by_nonce`. Fairness changes only
   if measured.
7. Error lifecycle: clear `error` on `retry` and on any transition into a working state, keeping the
   last reason only within the failed lifetime (REQ-021). Risk: chat-visible change → DEVIATION.

## Risks / deviations

- Items 1-3, 6, 7 change store/port semantics: keep the memory store and testkit in parity or
  conformance tests fail. Any externally visible transition rejection needs a `docs/PARITY.md`
  entry and owner approval; REQ-006 idempotent cancel must remain unchanged.
- Migration + trigger and any fairness change need an ADR. Sequence one commit each: migration →
  port → store → worker/use cases → tests. No raw connections; stay inside `with_tenant`.
- Renewal and dedup keys are larger refactors; fence-protecting side effects is the expensive one.
- All effects above are reasoned from code inspection; concurrency/duplicate-effect outcomes stay
  GUESS until the barrier tests below run.

## Test plan (REQ-tagged, real Postgres)

- `req_012_illegal_transition_is_refused_at_store` (completed→running, failed→completed).
- `req_012_stale_fence_cannot_resurrect_cancelled_run`; `req_006_cancel_of_terminal_stays_noop`.
- `req_014_expired_lease_reclaim_resets_steering_and_bumps_attempt`.
- `req_012_renewal_extends_lease_and_stale_worker_is_refused`.
- `req_021_retry_clears_error`; `req_003_set_steering_from_on_terminal_is_an_error`.
- `req_002_nonce_replay_returns_existing_run`; `req_005_unknown_status_row_does_not_poison_list`.
