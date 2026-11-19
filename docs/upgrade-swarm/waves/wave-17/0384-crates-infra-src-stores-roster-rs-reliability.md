# Wave 17 — worker 0384 — `crates/infra/src/stores/roster.rs` (LENS: reliability)

Target: v0.4.0. Scope: the Postgres `RosterStore` adapter, with the port, schema, mapping and tests cited as
evidence. All code facts below are CONFIRMED by direct reads in this session (`roster.rs`,
`stores/rows.rs:277-300,444-462`, `app/src/ports.rs:1224-1265`, `migrations/0002_m1.sql:141-166,291`,
`0010_read_states.sql`, `0011_snoozes.sql`, `crates/infra/tests/m1.rs:1015-1088`, `tenancy.rs:160-214`).
Concurrency/load impact is GUESS: no fault-injection or load test was run.

## Current shape (CONFIRMED)

- Two methods, both in `tenant::with_tenant`: `user_id`/`space_id` are required (`.ok_or(NotFound)`), so a
  system context has no rail. Each call is its own transaction; `tenant::unavailable` maps **every** SQL
  error to `AppError::Unavailable`.
- `list_bots` is one statement: `bots` **INNER JOIN** `threads` on `t.bot_id=b.id AND t.space_id=$2`,
  LEFT JOIN future snoozes (`s.snoozed_until > now()`), LEFT JOIN read states. Unread is a correlated
  subquery counting `m.role <> 'tool' AND m.seq > COALESCE(p.last_seq,0)`. Stable `ORDER BY created_at, id`.
- Snooze expiry is time-based in SQL (`0011`: no cleanup job) — the intended "filter, never a hiding place".
- `set_snooze(None)` is a blind `DELETE ... WHERE thread_id=$1 AND user_id=$2`, always `Ok(None)`.
  `set_snooze(Some)` is `INSERT..SELECT..WHERE EXISTS(ownership) ON CONFLICT UPDATE RETURNING`;
  `fetch_optional -> ok_or(NotFound)`. `until` is bound as text cast to `timestamptz` in SQL.
- Schema: `threads.bot_id` has only a **non-unique** index (`0002:291`); `bots.thread_id` is plain text,
  not an FK (`0002:148-150`); snoozes and read states have `PK (thread_id,user_id)` and RLS `FORCE`.
- Mapping: `rows::bot_summary` uses `collect::<Result<Vec<_>,_>>()` and
  `ThreadId::new(row.thread_id.unwrap_or_default())?` — one bad row fails the whole list.
- Tests cover the happy snooze round-trip (`m1.rs`), cross-tenant refusal (`tenancy.rs`) and the RPC
  surface (`rpc_surface.rs:1002-1091`). No duplicate-thread, missing-thread, malformed-row or concurrency test.

## Failure modes

1. **Duplicate members (CONFIRMED schema/query, GUESS frequency).** No `UNIQUE` on `threads(bot_id)`:
   a second thread for one bot (retried creation, helper/group thread sharing `bot_id`, manual repair)
   makes the INNER JOIN emit one `BotSummary` per thread. The rail shows the same bot twice with different
   `thread_id` and unread counts; `ORDER BY` does not dedupe. How often this happens depends on write
   paths not audited here.
2. **Missing bots (CONFIRMED).** The INNER JOIN silently drops any bot with no thread row in the caller's
   space (dangling `bots.thread_id`, thread in another space, NULL `thread_id`). Worse: a NULL/empty
   `thread_id` becomes `ThreadId::new("")`, the mapping error propagates through `collect()`, and the
   **entire** `list_bots` returns `Unavailable` — one corrupt row blanks every bot. No WARN or metric
   records dropped rows, so the loss is invisible.
3. **Stale roster (CONFIRMED behavior, GUESS prevalence).** `BotSummary` carries no version/etag/`updated_at`,
   so a consumer cannot tell that a cached list is behind a `markRead`, a new message or a snooze change.
   Snooze reappearance is deadline-driven, so a held cache stays wrong with no invalidation signal.
   Deadlocks (40P01), serialization failures (40001) and pool timeouts all collapse to `Unavailable`
   with no retry or retry hint — a transient blip looks like a permanent outage. The per-bot correlated
   unread subquery can become the slow path under load, feeding timeouts (GUESS).
4. **Clear-snooze contract gap (CONFIRMED).** The `None` path has no ownership `EXISTS` guard and returns
   `Ok(None)` for a foreign or nonexistent thread; RLS protects the data, but the method reports success
   where the `Some` path would return `NotFound`.
5. **Error-class mismatch (CONFIRMED).** A malformed `until` reaching the store fails the SQL cast and
   surfaces as `Unavailable`, not the port's documented `Rule` (`ports.rs:1257-1258`); usecase validation
   masks this today.

## Concrete hardening (with risks)

- **H1 — Make duplicates unrepresentable.** Add a partial `UNIQUE INDEX ON threads(bot_id) WHERE kind='bot'`
  (full `UNIQUE(bot_id)` only if helper/group threads never reference the same bot). *Risk:* migration fails
  if duplicates already exist — audit/dedupe first; confirm REQ-010 group threads do not share `bot_id`.
  Schema-only, not an external DEVIATION.
- **H2 — Stop silent drops.** Switch to `LEFT JOIN threads` and either skip missing-thread rows with a
  structured WARN + counter, or surface a sentinel. `BotSummary.thread_id` is non-optional, so full
  surfacing is an API change. *Risk:* lower-risk v0.4.0 variant is to keep the join and add a diagnostic
  count (bots vs roster rows) logged at WARN — no behavior change.
- **H3 — Poison-row isolation.** Replace `collect::<Result<...>>()` with per-row `filter_map` that logs a
  WARN (ids only, no personal data) and increments a dropped-rows counter, so one bad bot cannot blank the
  rail. *Risk:* masks corruption — must be paired with alerting; verify no security-relevant row is lost.
- **H4 — Freshness signal.** Add a `version`/etag (max `updated_at` across bots/threads/read states plus
  max message seq) to `BotSummary`, or an HTTP ETag on the roster endpoint. *Risk:* API/DTO change is a
  DEVIATION under AGENTS.md and needs operator approval; cheaper fallback is a documented "do not cache
  longer than N seconds" contract.
- **H5 — Classify and retry transients.** Retry 40P01/40001 once at the `with_tenant` boundary (fresh
  transaction, never inside a poisoned one), add a statement timeout, keep `Unavailable` for real outages,
  and map a bad `until` cast to `Rule`. *Risk:* adds latency under contention; retry must re-run the whole
  closure and keep tenant-scoped settings.
- **H6 — Align clear path.** Add the same ownership `EXISTS` guard to the `DELETE` (or explicit
  `space_id` scope as defense in fact) and return `NotFound` for foreign/missing threads, or document
  clear as deliberately idempotent. *Risk:* flips clear semantics — confirm clients do not rely on
  unconditional success first.
- **H7 — Observability.** Emit a tracing span with bots returned, dropped rows and query latency; confirm
  the unread subquery stays backed by `messages_thread_seq_idx` (`0002:292`) and consider a lateral
  aggregate only if bot counts grow. *Risk:* low; premature aggregation adds complexity.

## Recommended v0.4.0 order

H3 + H2 (diagnostic variant) + H5 first — no API change: stop blanking the roster, see drops, survive
transients. Then H1 + H6 for integrity and contract consistency. Defer H4 behind approval (DEVIATION).

## Tests to add (REQ id in the name)

- `req_009_two_threads_for_one_bot_yield_one_row` — H1/H2.
- `req_009_a_bot_without_a_thread_is_reported_not_silently_dropped` — H2.
- `req_009_one_malformed_roster_row_does_not_blank_the_list` — H3, asserts WARN/counter.
- `req_009_snooze_clear_on_a_foreign_thread_is_refused` — H6.
- `req_009_transient_serialization_failure_is_retried_on_a_fresh_transaction` — H5, fault injection.

## Open questions for the operator (do not guess silently)

- Do helper/group threads ever share the visible bot's `bot_id`? Decides H1 partial vs full unique index.
- Is a roster version/ETag (H4) in v0.4.0 scope, given the DTO change needs parity approval?
- Should clear-snooze stay idempotent-success by contract (H6), or flip to `NotFound`?
