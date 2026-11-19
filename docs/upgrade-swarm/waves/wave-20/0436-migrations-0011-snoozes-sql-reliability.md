# 0436 — `migrations/0011_snoozes.sql` — reliability proposal (v0.4.0)

Worker #436 · lens: reliability · READ-ONLY review of `.worktrees/before-m5`. Proposals only; no code
changed. **CONFIRMED** = read in this tree this session; **GUESS** = inference needing a runtime test.
Anchors: `0011_snoozes.sql:8-33`, `0002_m1.sql:27-42` (`current_space_id`, `is_system`),
`0010_read_states.sql:7-31`, `crates/infra/src/stores/roster.rs:40-124`,
`crates/infra/tests/m1.rs:1030-1085`, `spec/01-requirements.md:74-80` (REQ-009). **Additive only.**

## Scope note

One table, `thread_snoozes` (`:8-16`): PK `(thread_id, user_id)`, three cascading FKs, two timestamps,
and `snoozed_until timestamptz NOT NULL`. A `DO $$ FOREACH ... ARRAY['thread_snoozes']` loop
(`:18-33`) enables + forces RLS and creates one permissive policy keyed on `space_id =
boma.current_space_id() OR boma.is_system()`. The deadline is **not** enforced by a job: the roster
filters at read time with `LEFT JOIN thread_snoozes ... s.snoozed_until > now()`
(`roster.rs:56-57`); clearing deletes the row (`roster.rs:88`); snoozing upserts with
`INSERT ... ON CONFLICT DO UPDATE` guarded by `WHERE EXISTS (thread in caller space)`
(`roster.rs:106-112`). RLS is defence in depth; the SQL predicates are defence in fact
(`roster.rs:43-44,103-105`). Everything below is a *later* migration / config change; the file itself
is untouched (additive-only).

## What is already right (keep)

- FORCE RLS + a closed-form policy: no tenant set ⇒ `current_space_id()` is NULL ⇒ comparison false,
  so a member context fails closed; only an explicit `boma.system_reason` crosses tenants
  (`0002_m1.sql:30-42`). CONFIRMED.
- Three `ON DELETE CASCADE` FKs mean a deleted thread/space/user cannot leave an orphan snooze that
  hides a chat forever. CONFIRMED.
- `timestamptz` + `boma.rfc3339()` (`0002_m1.sql:49-53`) give one canonical UTC wire shape. CONFIRMED.

## Failure-mode inventory

F1 — No constraint on the deadline (CONFIRMED). `snoozed_until` has no CHECK and no sanity range. A
past timestamp produces a row that is invisible to the roster (`> now()`) yet never removed: a silent
no-op that accumulates dead rows. A typo/far-future value (year 9999, clock skew, wrong unit) hides a
chat effectively forever — exactly the outcome the header promises cannot happen. `created_at`/
`updated_at` are NOT NULL but have no `DEFAULT now()`: only the app's explicit `now()` keeps them sane;
a system-context writer can supply arbitrary values. A CHECK cannot use volatile `now()`, so a
trigger or boundary validation is required.

F2 — `space_id` is not tied to the thread's real space (CONFIRMED). The app's `WHERE EXISTS ... AND
space_id = $2` is the only binding (`roster.rs:109`). Under `boma.is_system()` RLS allows any
`space_id`, so a mis-scoped system write can store `(thread of space A, space_id = B)`. The row is then
visible only to B's RLS while pointing at A's thread; the roster join still finds A's thread, so the
snooze is inconsistent rather than safely isolated. No composite FK or trigger enforces agreement.

F3 — RLS is space-level, not member-level (CONFIRMED, gap). The policy grants full DML to *any* member
of the space: nothing in RLS limits `user_id` to the acting user. Correct app queries bind
`user_id = $1`, but a buggy/compromised query at the non-privileged role could read or delete another
member's snooze. No `boma.current_user_id()` helper exists yet (only proposed in wave-19 #0412 H8),
so a per-user policy cannot be written without adding that helper first.

F4 — Unbounded growth / no cleanup (CONFIRMED). Expired rows are filtered at read time and only
removed when a user explicitly clears (`roster.rs:88`). Nothing deletes expired rows, so the table
grows without bound on a busy deployment — contrary to the REQ-009 "at scale" rationale. There is no
index on `snoozed_until` or `space_id`, so a future cleanup scan would be a full sequential scan.

F5 — Write races and error classification (CONFIRMED in app, migration-adjacent). `ON CONFLICT DO
UPDATE` has no `updated_at` guard, so an older snooze can overwrite a newer one under interleaved
requests (last-write-wins, no lost-update check). A malformed `until` reaches `$4::timestamptz` and a
cast error is mapped through `tenant::unavailable` (likely 503) instead of a 400 — an input error
wearing an outage's costume. `now()` is transaction-start time, so very long transactions can act on
a slightly stale expiry.

F6 — Migration mechanics (CONFIRMED). The single-element `FOREACH` loop is a copy-paste pattern from
0010/0018/etc. and adds nothing; there is no `IF NOT EXISTS` idempotency guard, so a re-run against a
tracked DB fails on `CREATE TABLE`. Low severity (the migration tracker prevents re-runs) but worth a
note for disaster recovery.

## Concrete hardening (prioritized, all additive)

P0 — Bound the deadline. A new `BEFORE INSERT OR UPDATE` trigger rejecting `snoozed_until <= now()`
and a generous upper bound (e.g. `> now() + interval '1 year'`) with a clear SQLSTATE/message;
alternatively validate in the app boundary. Risk: a caller that intentionally snoozes to "now" to
clear would now error — verify no test/caller does (`m1.rs:1030-1085` uses explicit future/past
values, GUESS on product use of "indefinite"). Prefer a large cap or none to avoid rejecting
legitimate long snoozes; this prevents silent dead rows and forever-hidden chats without changing
the wire shape.

P0 — Index for expiry + tenant scans. `CREATE INDEX thread_snoozes_expiry ON thread_snoozes
(snoozed_until)` and optionally `(space_id, snoozed_until)`. Purely additive, no data change. Risk:
`CREATE INDEX CONCURRENTLY` cannot run inside a transaction block — confirm the migration runner
supports it or accept a brief lock (table is small). The PK already covers the roster join.

P0 — Periodic expired-row cleanup. Add a batched, idempotent `DELETE FROM thread_snoozes WHERE
snoozed_until <= now() ORDER BY ... LIMIT n` job under system context. Expired rows are invisible to
the roster, so deletion is behavior-neutral. Risk: must batch to avoid long transactions/bloat; must
run with `boma.is_system()` or RLS hides other spaces' rows; legacy past-dated rows (F1) get deleted
— intended, but confirm no product meaning attaches to them.

P1 — Bind `space_id` to the thread. Add a trigger validating `NEW.space_id = (SELECT space_id FROM
threads WHERE id = NEW.thread_id)` (a composite FK needs a redundant UNIQUE on `threads`; a trigger is
cheaper). Risk: preflight for existing mismatched rows or constraint creation fails; those rows are
the F2 bug and must be cleaned first. Rejects inconsistent system writes.

P1 — Per-user RLS as defence in depth. First add `boma.current_user_id()` reading a `boma.user_id`
GUC set by `tenant::with_tenant` (coordinate with wave-19 #0412 H8), then add a RESTRICTIVE policy
`USING (user_id = boma.current_user_id() OR boma.is_system())` with matching `WITH CHECK`. Risk:
system/maintenance paths without a user must keep `is_system()` set or they will be blocked; correct
app queries already bind `user_id`, so no query change expected. No wire change.

P1 — Timestamp hygiene. `ALTER COLUMN created_at/updated_at SET DEFAULT now()` plus a `BEFORE UPDATE`
trigger bumping `updated_at`; add a `WHERE thread_snoozes.updated_at < EXCLUDED.updated_at` guard to
the app upsert to stop stale overwrites (F5). Risk: the app's explicit `now()` and the trigger are
idempotent within a transaction; cross-module app edit needed for the guard.

P2 — Tests (integration, testcontainers, REQ-009 in the name): future snooze hides a chat; past
timestamp rejected; expired row invisible then removed by cleanup; cross-space insert blocked under a
member role and rejected under system by the F1/P1 trigger; cross-user access blocked by the new
policy; cascade on thread/space/user delete; malformed `until` returns 400. Wire-shape golden unchanged.

P2 — Classify bad timestamps at the HTTP/app boundary (400, not 503) so DB cast errors stop masquerading
as outages; internal reason code only, never expose SQL text.

## Open questions (route to docs/STATE.md)

- Is "snooze indefinitely" a supported product behavior that an extreme/past deadline encodes? If so,
  a trigger cap would be a DEVIATION — GUESS: spec shows only an RFC-3339 `snoozed_until`.
- Who owns the cleanup job and its cadence/batch size?
- Does `boma.current_user_id()` land in v0.4.0 (wave-19 dependency), or is user-level RLS deferred?

Status: all findings CONFIRMED by source reading except the noted GUESS items; no runtime verification
performed (read-only worker). No file other than this proposal was written.
