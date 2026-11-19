# Wave 20 / Worker 0438 — `migrations/0012_attempt.sql` (LENS: reliability)

Proposal for v0.4.0. Additive only: every change below *adds* a constraint, comment, guard or test;
nothing already applied is rewritten, dropped or destructively back-filled. Apply as new numbered
migrations (e.g. `0060_attempt_hardening.sql`) plus test files — never by editing `0012_attempt.sql`.

## Current structure (CONFIRMED — read this session)

`0012_attempt.sql` is a 6-line forward-only migration. Its single statement:
`ALTER TABLE runs ADD COLUMN attempt integer NOT NULL DEFAULT 1 CHECK (attempt >= 1);`
No `UP`/`DOWN` (matches the one-way convention, CONFIRMED), no explicit constraint name, no
`IF NOT EXISTS`, no upper bound. The constant `DEFAULT 1` means the `ADD COLUMN` is metadata-only
on PostgreSQL 11+ (no table rewrite); existing rows read as 1.

Related surface read this session (CONFIRMED):
- `runs` is RLS-protected since `0002_m1.sql`: `FORCE ROW LEVEL SECURITY` with policy
  `runs_tenant_isolation` on `space_id = boma.current_space_id() OR boma.is_system()`, for both
  `USING` and `WITH CHECK`. A query without tenant context sees nothing (fail-closed).
- `runs.lease_fence bigint NOT NULL DEFAULT 0` grows on each claim and is reset to 0 on retry
  (REQ-012); `runs` carries partial unique index `runs_thread_client_nonce_key (thread_id,
  client_nonce)`.
- `infra/src/stores/runs.rs::retry` (line ~431) increments in place, atomically:
  `UPDATE runs SET status='queued', attempt = attempt + 1, … lease_fence=0 … WHERE id=$1 AND
  status='failed' AND space_id=$2 RETURNING …`, via `tenant::with_tenant`; `fetch_one` turns a
  zero-row result (concurrent retry sees `queued`) into `AppError::Conflict`.
- `infra/tests/m1.rs` already pins: retry sets `attempt == 2`, a second retry is refused, a
  completed run never moves; auto-retry/re-claim also lands on `attempt == 2`.
- `spec/01-requirements.md` REQ-012/013/014 (state machine, in-flight survives deploy, orphan
  recovery); `spec/02-data-model.md` lists a separate `Attempt` entity (`runId, fence, status,
  error, startedAt, finishedAt`) — no such table exists in `migrations/` (CONFIRMED); `attempt`
  here is a counter, not that history. `DEC-02` (stale-write ignored vs rejected) is still GUESS.

## Reliability failure modes

1. **Constraint — non-idempotent DDL.** A re-run after a partial apply (column created, migration
   bookkeeping failed) errors with "column already exists"; the swarm's own re-run/rollback drills
   hit this. The auto-generated CHECK name (`runs_attempt_check`) is an implementation detail of
   PostgreSQL, so a later migration cannot safely reference it.
2. **Constraint — counter overflow.** `integer` caps at 2,147,483,647. `attempt = attempt + 1`
   is not guarded; a runaway auto-retry loop would turn a retry into a numeric-out-of-range DB
   error (500-class) instead of a clean refusal. Unreachable in normal use, but a liveness trap.
3. **Constraint — weak invariant.** `CHECK (attempt >= 1)` says nothing about *how* the counter
   changes; nothing at the DB level prevents a stray `UPDATE … SET attempt=…` or a manual status
   change from desynchronising the counter from the finite-state machine (REQ-012).
4. **RLS — misleading error under wrong tenant.** A stale/incorrect `boma.space_id` makes the
   retry `UPDATE` match zero rows (defence-in-depth: no cross-tenant write), but `fetch_one` maps
   that to `Conflict` (409), indistinguishable from a genuine double-retry. Fail-closed and safe,
   but hard to diagnose. System-path auto-retry bypasses via `boma.is_system()` (CONFIRMED).
5. **RLS — future attempt-history insert.** If the `Attempt` history table from the data model is
   added later, an insert fired by a trigger runs under the caller's RLS context; on member paths
   a child table without its own tenant policy would reject the write, silently losing history.
6. **Uniqueness — retry atomicity is the only guard.** There is no uniqueness constraint involving
   `attempt`. Exactly-once increment relies on READ COMMITTED row locking + the `status='failed'`
   predicate + EPQ; under REPEATABLE READ / SERIALIZABLE a concurrent retry can surface a
   serialisation failure instead of zero rows (mapped to `Unavailable` — retryable, not wrong
   state). Acceptable, but undocumented.
7. **Uniqueness — nonce index coupling.** Retry keeps `client_nonce` unchanged (in-place update),
   so it does not collide with `runs_thread_client_nonce_key`. Any future "retry = insert new run
   row" design would collide with that index and violate the "one conversation, no duplicate run
   rows" contract. Also, a DB-level "one active run per bot thread" index, if added in v0.4.0,
   could reject the `failed → queued` transition as 23505.

## Concrete hardening (additive only)

- **H1 — idempotent, named re-application.** New migration uses a `DO $$` block: `ADD COLUMN IF
  NOT EXISTS attempt …`, then `ADD CONSTRAINT runs_attempt_check CHECK (attempt >= 1)` only if
  `pg_constraint` has no such name. Deterministic name enables all later guards. *Risk*: low;
  re-validating an existing column is metadata-only. Audit `pg_constraint` before/after.
- **H2 — optimistic overflow guard in `retry`.** Add `AND attempt < 2147483647` to the retry
  `WHERE`, so a maxed counter is a clean `Conflict` rather than a DB error. *Risk*: low, but it is
  an application wire behaviour at the extreme; document as a safety edge, not a retry cap. Do
  **not** change `integer` → `bigint` here (table rewrite + long `ACCESS EXCLUSIVE` on a hot table)
  — that needs its own ADR.
- **H3 — column contract comment.** `COMMENT ON COLUMN runs.attempt`: monotonic counter, starts 1,
  incremented only by `retry`/auto-retry, the finite-state machine (REQ-012) owns status, the
  `Attempt` history entity is a separate concept. *Risk*: low (metadata); mark the history-entity
  relationship GUESS until a table exists.
- **H4 — RLS diagnosis test (no schema change).** Add `req_012_retry_under_wrong_tenant_affects_no_
  rows` and `req_012_system_path_auto_retry_still_increments`: assert zero-row + intended error on
  a bad tenant context and `attempt == 2` under `boma.is_system()`. *Risk*: low; tests only.
- **H5 — concurrency regression test.** Two simultaneous retries of one `failed` run: exactly one
  succeeds, final `attempt == 2`, no duplicate row — extends the existing `m1.rs` pins. *Risk*:
  low; test only. Keep EPQ/READ-COMMITTED reasoning in a comment.
- **H6 — uniqueness gate test.** If/when a "one active run per bot thread" constraint is proposed,
  add a test that the `failed → queued` retry succeeds when no other active run exists and is
  refused cleanly when one does; never add such an index without that gate. *Risk*: medium — it
  can reject legitimate retries; owner sign-off (DEVIATION) before shipping.

## Additive-only guarantee

H1/H3 are metadata/constraint additions; H2 adds a predicate (no data change); H4/H5/H6 are tests.
No `DROP`, no `ALTER COLUMN … TYPE`, no destructive `UPDATE`, no back-fill. Each new constraint is
namable and idempotent. Applied-migration hash chain preserved by using new numbered files.

## Verification before merge

- Apply all migrations to a fresh DB and to a copy of the current schema; assert row counts and
  existing `attempt` values unchanged, and that H1 re-application is a no-op.
- Run H4/H5/H6 against PostgreSQL; record results in `docs/STATE.md` as CONFIRMED only if run.
- Keep DEC-02 flagged GUESS; do not silently choose "rejected vs ignored" in code here.
- Claim status: structure/RLS/store/test facts above are CONFIRMED; the `Attempt`-history
  relationship and any "one active run" index are GUESS/proposal.

Status: proposal only — no file outside this report was modified.
