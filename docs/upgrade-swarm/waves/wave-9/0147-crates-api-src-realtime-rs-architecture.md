# v0.4.0 architecture proposal — `crates/api/src/realtime.rs` (SSE / realtime hub)

Worker #147 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no code changed).

Status of claims: "current state" statements are CONFIRMED by direct reads of `realtime.rs`,
`crates/infra/src/events.rs`, `crates/app/src/ports.rs:1093-1105`, `crates/api/src/wire.rs:84-118`,
`crates/api/src/rpc.rs:346`, `spec/01-requirements.md` (REQ-005/020) and `spec/05-known-defects.md`.
Forward-looking items are proposals (GUESS until implemented and tested).

## Current structure (CONFIRMED)

- `threads.subscribe` is dispatched from the generic RPC match (`rpc.rs:346`), not from a dedicated
  Axum route. `subscribe` takes raw `Bytes`, parses its own body, and returns `Response`
  (`realtime.rs:48-56`). The handler decides *what* the stream is; a detached `tokio::spawn`
  decides *when* frames are written (`realtime.rs:7-8, 82-131`).
- Resume is a two-phase replay: `events.since(after_seq)` is read **before** the task subscribes to
  the broadcast (`realtime.rs:70-75` vs `83`). `last_seq` deduplicates overlaps but cannot repair a
  gap; events published between the read and the subscribe are lost for that connection.
- Fan-out is in-process: `BroadcastPublisher` holds `Mutex<HashMap<ThreadId, broadcast::Sender>>`
  in `crates/infra/src/events.rs:22-25`. Channels are created on first use and **never removed**
  (`events.rs:54-60`), so the map grows with every thread ever seen. `EventPublisher` is a clean
  port (`ports.rs:1093-1105`), so the api crate is insulated from the transport.
- Capacity mismatch: broadcast holds 256 (`events.rs:18`), the SSE mpsc holds 64
  (`realtime.rs:27`). A slow client fills the mpsc first; the broadcast receiver then reports
  `Lagged`, which is silently ignored (`realtime.rs:111-114`) while the comment promises a resume
  "on the next connection" — a long-lived connection therefore drops events silently.
- Heartbeat is **per subscriber and persisted**: every tick appends an `EventType::Heartbeat` to the
  log and publishes it (`realtime.rs:93-128, 150-171`). N idle listeners on one thread cause N writes
  per 15 s, plus N log frames every client replays. Wire frames are `EventView` JSON
  (`wire.rs:84-118`); a failed serialization silently becomes `{}` (`realtime.rs:143`).
- Response is bare `Sse::new(stream).into_response()` (`realtime.rs:133-134`): no `no-cache`,
  `Connection: keep-alive` or `X-Accel-Buffering: no`, so proxies may buffer and delay frames.
- No cancellation handle, subscriber cap or shutdown drain: the task ends only when `send` fails,
  the publisher closes, or the process exits. `after_seq` is `unwrap_or_default()` with no boundary
  validation (`realtime.rs:69`). Only `tracing::debug!` appears (lines 54, 125); no spans/metrics.
- Only one test exists and it only asserts the heartbeat interval (`realtime.rs:180-189`). The public
  `FrameStream` alias is `ReceiverStream<mpsc::Receiver<Event>>` — a stream *of receivers*, not the
  stream `subscribe` actually returns (`realtime.rs:175`); it is misleading dead API.

## Guiding principle for v0.4.0

Treat realtime as a small, typed **hub boundary**, not handler-local glue: one correct replay/live
merge, one liveness mechanism that does not write to the log, one transport port that can scale past
a single process, and a lifecycle owned by the app rather than by detached tasks.

## Upgrades (5 concrete items)

1. **Close the replay/live race and recover from `Lagged`.** Subscribe to the publisher *first*,
   then read history, and let `last_seq` drop the duplicates this deliberately creates. On
   `RecvError::Lagged(n)`, re-read `events.since(last_seq)` into the producer instead of ignoring it.
   This is the core REQ-005/REQ-020 fix: no lost events across reconnect or prolonged slowness.
   *Risk:* re-reading on every lag can duplicate work under sustained overload; dedup must be exact.
   *Mitigation:* cap re-sync frequency, keep `last_seq` as the single source of truth, and add a
   deterministic test with a paused clock that injects a `Lagged` and asserts a gap-free sequence.

2. **Separate liveness from log continuity.** Keep a cheap SSE comment ping (`: keep-alive\n\n`) per
   connection for transport liveness, and coalesce the *sequenced* `heartbeat` event to one
   per-thread scheduler shared by all subscribers, stopping when the last subscriber leaves. This
   removes per-subscriber write amplification (N writes → 1) and log bloat on idle threads.
   *Risk:* changing heartbeat cadence/producers is wire-visible and may be read as an externally
   visible change. *Mitigation:* keep the frame shape and `EventType::Heartbeat` unchanged, verify
   against the golden fixtures, and record any observed behavior change in `docs/PARITY.md` before
   shipping (AGENTS.md).

3. **Make the hub bounded and multi-replica-capable behind the existing port.** Move the hub out of
   ad-hoc handler code into a dedicated `RealtimeHub` implementing `EventPublisher`; evict idle
   channels by TTL/LRU so the map cannot grow without limit, and add an optional cross-replica
   fan-out (e.g. Redis pub/sub) so a subscriber on replica B sees events published on replica A.
   *Risk:* a new service/infra dependency conflicts with the "few moving parts" criterion and must
   pass cargo-deny and be recorded in `docs/DEPENDENCIES.md` (use `cargo add` + docs.rs, not memory).
   *Mitigation:* keep the `EventPublisher` port unchanged so api code is insulated, make cross-replica
   mode opt-in via typed config with an ADR, and add a two-instance delivery test (echoes the D-01
   obligation in `spec/05-known-defects.md`).

4. **Own the stream lifecycle and make the HTTP surface proxy-safe.** Give the stream route a typed
   extractor and SSE-specific response headers (`Cache-Control: no-cache`, `Connection: keep-alive`,
   `X-Accel-Buffering: no`); retain the task `JoinHandle`/cancellation token on `AppState`, drain
   streams on graceful shutdown, cap concurrent subscribers, and validate `after_seq >= 0` at the
   boundary.
   *Risk:* header changes are wire-visible (benign but must be checked) and subscriber caps could
   reject legitimate clients. *Mitigation:* keep caps generous and configurable, mirror the auth
   extractor introduced by worker #121 where it overlaps, and assert headers in the contract test.

5. **Make the merge logic testable and observable.** Extract the select-loop into an isolated,
   clock-driven producer so it can be unit-tested with `tokio::time::pause`; add REQ-named tests
   for gap-free resume, dedup, `Lagged` recovery, tenant isolation and heartbeat backpressure.
   Add spans/counters for active subscribers, frames sent, lag events and reconnects (never log
   payloads or tokens), and delete or correct the bogus `FrameStream` alias.
   *Risk:* test refactor churn plus per-frame metric overhead on a hot path. *Mitigation:* use
   `#[cfg(test)]` seams, benchmark the stream hot path, and keep the REQ ids in test names
   (`tools/trace_check.py` counts declarations only).

## Cross-cutting risks / sequencing

- Order 1 → 2 → 3 → 4/5: correctness of the merge first, then write-cost reduction, then scaling,
  then lifecycle/observability. Items 4 and 5 are largely independent once 1 is in place.
- Realtime inherits the auth boundary (worker #121): a long-lived stream must not outlive a revoked
  session; re-check authorization on heartbeat or re-resolve the principal on reconnect.
- Keep `EventView` field names and the `threads.subscribe` procedure name for compatibility
  (`spec/03-api-contract.md`); any change is a DEVIATION in `docs/PARITY.md`.
- Verification gate for each step: `cargo fmt --check`, `clippy -D warnings`, unit + `tests/`
  integration, and golden/contract suites.

## Out of scope for this module

Notification push subscriptions (`rpc.rs:401`), the producer/publish call sites in the app layer,
and proxy/nginx deployment configuration itself; this proposal only restructures the realtime hub,
its stream lifecycle and its tests.
