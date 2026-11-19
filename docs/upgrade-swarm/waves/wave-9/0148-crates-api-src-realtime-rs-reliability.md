# Worker #148 — `crates/api/src/realtime.rs` — LENS: reliability — v0.4.0 proposal

- **Status:** PROPOSAL (read-only review; no source changed). CONFIRMED = read from `realtime.rs`, `infra/src/events.rs` and `AGENTS.md` in this worktree; runtime predictions are GUESS.
- **Scope:** `threads.subscribe` SSE stream (REQ-005, REQ-020). Producer task (82-131), mpsc queue (27, 80), heartbeat path (93-128, 150-172), broadcast bridge (`events.rs:18,70-79`).

## Summary

The design looks sound at a glance — bounded queue (64), sequence dedupe, heartbeats, log-backed replay — but every reliability claim in the module comments ("resumes without losing an event", "too slow; resumes on next connection") depends on client behavior the server never signals. Four failure modes dominate: a detached task with no cancellation, a send path that stalls the whole select loop under backpressure, gaps that are swallowed rather than repaired, and a per-subscriber heartbeat that amplifies into a broadcast storm. (CONFIRMED/GUESS)

## Failure modes

### Dropped clients and cancellation
- **D1 (HIGH, CONFIRMED):** the producer is a detached `tokio::spawn` (82) with no cancellation token and no graceful-shutdown registration; the only disconnect signal is a failed `sender.send()`, so a silent client holds a task plus a broadcast receiver until the next send or tick.
- **D2 (HIGH, CONFIRMED):** on each tick, `heartbeat()` appends to the DB log and publishes (156-170) *before* the `sender.send()` at 120 is checked. A client that disconnected between ticks still forces one durable append + one broadcast publish before the task exits.
- **D3 (MED, CONFIRMED):** on `RecvError::Closed` (110) the stream simply ends, with no terminal frame or reconnect hint; browsers auto-reconnect but get no `retry:` guidance.
- **D4 (LOW, CONFIRMED):** the `frame()` fallback `.data("{}")` (143) emits a syntactically valid but empty payload on JSON-encode failure — silent corruption instead of an error.

### Backpressure
- **B1 (HIGH, CONFIRMED):** `sender.send().await` sits inside the `select!` branch body (88, 105, 120). While it blocks (queue full at 64), the loop is not polling `live.recv()`, so the broadcast receiver (capacity 256, `events.rs:18`) stops draining and can go `Lagged` — exactly the silent loss the comment at 111-113 disclaims.
- **B2 (HIGH, GUESS):** a hung-but-open client never drains the mpsc; `send()` waits forever, leaking the task. No `send_timeout`, no queue-depth metric.
- **B3 (MED, CONFIRMED):** no cap on concurrent subscribers per thread, so N slow clients multiply the above.

### Reconnect / gap detection
- **G1 (HIGH, CONFIRMED):** `Lagged(_)` (113) is swallowed — no backfill, no gap frame, no close. The client sees frames simply stop and keeps waiting.
- **G2 (HIGH, CONFIRMED):** no SSE `id:` field is ever set (`frame()` 138-144), so browser-native `Last-Event-ID` reconnect cannot work; the module's central promise rests entirely on the client tracking seq out of band.
- **G3 (MED, CONFIRMED):** live events set `last_seq = event.seq` (104) with no contiguity check, so a missing seq is accepted as normal. `after_seq` (69) is unvalidated — negative or future values silently over-replay or stall forever.
- **G4 (MED, CONFIRMED):** history replay (70-75, 86-91) loads the whole range into memory with no limit; `after_seq: 0` on a large thread forces unbounded replay.

### Broadcast storms
- **S1 (CRITICAL, CONFIRMED):** every subscriber owns its own ticker (93) and calls `heartbeat()`, which appends + publishes (170). With N clients on a thread that is N log appends and N broadcasts *per 15 s*, and each broadcast wakes all N subscribers — O(N²) frame amplification plus a bloated event log. Worst reliability defect in the module.
- **S2 (MED, GUESS):** heartbeats carry increasing seqs, so each client forwards every other client's heartbeat (101-107), compounding S1.

## Concrete hardening (v0.4.0)

- **P0 — storms (S1/S2):** make heartbeat a single shared per-thread task in the app/publisher layer — one append + one publish per interval per thread, fanned out by the existing broadcast channel, with teardown when the last subscriber leaves. Or replace log heartbeats with SSE comment keepalives (`: keep\n\n`) written straight to the mpsc — zero DB/broadcast cost — *if* the spec permits (decision below).
- **P0 — cancellation (D1/D2):** add `_ = sender.closed() => return` and an app `CancellationToken` branch to the `select!`; precheck `sender.is_closed()` before append/publish in the tick; register spawned tasks in a tracked set drained at shutdown (AGENTS.md).
- **P1 — backpressure (B1/B2/B3):** replace blocking `send().await` with `try_send` (or a `timeout`) in a dedicated branch; on full, emit a gap/reconnect frame and close so the client replays via `after_seq`, converting silent loss into visible recovery. Make queue depth configurable, add `boma_realtime_queue_depth` / `boma_realtime_lagged_total` metrics, and cap subscribers with 429/close.
- **P1 — gaps/reconnect (G1-G4):** on `Lagged(n)`, re-query `events.since(..., last_seq)` and replay the missed range before resuming live (with a watermark to avoid duplicating frames already queued); emit `id: <seq>` and `retry:` on frames; detect `event.seq > last_seq + 1` and backfill; validate/clamp `after_seq` (400 on negative, clamp future to head); paginate/cap history replay.
- **P2 — polish (D3/D4):** terminal `gap`/`eof` event on publisher close; make encode failure a real error frame or a dropped frame with a metric, not `{}`; keep REQ-005/REQ-020 test names on every new behavior.

## Risks & mitigations

- **DEVIATION exposure:** `id:`/`retry:`, comment keepalives, gap/eof frames, close-on-full, subscriber caps and `after_seq` rejection all change the wire surface → record each in `docs/PARITY.md` and obtain operator approval (AGENTS.md).
- **Seq invariants:** shared heartbeat + backfill assume every append increments seq by exactly 1 and `events.since` is ordered/atomic; verify store semantics (GUESS) before trusting contiguity checks.
- **Replay duplication:** backfill can re-emit events already buffered in the mpsc; guard with a replay watermark or pause live forwarding during replay.
- **Shared-task lifecycle:** a per-thread scheduler must stop when the last subscriber leaves or it keeps appending heartbeats forever (the publisher intentionally keeps channels forever).
- **Config:** queue depth, subscriber cap and interval need a validated typed config with safe defaults; visible reconnect churn is the honest trade for no silent loss.

## Acceptance tests (REQ-005 / REQ-020 names)

- `req_005_client_disconnect_stops_heartbeat_appends` — D2.
- `req_005_slow_consumer_gets_gap_and_replays` — B1/G1.
- `req_005_frames_carry_sse_id_and_retry` — G2.
- `req_005_heartbeat_is_shared_per_thread` — S1.
- `req_005_future_after_seq_is_clamped` — G3.
- `req_005_subscriber_cap_is_bounded` — B3.

## Decisions needed (GUESS → operator/spec)

1. Are heartbeat events part of the REQ-005 wire contract, or may they become SSE comment keepalives (S1/P0)?
2. Confirm each append increments seq by 1 and `events.since` ordering/atomicity (G3, backfill).
3. Is browser-native `Last-Event-ID` reconnect acceptable as a DEVIATION (G2)?
