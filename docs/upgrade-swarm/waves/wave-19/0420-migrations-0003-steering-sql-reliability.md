# 0420 — `migrations/0003_steering.sql` reliability proposal (v0.4.0)

Module: `migrations/0003_steering.sql`
Lens: reliability
Spec anchor: REQ-003 (a message during an active run steers it); REQ-043 / ADR-002 (RLS tenant
isolation from the first commit); AGENTS.md (versioned, forward-only, parameterized migrations;
traceability). Code anchors read this session: `crates/infra/src/stores/runs.rs`,
`crates/domain/src/run.rs`, `crates/app/src/worker.rs`, `crates/infra/src/db.rs`, `migrations/0002_m1.sql`.

Status of claims: "Current behavior" is CONFIRMED by reading those files. Failure modes and
hardening are GUESS (design inference, not implemented, not tested).

## Current behavior (CONFIRMED)

- The file is purely additive: two `ALTER TABLE runs ADD COLUMN`s plus two `COMMENT ON COLUMN`s.
  `steering_from_seq bigint` is nullable (NULL = no steering window); `steering_consumed_seq bigint`
  is `NOT NULL DEFAULT 0`. No CHECK, FK, index, or trigger is added.
- `runs` already carries RLS from `0002_m1.sql`: `ENABLE` + `FORCE ROW LEVEL SECURITY` and a policy
  `runs_tenant_isolation` built in a DO block as
  `USING (space_id = boma.current_space_id() OR boma.is_system()) WITH CHECK (...)`. The policy has
  no column list, so it automatically covers new columns; ADD COLUMN does not drop it.
- Migrations are embedded with `sqlx::migrate!` (`crates/infra/src/db.rs`) and run forward-only, each
  file in its own transaction — a failed migration rolls back as a unit.
- All steering invariants live in Rust, not the DB: `set_steering_from` uses a CASE that keeps the
  *smallest* seq (first pending message wins); `set_steering_consumed` uses `GREATEST` and an
  optional `lease_fence` guard; the worker passes `latest_seq` (the highest seq the provider pass
  answered) as the consumed value; requeue resets both columns to NULL / 0.
- `Run::steering_pending()` is `steering_from_seq.is_some_and(|from| from > steering_consumed_seq)`.
  Message seqs are 1-based (`threads.next_message_seq DEFAULT 1`).

## Failure modes

1. **Unenforced value domain (constraints).** Nothing stops a negative `steering_consumed_seq`, a
   zero/negative `steering_from_seq`, or a wrong-type write at the SQL layer. A stray `0` or `-1`
   from a future writer or manual fix flips `steering_pending()` and can loop the worker through
   `MAX_STEERING_PASSES` or silently drop a real steering message. The monotonic updates are also
   unguarded: a refactor that drops `GREATEST`/the MIN-CASE could move the window backward with no
   database error.
2. **Tempting-but-wrong cross-column constraint.** A CHECK such as
   `steering_from_seq >= steering_consumed_seq` is *not* a valid invariant: the worker writes
   `latest_seq` into `steering_consumed_seq`, which can legitimately exceed `steering_from_seq`, and
   requeue resets consumed to 0 in the same row. Adding it would reject valid worker writes and turn
   steering into `AppError::Conflict` / 500s. This is the main reliability trap in this module.
3. **RLS loss on a table rebuild.** Today RLS is preserved. A future hardening that recreates the
   table (`CREATE TABLE ... LIKE`, dump/reload, or rename dance) to add a constraint would drop
   `FORCE ROW LEVEL SECURITY`, the `runs_tenant_isolation` policy, and grants. With `FORCE`, even the
   table owner is tenant-scoped, so any backfill UPDATE that forgets the system reason
   (`boma.is_system()` / `boma.system_reason`) silently affects **zero** rows while reporting
   success — a quiet data-integrity failure, not an error.
4. **Backfill / default semantics.** On a fresh DB the runs table is empty at 0003, so no backfill.
   On an M1 upgrade, existing rows receive `steering_from_seq = NULL` and
   `steering_consumed_seq = 0` — the correct "no pending steering" state. Risk appears if a later
   change makes `steering_from_seq` NOT NULL or adds a CHECK violated by legacy rows, or if a
   large-table ADD COLUMN with a default takes a long ACCESS EXCLUSIVE lock on older PostgreSQL.
5. **Dangling-sequence reference.** `steering_from_seq` is a bare bigint with no FK to
   `messages(thread_id, seq)`. A message pruned after steering was set would leave a seq that no
   longer exists; the worker would fold in "steering" text that is not there (or none). Rust never
   checks existence at write time.

## Concrete hardening for v0.4.0 (GUESS, additive only)

1. Add two unary CHECKs in a **new** numbered migration (do not edit 0003):
   `steering_consumed_seq >= 0` and `steering_from_seq IS NULL OR steering_from_seq > 0`. Use
   `ADD CONSTRAINT ... NOT VALID` followed by `VALIDATE CONSTRAINT` in the same file to keep lock
   time short. Every existing row already satisfies both (NULL or 0). *Risk:* if any writer ever
   adopts `0` as a "no steering" sentinel instead of NULL, the second CHECK breaks it — confirmed
   here that Rust uses NULL and real 1-based seqs, but verify no raw SQL writer disagrees.
2. Explicitly **reject** the cross-column CHECK in review, and instead document the real
   relationship in `COMMENT ON COLUMN` (e.g. "consumed may exceed from when the provider answers
   past the first steering message; requeue resets both"). *Risk:* none to add; the risk is a
   future well-meaning constraint that breaks live steering — record the rationale to prevent it.
3. Add a post-migration invariant test (infra integration tests) asserting `pg_policies` still
   contains `runs_tenant_isolation` for `runs` and `relforcerowsecurity = true`, so a future table
   rebuild cannot silently strip tenant isolation. Keep all hardening as `ALTER TABLE ... ADD
   CONSTRAINT` / `COMMENT` only. *Risk:* the policy name is generated by 0002's DO block; a rename
   there must update the test.
4. If any data backfill is ever added, require it to run under a system reason and in batches with
   `WHERE steering_from_seq IS NULL`, asserting an expected row count. *Risk:* a forgotten system
   context is a silent no-op under FORCE RLS; always verify affected rows, never assume success.
5. Consider (deferred, high-risk) a composite FK
   `(thread_id, steering_from_seq) REFERENCES messages(thread_id, seq)` to kill failure mode 5.
   *Risk:* message deletion/cleanup would then violate the FK and fail the steering UPDATE; message
   retention is not verified here, so do NOT ship this without confirming messages are never
   removed while a run is active. Prefer an application-level existence check + test instead.
6. Do not add a trigger to enforce monotonicity: it would fight the requeue reset (mode 2), add
   write cost, and interact with RLS. Keep `GREATEST` / MIN-CASE plus a regression test as the
   guard. *Risk:* residual DB-level gap remains for non-store writers — accept and document it.

## Constraints

- Additive only: no edits to 0003, no column drops/renames, no table rebuild, no policy changes.
- Wire behavior (`RunView.steering_from_seq` / `steering_consumed_seq` serde defaults) and REQ-003
  semantics must not change; a constraint that rejects an existing valid write is a regression.
- RLS isolation (REQ-043 / ADR-002) and the fence-based stale-worker protection (REQ-012) must stay
  intact; all backfills respect FORCE RLS via a system path.
- Claims stay GUESS until backed by a migration run plus the policy-invariant and CHECK tests.
