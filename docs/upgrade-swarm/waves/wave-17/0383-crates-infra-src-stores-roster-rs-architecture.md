# Worker #383 — Architecture proposal: `crates/infra/src/stores/roster.rs` (v0.4.0)

Lens: architecture. Scope: REQ-009 (read state, unread divider, snooze) and the roster store
structure. Read-only review; no code changed. Claims are **CONFIRMED** (read in this worktree this
session) or **GUESS** (inferred).

## Current shape (CONFIRMED)

- `PostgresRosterStore { pool }` is a thin adapter: `const new` plus
  `RosterStore::{list_bots, set_snooze}` (`ports.rs:1239-1265`).
- `list_bots` is one SQLx query: `bots` INNER JOIN `threads` on `(bot_id, space_id)`, LEFT JOIN
  `thread_snoozes` with `user_id = $1 AND snoozed_until > now()`, LEFT JOIN `thread_read_states`,
  plus a correlated `count(*)` per row over `messages` with `role <> 'tool'` and
  `seq > COALESCE(last_seq, 0)` (`roster.rs:45-61`). Timestamps go through `boma.rfc3339`; the
  order is `created_at, id`.
- "The snooze is a filter" is a **NULL marker**, not a row filter: every bot is returned; only a
  future deadline yields `snoozed_until`, and past deadlines collapse to `None` through the join
  predicate (`roster.rs:56-57`, `migrations/0011_snoozes.sql:3-6`). Shelf placement (REQ-011) is a
  consumer decision — keep that separation of concerns.
- Tenant scoping is explicit `b.space_id = $2` plus a space predicate on the thread join
  (defense in fact), with RLS on both per-member tables (defense in depth)
  (`migrations/0010_read_states.sql:16-30`, `migrations/0011_snoozes.sql:18-32`).
- `set_snooze(Some)` is one guarded atomic write:
  `INSERT … SELECT … WHERE EXISTS(threads ∧ space_id) … ON CONFLICT … RETURNING`
  (`roster.rs:100-125`). `set_snooze(None)` is a plain `DELETE` keyed only on `(thread_id, user_id)`
  with no space predicate and an unconditional `Ok(None)` (`roster.rs:85-98`) — asymmetric.
- `until` is validated upstream in `usecases::snooze_thread` → `validate_until`
  (`usecases.rs:5421-5464`), along with a thread `find` and space check. The port documents
  `Rule`/`NotFound`, but the adapter itself never returns `Rule`: a direct caller gets `Unavailable`
  from the `$4::timestamptz` cast instead.
- `rows::bot_summary` (`rows.rs:277-300`) rebuilds a `BotRow`, calls `rows::bot`, and builds
  `ThreadId::new(row.thread_id.unwrap_or_default())` twice; `unread_count` uses
  `u64::try_from(...).unwrap_or(u64::MAX)` (silent saturation).
- Tests exist and carry the REQ id: `req_009_the_read_position_round_trips_in_postgres` and
  `req_009_the_roster_reads_unread_and_snooze_in_postgres` (`tests/m1.rs:974-1088`), a cross-tenant
  snooze probe (`tests/tenancy.rs:172-209`), and golden `ui/REQ-009-unread-divider.json`.
  Traceability is sound; the gaps below are structural, not naming.

## Invariants to preserve (do not "simplify" these away)

- No job or timer for snooze expiry: `snoozed_until > now()` in the read path is the whole
  mechanism (REQ-009, migration 0011).
- Background steps (`role = 'tool'`) never contribute to the badge.
- Read state stays per `(thread_id, user_id)`; there is no thread-wide position.
- One space predicate in fact **and** RLS in depth, including on superuser connections.

## Upgrade 1 — Give the clear path the same guard as the set path

Fold both branches into one guarded write: `DELETE FROM thread_snoozes s USING threads t WHERE
s.thread_id = t.id AND t.space_id = $2 AND t.id = $1 AND s.user_id = $3`, or the same `WHERE
EXISTS` shape the insert uses. Today clearing relies only on RLS and reports success for any thread
id, contradicting the "defense in fact" rule stated in this very file.
*Risks:* returning `NotFound` for an already-cleared or absent thread would break idempotent clears
(externally visible). *Mitigation:* keep `Ok(None)` for "no row to delete" and only scope the
DELETE; add a tenancy test asserting a foreign-space thread's snooze survives a clear.

## Upgrade 2 — Make the port's documented errors true of the adapter

Either (a) narrow the trait docs to say validation belongs to the use case, or (b) introduce a
validated `SnoozeUntil` newtype in `app`/`domain` carried through the port, so `$4::timestamptz` can
never receive `"tomorrow"` even when the store is called directly. The guarantee currently lives at
one call site out of potentially many.
*Risks:* (b) changes the public port and service wiring and may be a DEVIATION; two parsers
(`time::Rfc3339` and Postgres) can drift on accepted spellings. *Mitigation:* take (a) now plus a
direct-adapter test recording actual behavior; put (b) behind an ADR + `docs/PARITY.md` with a
single parser owning the format.

## Upgrade 3 — Collapse the correlated count and the ambiguous thread join

Replace the per-row `count(*)` subquery with one grouped lateral/aggregate join
(`LEFT JOIN LATERAL … GROUP BY thread_id`), and choose the canonical thread explicitly
(`t.kind = 'bot'` and/or equality with `bots.thread_id`) instead of relying on `bot_id` alone.
This removes an N+1-shaped cost on large spaces and prevents duplicate summaries if a bot ever has
two threads — already flagged in wave-15's bots proposal (non-unique index on `threads.bot_id`).
*Risks:* a new plan can change tie/order behavior; filtering `kind = 'bot'` changes behavior for
external threads (REQ-048) — a DEVIATION. *Mitigation:* keep `ORDER BY b.created_at, b.id`; assert
exactly one summary per bot and exact unread counts in the `req_009…` test; run `EXPLAIN ANALYZE`
on a seeded space before/after; pair with the proposed unique index.

## Upgrade 4 — Make row mapping total; stop one bad row failing the whole roster

`rows::bot_summary` turns a NULL `bots.thread_id` into `ThreadId::new("")`, which the domain
rejects, so one half-created bot makes every `list_bots` call fail. Build the id once, handle rows
with no canonical thread explicitly, and use a checked conversion instead of saturating to
`u64::MAX` for an impossible negative count.
*Risks:* **GUESS** whether NULL `thread_id` is reachable after bot creation; hiding such a bot is
user-visible. *Mitigation:* log and metric the anomaly, keep the bot listed when a safe fallback
exists, and add a test seeding a bot without a thread that asserts the other bots still return.

## Upgrade 5 — Name the clock contract and cover the boundary

The read path uses database `now()` and the write path uses `now()` for bookkeeping, while `until`
arrives as a client string and leaves through `boma.rfc3339()`. Document that the **database clock
is authoritative** for expiry, and add tests for a deadline exactly at `now()`, a past deadline,
and a malformed deadline reaching the adapter directly.
*Risks:* switching to an app-supplied clock invites boundary flakiness and timezone drift; a typed
instant on the wire is a DEVIATION. *Mitigation:* keep wire strings for parity, use fixed
timestamps in tests, and record the clock choice in `docs/STATE.md`.

## Sequencing

Take 1 and 4 first (contained fixes for real correctness holes), then 3 (query rewrite gated by
plan + tests), then 2 and 5 (contract/ADR work). Net effect: the roster keeps its single-query shape
and its no-job expiry model, while tenant checks, mapping, and the port's promises are each enforced
in one place — with no public API change unless the stricter variants of 2 or 3 are chosen.
