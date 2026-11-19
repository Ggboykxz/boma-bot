# v0.4.0 reliability proposal — `crates/infra/src/stores/events.rs`

Lens: reliability. Worker #344. Scope: the Postgres `EventStore` adapter and the append/read paths it
feeds. All facts below are CONFIRMED by reading `events.rs`, `rows.rs` (`event`), `tenant.rs`,
`migrations/0002_m1.sql` (tables `events`, `threads.next_event_seq`), `crates/api/src/realtime.rs`,
`crates/infra/src/events.rs` (`BroadcastPublisher`), `crates/app/src/ports.rs` (`NewEvent`,
`EventStore`, `EventPublisher`), `crates/app/src/usecases.rs`, `crates/bin/src/commands.rs:183`, and
`crates/testkit/src/memory.rs:2752-2786` in this worktree. No code was changed; read-only analysis.
Runtime claims not verified by an end-to-end run this session are marked GUESS.

## Failure modes

### 1. Live events are never published in production (CONFIRMED by code trace)
- `PostgresEventStore::append` (lines 37-74) only writes the row; it never calls `publisher.publish`.
  The production wiring (`crates/bin/src/commands.rs:183`) installs the bare `PostgresEventStore`,
  with no publishing decorator. Across the whole crate tree the only publish call sites are the
  heartbeat (`realtime.rs:170`) and the in-memory test store (`memory.rs:2784`).
- Consequence (GUESS without an e2e run): events appended by usecases/worker (e.g.
  `usecases.rs:5131` routine `Message`, card/worker events) are durable but invisible to live SSE
  subscribers until they reconnect and replay via `since`. The memory store publishes inside its
  append under one lock (`memory.rs:2782-2784`), so this is also a Postgres/memory parity divergence.

### 2. Lost events / ordering holes at the consumer boundary (CONFIRMED)
- `realtime.rs:70-83` takes the `since()` snapshot first, then registers `publisher.subscribe()`
  inside the spawned task. Any event appended in that window is in neither the history nor the live
  channel — a silent gap. The store offers no atomic snapshot+tail primitive, so consumers cannot
  close this window on their own.
- Publisher broadcast capacity is 256 (`infra/src/events.rs:18`) and the SSE mpsc is 64
  (`realtime.rs:27`). A lagging receiver is silently ignored (`realtime.rs:113`); those events are
  gone from the live stream and are only recoverable if the client reconnects. No gap signal exists.
- Publish happens after commit; two concurrent appends can allocate seq 5 then 6 but commit/publish
  in reverse. The consumer's `if event.seq <= last_seq { continue }` (`realtime.rs:101`) then drops
  the out-of-order lower seq. A decorator that publishes after commit makes this reachable.

### 3. Duplicate events on retry (CONFIRMED)
- `append` mints a fresh id per call (`new_id("evt")`, line 38) and has no dedup key, unlike
  `messages.insert` with `client_nonce`. An ambiguous outcome (client timeout after commit) retried
  produces a second row with a new seq — duplicate delivery. `UNIQUE(thread_id, seq)` only prevents
  seq collisions, not logical duplicates.

### 4. Replay poisoning by a single bad row (CONFIRMED)
- `since` (lines 76-102) does `fetch_all` then `.map(rows::event).collect()`. `rows::event` parses
  `type` via `EventType::parse(...)?` (`rows.rs:386`); one unknown/stale type string in the table
  fails the entire collect, so the whole stream replay is unavailable. No skip-and-report path.

### 5. Unbounded read and write amplification / backpressure (CONFIRMED)
- `since` has no `LIMIT` and uses `fetch_all` — a long thread replays every row into memory on every
  reconnect, with latency and OOM risk.
- Heartbeats are durable, sequenced events appended per subscriber every 15 s (`realtime.rs:24,150`).
  N subscribers on one thread cause N writes/15 s and inflate `next_event_seq` and the events table
  (~240 rows/hour/thread/subscriber), which in turn worsens mode 4. All appends also serialize on
  the `UPDATE threads SET next_event_seq = ...` row lock (`events.rs:44-51`), a per-thread ceiling.

### 6. Error classification and tenant predicate gaps (CONFIRMED)
- Every driver error is collapsed to `AppError::Unavailable` via `tenant::unavailable`, including
  unique violations and serialization failures (40001); `tenant::is_unique_violation` is unused here.
  No retry taxonomy, no statement timeout, so transient faults look like outages and cannot be
  safely retried.
- `since` filters only `thread_id`/`seq` — it relies solely on RLS, unlike `messages.count_above`
  which also adds `space_id = $3` as defense in fact. `append` binds `event.space_id` without checking
  it against a member context's space (superuser/backup paths bypass RLS).

## Concrete hardening (v0.4.0)

1. **Publish every append.** Wrap the store in a `PublishingEventStore` decorator (delegate, then
   `publisher.publish`) at the binary seam so all appends reach live subscribers as the memory store
   does; remove the heartbeat's manual publish to avoid doubles. Risk: post-commit publish is not
   atomic (crash loses the live frame, replay recovers it); commit/publish reordering can drop a live
   seq — pair with item 2 and a consumer gap check.
2. **Close the snapshot gap.** In `realtime.rs`, subscribe *before* `since()` and dedupe by seq; add
   a consumer gap detector: if a live seq > `last_seq + 1`, re-query `since(last_seq)` and backfill.
   Risk: low; needs bounded `since` (item 3) to keep backfill cheap. Store-side alternative: expose
   a combined `subscribe_from(...)`/long-poll tail primitive (larger change, DEVIATION).
3. **Bounded, paginated replay.** Add `since_page(..., after_seq, limit) -> (Vec<Event>, Option<i64>)`
   with `LIMIT`, keep legacy `since` as a wrapper on a generous default. Risk: internal port change;
   callers assuming unbounded reads must be updated; existing `events_thread_seq_idx` covers it.
4. **Idempotent append.** Add an optional `dedup_key` to `NewEvent` and a partial
   `UNIQUE(thread_id, dedup_key)` migration; on conflict return the existing event (as
   `messages.insert` does). Plumb a stable key from each producer. Risk: port + migration change;
   needs a retry-stable key; suppressing duplicates is a behavior change (DEVIATION entry).
5. **Resilient replay mapping.** On `EventType::parse` failure, skip the row with a WARN + metric and
   an explicit list of skipped seqs rather than failing the whole collect; consider an
   `EventType::Unknown(String)` fallback. Risk: hiding corruption (keep the metric/alert); a new
   enum variant may touch the wire format → DEVIATION, prefer skip-and-report first.
6. **Amortize heartbeats.** Coalesce to one durable heartbeat per thread per interval regardless of
   subscriber count, and add heartbeat retention/compaction. Risk: coalescing is behavior-preserving;
   making heartbeats non-durable breaks the "no seq gap" assumption → ADR + DEVIATION.
7. **Errors and scope.** Classify unique violations and 40001 as retriable, add `SET LOCAL
   statement_timeout` in the tenant scope, and add `space_id = context.space_id()` to `since` plus a
   member-context check in `append`. Risk: keep the HTTP error mapping stable to avoid a DEVIATION;
   never log payloads (may carry PII).

## Cross-cutting risks
- Items 1, 2, 4, 5, 6 alter observable behavior or schema → DEVIATION entries in `docs/PARITY.md`
  and owner approval before shipping. Sequence: migration → port → store/decorator → consumer →
  tests, one commit each.
- Keep all writes inside `with_tenant`'s transaction with transaction-local RLS settings; do not
  open raw connections. Append cancellation safety: sqlx rolls back `Transaction` on drop, but add
  an abort-mid-append test to prove no half row or burned seq survives.

## Test plan (REQ-tagged names, real Postgres)
- `req_005_concurrent_appends_get_unique_contiguous_seq`,
  `req_005_append_publishes_to_live_subscriber_in_seq_order`,
  `req_005_snapshot_gap_is_backfilled`, `req_005_unknown_type_row_does_not_poison_replay`.
- `req_020_retried_append_with_dedup_key_returns_same_seq`,
  `req_020_since_respects_limit_and_cursor`, `req_020_since_is_space_scoped`,
  `req_020_lagging_subscriber_recovers_from_log`, `req_020_aborted_append_leaves_no_row_or_gap`.

## Open questions (GUESS)
- Whether out-of-scope events are intentionally live-only by design (no publish on the Postgres
  path) or an oversight — assumed oversight; confirm against spec/05-known-defects.md and the
  original before item 1.
- Acceptable heartbeat retention window and whether a durable heartbeat is required by REQ-005
  parity — assumed durable with compaction pending product decision (mark GUESS in `docs/STATE.md`).
