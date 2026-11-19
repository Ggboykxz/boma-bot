# Worker 0343 — `crates/infra/src/stores/events.rs` architecture proposal (v0.4.0)

- Worker: 343 · Lens: architecture · Module: `crates/infra/src/stores/events.rs` (103 lines) · Target: v0.4.0.
- Scope: read-only review of this module plus targeted reads of `stores/rows.rs`, `infra/tenant.rs`,
  `app/ports.rs:212-227,1035-1060,1092-1105`, `app/service.rs:23-60`, `app/worker.rs:406-424`,
  `api/realtime.rs:48-172`, `testkit/memory.rs:2750-2810`, `migrations/0002_m1.sql:215-224,296`,
  and sibling briefs (wave-9 #147 realtime, wave-13 #289 publisher). No product code changed; no tests run.
- "Current" claims are **CONFIRMED by source inspection** in this worktree; forward-looking items are
  proposals (**GUESS** until implemented and verified). English per repo disk convention.

## Current structure of the events store (CONFIRMED)

- `PostgresEventStore` holds a `PgPool` and a Rust-side `crypto::IdGenerator`; it implements the async
  `EventStore` port (`append`, `since`, `app/ports.rs:1037-1060`). `MemoryEventStore` is the test twin.
- `append` (l.37-74): builds `evt`-prefixed id in Rust, opens one tenant transaction via
  `tenant::with_tenant` (transaction-scoped `boma.space_id`/`system_reason`), allocates `seq` with
  `UPDATE threads SET next_event_seq = next_event_seq + 1 ... RETURNING next_event_seq - 1`, inserts
  one `events` row (`Json` payload, caller-supplied `created_at` cast to `timestamptz`), returns via
  `rows::event`. `UNIQUE (thread_id, seq)` plus `events_thread_seq_idx (thread_id, seq)` back it.
- `since` (l.76-102): `SELECT ... WHERE thread_id = $1 AND seq > $2 ORDER BY seq ASC`, `fetch_all`
  into an unbounded `Vec<Event>`. The port has no `limit`/cursor parameter.
- The store **does not publish**. `MemoryEventStore::append` publishes inside its lock; in production
  the only `publisher.publish` call is the SSE heartbeat (`realtime.rs:170`). Worker-emitted
  `MessageUpdate` events (`worker.rs:406-424`) are persisted but never broadcast. This split is the
  root architectural seam; the publisher-side brief (#289) covers the bus, this one covers the store.
- Sequence allocation shares the `threads` row with `next_message_seq`: event and message writes,
  including one `MessageUpdate` per streamed chunk, contend on the same hot row.
- A missing/foreign thread makes the `UPDATE` affect 0 rows; `fetch_one` yields `RowNotFound`, mapped
  by `tenant::unavailable` to `AppError::Unavailable` — the memory adapter returns `NotFound`. The
  store trusts `NewEvent.space_id` for the insert instead of deriving it from the locked thread.

## Guiding principle for v0.4.0

Make the store the single durable commit boundary and the sole owner of write invariants: sequence,
tenant, time and the hand-off to delivery. Keep adapters contract-equivalent so app code cannot tell
Postgres from memory, and preserve REQ-005/REQ-020 monotonic per-thread sequence and gap-free replay.

## Concrete upgrades (5), with risks

1. **Own the commit-then-publish hand-off (durable outbox).** Inject `Arc<dyn EventPublisher>` into the
   store and publish the committed `Event` at the boundary, mirroring the memory adapter; remove the
   stray heartbeat publish so there is exactly one site. For the multi-process gap, write an outbox
   row in the *same* tenant transaction and let a bridge drain it in order. *Risks:* a publish after
   commit leaves a crash window (replay covers it, but live clients see silence); publishing inside
   the transaction exposes uncommitted rows; outbox adds a table/config surface and needs an ADR,
   `docs/DEPENDENCIES.md` entries and a reconciliation with the health strings. No wire change.

2. **Decouple sequence allocation from the hot `threads` row.** Move `next_event_seq` off the shared
   message/event counter row (dedicated per-thread counter table, or batched allocation that reserves
   a range in one UPDATE) so streaming deltas no longer serialize against message appends.
   *Risks:* changes to allocation change `seq` value semantics — parity-sensitive under REQ-005/020;
   a migration must backfill counters atomically and a concurrent-append test must prove no duplicate
   or regressive seq; batching wastes seq on crashes (gaps are fine for dedupe, not for "gap-free"
   copy in docs). Coordinate with `threads`/`messages` stores and record any deviation.

3. **Bound `since` with a cursor/limit.** Add an additive `since_page(context, thread_id, after_seq,
   limit) -> (Vec<Event>, next_cursor)` (or extend `since` with a limit) so reconnect replay cannot
   load an unbounded log into memory; have `realtime::subscribe` page until caught up. *Risks:* a
   signature change ripples to memory adapter, realtime and tests; paging changes replay latency and
   must keep ordering and dedupe exact; verify against golden/reconnect tests and the OpenAPI contract
   before touching the public `threads.subscribe` shape. Spec NFRs already call for bounded history.

4. **Align error and tenant semantics across adapters.** Map a 0-row thread `UPDATE` to
   `AppError::NotFound`, not `Unavailable`, so callers distinguish "thread gone" from "DB down";
   return `space_id` from that same `RETURNING` and validate it against `NewEvent.space_id` and the
   tenant context before inserting. *Risks:* error-variant change is externally visible via the API
   error mapping — check `api/error.rs` and record a DEVIATION if observed; system contexts cross
   tenants by design, so validation must exempt them; an extra `RETURNING` column touches `EventRow`.

5. **Make time and idempotency store-owned invariants.** Stop trusting caller `created_at` cast from a
   string: derive it from the DB (`now()`) or validate/normalize it, so seq order and event time cannot
   disagree; accept an optional client idempotency key (unique per thread) so an ambiguous retry does
   not append a second logical event. *Risks:* DB time changes stored values and the `EventView`
   format (parity/DEVIATION + golden fixtures); a new key needs a migration and port change and must
   preserve the append-only, dedupe-by-seq contract consumers rely on. Do this last, behind a flag.

## Sequencing and cross-cutting risks

- Order: 1 → 3 → 4 → 2 → 5. Item 1 closes the CONFIRMED production delivery gap at lowest risk;
  3 and 4 are independent hardening; 2 is the throughput payoff with the largest parity surface; 5
  is broadest and last.
- Keep delivery at-least-once with client-side seq dedupe and replay — do not remove `last_seq`/`since`
  replay while adding an outbox or bridge (#289, #147).
- Preserve the port as the seam: any `EventStore` change must be mirrored in `MemoryEventStore` and
  the testkit fixtures so adapter equivalence holds and REQ-named tests stay meaningful.
- Verification gate per step: `cargo fmt --check`, `clippy -D warnings`, unit + `tests/` integration,
  golden and OpenAPI contract suites; any externally visible change goes to `docs/PARITY.md`.

## Evidence

CONFIRMED — read this session: `stores/events.rs:1-103`; `stores/rows.rs:140-150,381-390`;
`infra/tenant.rs:41-164`; `app/ports.rs:212-227,1035-1060,1092-1105`; `app/service.rs:23-60`;
`app/worker.rs:406-424`; `api/realtime.rs:48-172`; `testkit/memory.rs:2750-2810`;
`migrations/0002_m1.sql:215-224,296`; grep of `publisher.publish`/`events.append` across `crates/`.
GUESS — crash-window tolerance of replay, exact seq-value sensitivity of golden fixtures, and whether
`Unavailable` on a missing thread is relied upon by callers (verify before items 4 and 5).
