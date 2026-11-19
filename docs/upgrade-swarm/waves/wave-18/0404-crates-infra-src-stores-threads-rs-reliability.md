# Worker #404 — `crates/infra/src/stores/threads.rs` — LENS: reliability — v0.4.0 proposal

Status: PROPOSAL (read-only review; no source changed).
Evidence: **CONFIRMED** = read this session from `threads.rs`, `rows.rs`, `tenant.rs`, `ports.rs`,
`bots.rs`, `groups.rs`, `messages.rs`, `events.rs`, `messaging.rs`, `domain/src/thread.rs`,
`migrations/0002_m1.sql` and `0042_messaging.sql`. **GUESS** = runtime consequence inferred, not
reproduced by a test this session.

## Scope

`PostgresThreadStore` implements only `ThreadStore::{find, for_bot}` (L24-79): the read side of the
thread aggregate. It owns no writes, yet it is the authority that reports which thread is *the*
conversation and the only reader that surfaces `next_message_seq`/`next_event_seq` into the domain.
Writers it must stay consistent with: `bots.rs:29-102`, `groups.rs:49-79`, `messaging.rs:437-467`;
counter allocators: `messages.rs:51-58`, `events.rs:44-51`.

## Failure modes

### 1. Duplicate creation / "one conversation" is only a query convention
- **D1 (HIGH, CONFIRMED + race GUESS):** `for_bot` picks one row with
  `WHERE bot_id=$1 AND kind='bot' ORDER BY created_at ASC LIMIT 1` (L60-67). The sole bot index is
  the *non-unique* `threads_bot_id_idx` (`0002_m1.sql:291`). Nothing stops two `bot`-kind threads for
  one bot (two concurrent `create_with_thread`, or a re-create after archive). `LIMIT 1` silently
  returns the oldest; `bots.thread_id` may point at the other, so the two identities disagree.
- **D2 (MED, CONFIRMED):** `for_bot` never reads `bots.thread_id`. It can return a thread the bot no
  longer points to (after a re-point or manual repair) with no error — read truth diverges from the
  write-side pointer.
- **D3 (MED, CONFIRMED):** `groups.rs:54-65` inserts a group thread with the *default* `kind='bot'`
  and `bot_id = first member bot`. `for_bot(owner)` can therefore return a group thread as the
  bot's personal conversation (it wins `ORDER BY created_at` if created first).
- **D4 (LOW, CONFIRMED):** `ORDER BY created_at ASC` has no tiebreaker; equal timestamps give a
  nondeterministic pick. `find(id)` ignores the pointer entirely, so a stale id reads a thread the
  bot no longer uses.

### 2. Sequence counters (`next_message_seq`, `next_event_seq`)
- **C1 (MED, CONFIRMED mechanism / GUESS impact):** this store is the only path that lifts the
  counters into a domain `Thread` (`rows.rs:309-310`); writers bump the same row with
  `UPDATE ... +1 ... RETURNING`. Nothing checks the counter against `MAX(seq)` in `messages`/`events`.
  A restored, backfilled or partially migrated row with a drifted counter is trusted: the next send
  either collides with `UNIQUE(thread_id, seq)` (→ `Conflict`) or skips arbitrarily, silently.
- **C2 (LOW, CONFIRMED domain / GUESS runtime):** counters are bare `i64` (`domain/src/thread.rs:30,32`)
  with no newtype or checked increment; SQL `+1` at `i64::MAX` errors → `Unavailable`. Remote, but
  unbounded threads make it nonzero.
- **C3 (MED, CONFIRMED):** counter allocation takes a row lock on the thread (`messages.rs:51`,
  `events.rs:44`). `find`/`for_bot` are plain SELECTs and do not block under MVCC — good. A caller
  that reads a `Thread` then acts later can see a stale counter, so callers must never compute seq
  themselves (the port already forbids it, REQ-005) — preserve that contract.
- **C4 (MED, CONFIRMED mechanism):** a missing thread row makes the allocators' `fetch_one` return no
  rows → `Unavailable` instead of `NotFound` (also flagged in 0358/0344). `find` returning `None`
  for a thread whose bot still exists is the same dangling condition, seen from the read side.

### 3. Orphan threads and dangling links
- **O1 (HIGH, CONFIRMED schema + GUESS runtime):** `threads.bot_id` is `FK ON DELETE CASCADE`
  (`0002_m1.sql:158`), so a thread with no bot (true orphan) is DB-prevented. The inverse is not:
  `bots.thread_id` is plain text with no FK (comment 148-149). A direct thread delete leaves a live
  bot pointing at nothing; `find`/`for_bot` return `None`. This store cannot distinguish "no thread"
  from "broken bot" — it only sees threads (cross-ref 0332 C1).
- **O2 (LOW, CONFIRMED):** external-thread creation uses pre-check + `ON CONFLICT DO NOTHING` +
  re-SELECT (`messaging.rs:410-467`) guarded by the partial unique index `threads_external_identity`
  (`0042_messaging.sql:19-21`). This is sound and deduplicates correctly — preserve it; the only
  weakness is that a post-insert failure is mapped to `Unavailable`, not a retryable conflict.

### 4. Error and scope hygiene (local to this file)
- **E1 (MED, CONFIRMED):** every driver error is flattened to `AppError::Unavailable` by
  `tenant::unavailable` (`tenant.rs:153-156`); no timeout/classification. Reads are cheap, so risk is
  mainly misreporting, not hangs.
- **E2 (MED, CONFIRMED):** `rows::thread` maps a malformed id/kind to a domain parse error (not
  `Unavailable`), unlike `rows::message` which softens bad rows. One corrupt row yields an internal
  error to the caller.
- **E3 (LOW, CONFIRMED/GUESS):** `find` relies solely on RLS (no `space_id` predicate). Under an
  owner/superuser role RLS does not apply → cross-tenant read is possible (latent, shared risk).

## Concrete hardening (with risks)

1. **Enforce one conversation in the DB.** Add a unique index on threads for the bot's personal
   conversation. *Risk:* a naive `UNIQUE(bot_id) WHERE kind='bot'` (0331 Upgrade 1) would also
   forbid group threads, which are currently mislabelled `kind='bot'` (D3) — reclassify group
   threads to `kind='group'` (or add a `purpose` column) in the same migration, or the index will
   reject legitimate group creation. Needs a pre-migration duplicate audit, `CONCURRENTLY`, and a
   DEVIATION entry. Coordinate with worker #331 so the two proposals do not conflict.
2. **Make `for_bot` authoritative.** Read `bots.thread_id` and confirm it names a `kind='bot'`
   thread in the same space; keep the indexed query only as a temporary migration fallback. Add
   `ORDER BY created_at ASC, id ASC` as an interim tiebreaker. *Risk:* the pointer is unconstrained
   text and may be blank/dangling (O1) — parse tolerantly and return `None`, never a domain error;
   if the two identities already disagree, which row is returned changes → DEVIATION note.
3. **Reconcile counters out of band.** Add a repair/metric job comparing counters to `MAX(seq)` in
   messages and events, plus a `CHECK (next_message_seq > 0 AND next_event_seq > 0)` migration.
   Replace bare `i64` with a checked `Seq` newtype (coordinate with 0230). *Risk:* a newtype touches
   the domain, possibly the wire → DEVIATION; do NOT add a reconciliation query to the hot `find`
   path — keep it an admin/metric task. If a DEVIATION is unacceptable for v0.4.0, ship only the
   CHECK + metric and defer the newtype.
4. **Dangling-bot reporting.** Since this store cannot see bots, add an admin query/report for bots
   whose `thread_id` resolves to no thread, and make `rows::thread` map malformed rows to
   `NotFound` (E2). *Risk:* new admin surface; no externally visible behavior change.
5. **Scope and error hygiene.** Add `AND space_id = $tenant` to both queries as defense in depth
   (E3), and set `statement_timeout`/`lock_timeout` in `with_tenant` (shared with 0358/0344).
   *Risk:* low; keep the HTTP error mapping stable to avoid a DEVIATION.
6. **Preserve the sound parts.** `find` by primary key, RLS via `with_tenant`, the external
   `ON CONFLICT DO NOTHING` + partial-unique-index dedup, and the transaction-local identity GUC
   (`tenant.rs:118-129`) must stay unchanged.

## Verification to add (REQ ids in test names)

- `req_004_two_concurrent_bot_creates_yield_one_readable_thread` — D1 race.
- `req_004_for_bot_follows_bots_thread_id_pointer` — D2.
- `req_004_group_thread_is_not_the_bot_conversation` — D3.
- `req_005_thread_find_counter_matches_max_message_and_event_seq` — C1.
- `req_043_find_is_space_scoped_under_owner_role` — E3.
- `req_054_corrupt_thread_row_is_not_an_internal_error` — E2.
- Keep conformance golden `bot-create` green as the regression baseline.

## Open questions (GUESS)

- Is `kind='bot'` on group threads intentional (D3)? If so, reclassification is a product decision
  and a DEVIATION; confirm against the original before item 1.
- Should `for_bot` trust the pointer or keep scanning? REQ-004 says "one continuous thread per bot",
  which favours the pointer, but confirm the exact spec wording before item 2.
