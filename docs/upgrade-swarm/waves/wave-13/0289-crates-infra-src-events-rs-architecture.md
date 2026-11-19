# Worker 0289 — `crates/infra/src/events.rs` architecture proposal (v0.4.0)

- Worker: 289 · Lens: architecture · Module: `crates/infra/src/events.rs` (80 lines) · Target: v0.4.0.
- Scope: read-only review of this module plus targeted reads of `stores/events.rs`, `domain/src/event.rs`,
  `app/src/ports.rs:212-227,1035-1105`, `api/src/realtime.rs:48-131,150-172`, `testkit/src/memory.rs:2751-2790`,
  `infra/src/lib.rs:49`, `app/src/health.rs:118-143`, `spec/01-requirements.md` (REQ-005/020),
  `spec/04-nonfunctional.md:7-36` and sibling wave briefs. No product code changed; no tests run.
- "Current" claims below are **CONFIRMED by source inspection** in this worktree; forward-looking
  items are proposals (**GUESS** until implemented and verified). English per repo disk convention.

## Current event bus / store structure (CONFIRMED)

- Two distinct adapters sit behind two ports in `app/ports.rs`: `EventPublisher` (sync,
  `publish`/`subscribe -> broadcast::Receiver<Event>`, :1093-1105) implemented only by
  `BroadcastPublisher` in this file, and `EventStore` (async `append`/`since`, :1037-1060) implemented
  by `PostgresEventStore` (`stores/events.rs`) and `MemoryEventStore` (testkit).
- `BroadcastPublisher` holds `Mutex<HashMap<ThreadId, broadcast::Sender<Event>>>`, capacity 256,
  one channel per thread. `channel()` lazily creates a sender and clones it; poisoned locks are
  recovered with `into_inner()`; `publish` is fire-and-forget (`let _ = sender.send(event)`).
  Channels are never removed — the doc says they live until process end.
- `PostgresEventStore::append` allocates `seq` via `UPDATE threads SET next_event_seq = ... RETURNING`
  inside the tenant transaction, then inserts the row; `since` reads `seq > after_seq` ordered.
  **It does not publish.** Only `MemoryEventStore::append` publishes (on append, under its lock, so
  subscriber order equals seq order). In production, the only `publisher.publish` call is the
  heartbeat in `api/realtime.rs:170` — message/run/card/typing events are stored but never broadcast.
- `realtime::subscribe` replays `since(after_seq)`, subscribes to the broadcast, dedupes by
  `event.seq <= last_seq`, ignores `RecvError::Lagged`, and heartbeats every 15 s into its own
  `mpsc` channel of capacity 64. It spawns an untracked `tokio::spawn`.
- `domain::Event` carries `thread_id`, `seq`, `kind`, `payload`, `created_at` — no `space_id`,
  although `NewEvent` does. The bus can only key by `ThreadId`.
- `health::Capabilities::realtime` hard-codes `"postgres"`; no Postgres LISTEN/NOTIFY, outbox table
  or pub/sub bridge exists anywhere in `crates/` or `migrations/` (GUESS: aspirational string).
- Spec demands self-hosted **multiple processes** (api + worker), with realtime Redis optional and
  in-process fallback (`04-nonfunctional.md:9-12`): an in-process broadcast cannot deliver a
  worker-emitted event to a subscriber on another API replica. This is the central architecture gap.
- No `#[cfg(test)] mod tests` in this file: capacity, fan-out, ordering and poison recovery are untested.

## Guiding principle for v0.4.0

Keep `BroadcastPublisher` as the cheap **local** fan-out sink, but make the **store** the single
commit-then-publish boundary and add a small cross-process bridge behind the same port. Delivery
must stay at-least-once with client-side seq dedupe and replay, never exactly-once in the transport.

## Concrete upgrades (5), with risks

1. **Unify append + publish at the store boundary.** Inject `Arc<dyn EventPublisher>` into
   `PostgresEventStore` and publish the returned `Event` immediately after the tenant transaction
   commits, mirroring `MemoryEventStore`; remove the stray publish from `realtime::heartbeat` so
   there is exactly one publish site. *Risks:* a crash between commit and publish loses the live
   frame (replay covers it, but clients see silence until reconnect) — mitigate with a durable
   outbox in the same transaction if the gap is unacceptable; publish must happen after commit, not
   inside it, or readers can see uncommitted events. No wire change.

2. **Add a Postgres-backed cross-replica bridge.** A `PgNotifyPublisher` using sqlx `LISTEN/NOTIFY`
   on an `events` channel (or an outbox table + polling task) so any api/worker process emits frames
   to every replica's local `BroadcastPublisher`. Keep `BroadcastPublisher` as the local sink, build
   one bridge task in `bin/commands.rs`. *Risks:* NOTIFY payloads are lossy and may arrive out of
   order or twice — the existing seq dedupe and `since` replay already tolerate this, but a test
   must prove it; new dependency/config surface needs `docs/DEPENDENCIES.md` and a validated env var;
   reconcile the `"postgres"`/`"memory"`/`"disabled"` health strings with the actual wiring.

3. **Bound channel lifetime and unify capacities.** Reap channels whose `receiver_count() == 0` after
   an idle TTL (timestamp on the map entry, periodic sweep in the bridge task); align the broadcast
   capacity (256) with the SSE hop (64) so lag is caught at the smallest buffer. *Risks:* subscribe
   racing eviction could hand out a receiver on a channel about to be dropped — create/remove under
   the same lock and re-check `receiver_count()`; a re-subscribe after eviction must replay from the
   store (it does), so this is not a client-visible gap. Prevents unbounded HashMap/ring-buffer growth
   on long-lived hosts (every thread ever published to currently leaves ~256 slots resident).

4. **Make backpressure and health observable.** Increment `services.metrics` counters for publishes,
   active subscribers, dropped sends and `Lagged`; emit a `tracing` warning on lag (not per event).
   Consider ending the SSE stream on `Lagged` so the client performs a clean replay instead of
   silently skipping frames. *Risks:* ending the stream is client-visible — record as a DEVIATION in
   `docs/PARITY.md` and verify the golden/reconnect tests; per-publish logs would be hot, so sample;
   metrics wiring must not introduce a lock held across `.await`.

5. **Give events tenant context and real tests.** Thread an `Option<SpaceId>`/`TenantContext` (or a
   `space_id` field) through `Event` so channels can be keyed by `(space, thread)` and authorization
   is enforceable at the bus, not only in `realtime::subscribe`; add unit tests in this file for
   multi-subscriber fan-out, seq ordering, capacity/lag, poison recovery and eviction, with
   `req_005_`/`req_020_` in the names. *Risks:* adding a serialized field changes the wire DTO
   (`EventView`) and the golden fixtures — make it additive with a serde default, check
   `spec/openapi.yaml` and the contract test first; a broader field change fans out across
   app/infra/api and needs a coordinated commit.

## Sequencing and cross-cutting risks

- Order: 1 → 2 → 3/4 → 5. Step 1 fixes the CONFIRMED production delivery gap at lowest risk; step 2
  is the multi-process payoff; 3 and 4 are independent hardening; 5 is the broadest and last.
- Keep publish idempotent and unordered-safe at the client: dedupe by `seq` is the contract that makes
  at-least-once transports acceptable — do not remove it while adding a bridge.
- Preserve REQ-005/REQ-020 guarantees: monotonic per-thread seq, gap-free replay, 15 s heartbeat,
  bounded history paging (`04-nonfunctional.md:32-34,57-59`).
- Verification gate per step: `cargo fmt --check`, `clippy -D warnings`, unit + `tests/` integration,
  golden and OpenAPI contract suites; any externally visible change goes to `docs/PARITY.md`.

## Evidence

CONFIRMED — read this session: `events.rs:1-80`; `stores/events.rs:1-103`; `domain/event.rs:1-141`;
`ports.rs:212-227,1035-1105`; `realtime.rs:48-131,150-172`; `testkit/memory.rs:2751-2790`;
`infra/lib.rs:49`; `health.rs:118-143`; grep of `publisher.publish` (only `realtime.rs:170` and
`memory.rs:2784`); `spec/01-requirements.md`, `spec/04-nonfunctional.md:7-59`.
GUESS — that `realtime: "postgres"` is aspirational (no bridge found); exact NOTIFY frame ordering
and golden sensitivity to an added `space_id` field (verify before steps 2 and 5).
