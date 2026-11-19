# 0524 — `migrations/0056_notifications.sql` — reliability lens (v0.4.0)

Worker #524 · module `migrations/0056_notifications.sql` · lens: reliability · READ-ONLY review of
`rebuild-workspace/.worktrees/before-m5`. Proposals only; no code changed here. **CONFIRMED** =
read in this tree this session; **GUESS** = inferred, needs a test.
Scope: `notification_subscriptions`, `notification_policies`, `notification_log` (72 lines).
REQ-064: content-free Web Push, per-bot policy, per-(space, member, bot) daily cap, dead
subscription deleted after repeated permanent failures.

## Already right (keep, do not re-propose)

- `UNIQUE (space_id,user_id,endpoint)` + non-empty CHECK dedup subscriptions at the DB layer;
  cascade FKs on all three tables prevent orphans (CONFIRMED).
- `daily_cap >= 0`, `kind IN ('waiting','completed')`, `fail_count DEFAULT 0`,
  `(space_id,user_id,bot_id,sent_at)` index exist (CONFIRMED).
- RLS ENABLE + FORCE with matching USING/WITH CHECK on all three tables. Helpers (`0002_m1.sql`):
  `current_space_id()` = `boma.space_id` GUC (NULL unset); `is_system()` = non-empty
  `boma.system_reason` GUC (CONFIRMED).

## Failure modes
### Dedup

- **F1. No idempotency key on `notification_log` (high).** No unique constraint on the log: an
  at-least-once dispatcher or worker restart inserts two rows per event, inflating cap counts
  (premature suppression breaks the REQ-064 cap case) and risking a second push (GUESS on app
  dedup strategy) (CONFIRMED absence).
- **F2. Key rotation collides with the unique index (medium).** p256dh/auth_secret rotate while
  endpoint stays: a plain `INSERT` on refresh violates `notification_subscriptions_owner` → 500,
  stale keys remain and every later push fails (CONFIRMED structure; GUESS on INSERT vs UPSERT).
- **F3. `fail_count` unbounded, no terminal state (medium).** No `CHECK (>= 0)`, no threshold, no
  `disabled_at`; REQ-064's "delete after repeated permanent failures" is fully app-side, transient
  429/503 look permanent, and a naive counter deletes healthy subscriptions (CONFIRMED; GUESS on app).
- **F4. `updated_at` is insert-only (low).** No touch trigger on subscriptions/policies, so key
  rotation and failures are invisible and sync watermarks drift (CONFIRMED).

### Cap

- **F5. Cap is app-trust only and race-prone (high).** Count-then-insert is not atomic: two runs
  both see 19 < 20 and both send. No DB constraint and no mandated `SELECT … FOR UPDATE` on the
  policy row; a second writer can exceed the cap (CONFIRMED absence; GUESS on app counting).
- **F6. "Daily" has no boundary (high).** `sent_at timestamptz` with a trailing index, no day
  bucket or timezone: `::date` resets at UTC midnight for a remote member; DST double-counts or
  skips an hour (CONFIRMED structure; GUESS on app using UTC).
- **F7. `daily_cap = 0` ambiguous (medium).** CHECK admits 0, default is 20; spec is silent on
  "block all" vs "unlimited" — two correct-looking implementations diverge.
- **F8. Log mixes attempted and delivered pushes (medium).** No status column, so a failed
  delivery consumes quota: member suppressed because delivery broke, not because they were
  notified (CONFIRMED).
- **F9. Log is immortal (medium).** At 20/user/bot/day it grows fast and no leading `sent_at`
  index exists for time-sliced pruning; autovacuum and cap-count latency degrade (CONFIRMED).

### RLS

- **F10. Missing context fails quiet on reads (high).** Unset `boma.space_id` makes USING false
  (`space_id = NULL`): a worker sees zero subscriptions and silently drops pushes; writes fail
  loudly. Nothing distinguishes "no subscribers" from "no context" (CONFIRMED; GUESS on GUC).
- **F11. FORCE RLS binds the owner (medium).** Backup/prune/cascade jobs as owner without
  `boma.system_reason` see nothing or fail writes; a blocked cascade retains orphans and inflates
  counts (CONFIRMED FORCE; GUESS on job identity).
- **F12. `is_system()` is a non-empty GUC check (high).** Any non-empty `boma.system_reason` grants
  global cross-tenant read/write on all three tables; the closed enum lives only in an SQL
  comment. A role able to `SET` it bypasses isolation (CONFIRMED SQL; GUESS on DB role grants).
- **F13. Three hand-copied policies drift (low).** A future change to one predicate desynchronises
  the others (CONFIRMED).

## Concrete hardening (v0.4.0, additive only — new objects, no destructive DDL)

Follow-up migration `0058`; put `REQ-064` in every new test name.

- **P1.** Nullable `dedup_key text` on `notification_log` + partial
  `UNIQUE (space_id,user_id,bot_id,kind,dedup_key) WHERE dedup_key IS NOT NULL`; reserve with
  `INSERT … ON CONFLICT DO NOTHING` before sending. NULLs keep legacy rows valid → F1.
- **P2.** `CHECK (fail_count >= 0)`, nullable `disabled_at`, trigger setting it only on repeated
  *permanent* failures, reset on success, plus a batched prune job → F3.
- **P3.** BEFORE UPDATE triggers setting `NEW.updated_at := now()` unless explicitly set → F4.
- **P4.** `notification_daily_counts(space_id,user_id,bot_id,day)` + SECURITY DEFINER
  `boma.reserve_notification_slot(...)` (SET search_path, tenant-checked): one
  `INSERT … ON CONFLICT DO UPDATE … RETURNING` RAISEing above `daily_cap`; call before send → F5.
- **P5.** Define `day = (sent_at AT TIME ZONE 'UTC')::date`; use plain column + BEFORE INSERT
  trigger if a STORED generated column's rewrite is too costly, index
  `(space_id,user_id,bot_id,day)` → F6.
- **P6.** `COMMENT ON COLUMN notification_policies.daily_cap` pinning 0 = no sends + REQ-064 test;
  confirm with spec owner (GUESS → decision needed) → F7.
- **P7.** Nullable `delivered_at timestamptz`; count only delivered rows toward cap, failures bump
  `fail_count` without consuming quota → F8.
- **P8.** `CREATE INDEX CONCURRENTLY notification_log_sent_at_idx ON notification_log (sent_at)`
  + bounded prune job → F9.
- **P9.** `boma.require_space_context()` / `boma.require_system_context()` at worker entry + alert
  on SQLSTATE 42501; `boma.as_system(reason)` SET LOCAL wrapper for owner jobs → F10, F11.
- **P10.** Inventory roles able to `SET boma.system_reason`, restrict to maintenance and audit
  usage → F12; CI assertion that the three tenant policies stay identical → F13.

## Risks / trade-offs

- **R1.** P1's partial index does not dedup legacy rows: reconcile first or counts stay wrong; key
  must come from the run event, not `now()`.
- **R2.** P4 runs SECURITY DEFINER under FORCE RLS: search_path/tenant bug mints cross-tenant slots;
  if the app also counts, P4 double-counts — migrate call sites in the same release.
- **R3.** P5 freezes UTC days into the schema; local-time days later need a non-additive ADR.
- **R4.** P2 auto-disable deletes healthy subscriptions unless P7 distinguishes permanent failures.
- **R5.** P3 overrides app-set `updated_at` (imports) — keep the explicit-set guard.
- **R6.** P5's STORED generated column rewrites the table (lock risk); prefer the trigger variant;
  build all new indexes CONCURRENTLY.
- **R7.** P10 may break jobs relying on a free `SET boma.system_reason` — inventory first.
- **R8.** No public API shape change (DEVIATION for `docs/PARITY.md`); every object must be
  RLS-/search_path-safe and covered by a two-tenant REQ-064 test.

## Non-goals / rejected options

- No change to existing columns, constraints, policies, indexes or cascades in v0.4.0.
- No partitioning, no secret-at-rest redesign (security lens), no contract-changing CHECKs.
- No decision on local-time cap days or `daily_cap = 0` without spec-owner confirmation.

## Suggested rollout

Wave A (invisible): P3, P6, P8, P9 observability half, P2's CHECK only. Wave B (backfill +
call-site migration): P1, P2 disabled_at, P4, P5, P7, P10; REQ-064 tests for cap-at-limit,
concurrent reserve, key rotation and two-tenant RLS.
Nothing executed; all source claims are from files read this session.
