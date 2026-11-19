# Worker #290 — `crates/infra/src/events.rs` — Reliability (v0.4.0)

Scope: reliability hardening of `BroadcastPublisher` (REQ-005, REQ-020). Evidence is static
inspection of this module and its direct callers (`api/src/realtime.rs`, `stores/events.rs`,
`app/src/ports.rs`, `testkit/src/memory.rs`); no runtime test was run by this worker. CONFIRMED
means read in code this session; GUESS is inference. Any externally visible change needs a
DEVIATION entry in `docs/PARITY.md`.

## Failure mode 1 — Event loss on the live path

Evidence (CONFIRMED): `publish()` discards the `send()` result (l.70-75). `broadcast::Sender::send`
returns `Err(SendError(event))` when the thread has zero receivers; the event then reaches nobody
live. The code comment says the event "stays in the log" and a reconnect replays it — true only if
something wrote it to the log first. `publish()` takes an already-appended `Event`, so that link is
the caller's job. In production the only `publisher.publish()` call site in the tree is the heartbeat
(`api/src/realtime.rs:170`); `PostgresEventStore::append` does not publish (`stores/events.rs:37-74`).
By contrast the in-memory test store publishes inside `append()`
(`testkit/src/memory.rs:2782-2784`). Net effect: production SSE subscribers get heartbeats but never
live `message`/`run`/`card`/`typing`/`roster` events; those exist only in the log and need a
reconnect/poll. Memory-store tests cannot catch this (GUESS: wiring defect, not intended behavior).

Hardening:
- Make "append ⇒ publish" one invariant: either have `PostgresEventStore::append` publish (as the
  memory store does, preserving seq order) or add an app-layer `emit()` that appends then publishes,
  and route every `NewEvent` producer through it. Keep publish after the commit.
- Surface the dropped-send signal: return a count/`Result` from `publish()` or expose a metric so
  "no listeners" and `SendError` are observable instead of silently ignored. Changing the
  `EventPublisher` port signature is externally visible → DEVIATION; prefer an additive metric.

Risks: publishing from the store couples infra to the publisher and reopens lock ordering (the
memory store publishes under its append mutex to keep seq order; Postgres cannot hold a DB
transaction across a broadcast). With publish-after-commit, a crash between the two still loses the
live notification until reconnect — acceptable only if clients always reconnect with `after_seq`.

## Failure mode 2 — Replay gaps

Evidence (CONFIRMED): the subscribe flow in `api/src/realtime.rs` reads history *before* subscribing
to the live channel — `events.since()` at l.70-75 runs in the handler, `publisher.subscribe()` at
l.82-83 runs later inside the spawned task. Any event appended between the two is in neither the
`history` vector nor the live receiver: a permanent gap on that connection. The `seq <= last_seq`
guard (l.101-103) removes duplicates but cannot detect holes. Second gap source: append and publish
are separate steps with no outbox; a crash/restart between them (or a bare restart — the channel
map is in-memory and rebuilt empty) drops the live event. Third: the channel map (l.23, l.54-60) has
no eviction — every thread ever seen keeps a Sender and its ring buffer until process exit ("kept
until the process ends", module doc l.4-5), an unbounded slow leak.

Hardening:
- Invert the consumer order: subscribe to the broadcast *first* (buffers from that instant), then
  read `since()`, set `last_seq` to the max history seq, and drain buffered live events filtering
  `seq <= last_seq`. Closes the race without touching the port.
- Add `shutdown()` / `remove(thread_id)` to `BroadcastPublisher` that drops a thread's Sender so
  receivers get `RecvError::Closed`; wire into graceful shutdown and thread deletion. Today
  `Closed` (realtime.rs:110) is effectively unreachable because the map never drops a Sender.
- Reclaim channels for threads with no receivers and no recent activity (idle TTL), with a metric
  for map size.

Risks: "subscribe first, then read log" replays buffered live events; the `<= last_seq` filter must
be correct or clients see duplicates, so add a regression test. Eviction/TTL adds state and a
background task; too short a TTL can drop buffered events on a fast reconnect (recoverable from the
log). `shutdown()` changes observable stream termination → DEVIATION.

## Failure mode 3 — Ordering

Evidence (CONFIRMED): ordering is only as good as the caller. `publish()` broadcasts in call order,
not `seq` order; it never checks `event.seq`. The consumer drops any event with `seq <= last_seq`
(realtime.rs:101-103), so if two events of a thread are published out of seq order (concurrent tasks
append then publish), the later lower-seq event is silently discarded — a hole recoverable only by
reconnect. The memory store avoids this by publishing under its append mutex (memory.rs:2782-2784);
the Postgres path has no equivalent. Also one `std::sync::Mutex` (l.23, l.44-51) serializes
publish/subscribe across *all* threads; the `broadcast::channel()` ring allocation happens in the
critical section (l.58), so a slow allocation stalls every publisher (brief, no `.await` held — not
a runtime-block bug, but a throughput bottleneck). Poison recovery (l.47-50) is silent.

Hardening:
- Guarantee per-thread seq-ordered publish at the source: a single emit point after commit,
  serialized per thread. Keep `Event.seq` as the ordering key, never arrival order.
- Make the consumer resequence by `seq` (small reorder buffer, or accept only `seq == last_seq+1`
  and request replay for gaps) instead of a bare `<= last_seq` drop.
- Shrink the critical section if profiling shows contention; keep poison recovery but log when a
  poisoned lock is observed, since silent recovery can mask a panic.

Risks: a reorder buffer adds latency/memory and needs a timeout; strict `== last_seq+1` turns
duplicates into gaps unless replay is solid. Sharding the map adds a dependency and complexity —
justify only with measured contention (GUESS: unlikely at current scale).

## Failure mode 4 — Backpressure / capacity

Evidence (CONFIRMED): `CHANNEL_CAPACITY = 256` (l.17-18). `broadcast` is lossy: a receiver falling
>256 events behind gets `RecvError::Lagged(n)`; the consumer swallows it with an empty arm
(realtime.rs:113) and carries on, so a slow client silently misses a burst and is never told to
resync. `with_capacity()` (l.36-41) is public and unvalidated — `broadcast::channel(0)` panics in
tokio, and a huge value overallocates every ring. The SSE producer adds a second bounded hop
(`mpsc` 64, realtime.rs:80) whose `send().await` backpressures the task and stalls that subscriber.

Hardening:
- Validate/clamp capacity in `with_capacity()` (at least `max(1)`; document a sane upper bound) so
  config cannot panic the process.
- Replace the swallowed `Lagged(_)` with a real resync: re-read `events.since(&thread, last_seq)` and
  re-inject missed frames, plus a `tracing::warn!`/metric with the lag count. Size 256 from a
  measured worst-case burst and expose it in typed config (fail fast at startup).
- Emit counters for publishes, no-listener sends, lagged events, and active channels — this module
  currently has no observability.

Risks: auto-resync on lag can flood a slow client and worsen backpressure — cap the replay window
and prefer client-driven reconnect with `after_seq`; capacity changes are tuning, not correctness.
Additive metrics are safe; changing `with_capacity` semantics is a deviation only if a caller passes
0 today (GUESS: none do).

## Priority for v0.4.0
1. Close the read/subscribe race and the append⇒publish gap (smallest, highest impact — both are
   live-event loss). Add REQ-005/REQ-020 tests that publish during `since()`.
2. Add capacity validation, `Lagged` resync + observability, and `shutdown()`/`remove()`.
3. After profiling: ordered-emit helper, reorder buffer, channel eviction.

Tests to add (tag names): `req_005_publish_with_no_listener_is_counted`,
`req_005_with_capacity_rejects_zero`, `req_020_lagged_subscriber_resyncs_from_log`,
`req_020_subscribe_during_append_has_no_gap`, `req_020_publish_preserves_seq_order`,
`req_020_shutdown_closes_receivers`. Cross-module fixes (realtime.rs, stores/events.rs) need their
own REQ-tagged tests and a `docs/PARITY.md` DEVIATION if live-stream behavior changes.
