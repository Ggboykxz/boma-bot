# Worker 0452 — `migrations/0019_computer_takeover.sql` — Reliability (v0.4.0 proposal)

Scope: the two added columns and the DB-level guarantees (or absence of them) around REQ-029.
Evidence tags follow AGENTS.md: CONFIRMED = read in this session, GUESS = inferred runtime behaviour.
Auditor proposal; no code or schema changed. All schema suggestions are **additive** (`ADD COLUMN IF
NOT EXISTS`, `ADD CONSTRAINT`, `CREATE INDEX`, `COMMENT`) — no drops, no rewrites, no wire change
unless explicitly flagged as a DEVIATION.

## Current state (CONFIRMED)

0019 adds `takeover_by text` and `takeover_at timestamptz`, both nullable, with no constraint,
index, FK or comment. The table has `UNIQUE (space_id, bot_id)` (0017:32) and `FORCE ROW LEVEL
SECURITY` with a **space-only** policy `space_id = current_space_id() OR is_system()` (0017:83-94);
no `boma.current_user_id()` helper exists. The app sets both columns together (usecases.rs
6027-6028, 6063-6064) and gates input on `takeover_by_user.is_some()` (usecases.rs 6225).
`users.id` is global `text PRIMARY KEY` (0002:70-79); ids are opaque, unbounded non-empty strings
(domain/src/ids.rs 20-36). SQLx applies each migration in a transaction (infra/src/db.rs 20, 67-71).

## Failure modes

**FM-1 — Exclusivity is a TOCTOU check, not a DB guarantee (CONFIRMED code path).**
`computer_takeover` reads the computer, checks `takeover_by_user` (usecases.rs 6015-6026), then
writes it. `computer_release` does the same (6055-6066). Neither takes `SELECT ... FOR UPDATE`,
and the store issues a blind full-row `UPDATE ... WHERE id = $1` with no predicate on the hold
(infra/src/stores/computer.rs 160-195). Two concurrent takeovers by different members can both
observe `NULL` and both succeed; the loser receives `Ok(computer)` while not holding the screen.
This directly violates the golden `computer-takeover-conflict.json` (second takeover must be
409/CONFLICT) and the "exclusive control" promise of REQ-029. GUESS: most likely during the
CAPTCHA/password hand-off burst the feature exists for.

**FM-2 — A stale full-row write silently clears a hold (CONFIRMED shape).**
The same `update()` writes `takeover_by`/`takeover_at` from the supplied struct on every call
(computer.rs 174-175, 193-194). Any caller holding a `Computer` loaded before a takeover (e.g. an
idle/screen/status update) overwrites the hold with `NULL`. The hold then vanishes from the UI
while the human still controls the screen, and input actions are no longer refused — a liveness
and safety failure. GUESS: which callers actually pass stale structs needs an audit of `update()`
call sites.

**FM-3 — Holder authorization is app-only and bypassable from SQL (CONFIRMED).**
RLS is tenant-scoped, not user-scoped, and there is no current-user GUC. "Only the holder may
release" (usecases.rs 6058-6061) is not expressible as a policy: any tenant-scoped writer, or any
`boma.is_system()` path, can read, steal or clear the hold. No column privilege or trigger defends
it. GUESS: low likelihood today (one app), but the failure is silent and the boundary should be
stated explicitly.

**FM-4 — Orphan holder id deadlocks the computer (CONFIRMED no FK).**
`takeover_by` is free text. A deleted/stale member id satisfies `is_some()`, so input stays
refused forever and no live user matches the release check. Because `waiting_takeover` holds the
queue (spec DEC-03), the run and its background triggers are stuck indefinitely. GUESS: triggered
by account deletion or import skew; rare but unrecoverable without manual SQL.

**FM-5 — The pair invariant is unenforced (CONFIRMED schema, CONFIRMED gating split).**
Both columns must be set or cleared together; only the app does so. A partial state
(`takeover_by` set, `takeover_at` NULL) makes gating refuse input while time-based UI/logic sees
no hold. App-supplied timestamps (computer.rs 175) also allow skewed/future `takeover_at`; there
is no DB `now()` floor. Fresh migration starts all-NULL, so a CHECK is safe today.

**FM-6 — Abandoned holds have no support for reclamation (CONFIRMED absence).**
`takeover_at` is written but never read or indexed by any query in this tree. A member whose
session dies without releasing leaves the hold set, replaying FM-4's stuck queue with a live id.
No TTL, reaper, or partial index exists. GUESS: the main operational failure once the app is used
for real sign-ins.

**FM-7 — DDL availability on a busy table (CONFIRMED SQL text).**
`ALTER TABLE ... ADD COLUMN` takes ACCESS EXCLUSIVE on `computers`; there is no `lock_timeout`
and no `IF NOT EXISTS`. A long-running computer query blocks the migration (and every computer
operation behind it) with no bound; a retry after a lost migration record fails outright. GUESS:
low frequency, high blast radius during deploys.

## Concrete hardening (with risks)

1. **Bound the DDL (FM-7).** `SET LOCAL lock_timeout = '5s';` then `ADD COLUMN IF NOT EXISTS` for
   both columns. *Risk:* a busy deploy fails fast and must be retried; `IF NOT EXISTS` can mask a
   wrong-type pre-existing column — verify no drift first.

2. **Enforce the pair invariant (FM-5).** `ADD CONSTRAINT computers_takeover_pair_check CHECK
   ((takeover_by IS NULL) = (takeover_at IS NULL))`. Additive, no wire change. *Risk:* blocking if
   violating rows exist — confirm the count is 0 first (expected for new columns); consider
   `NOT VALID` + `VALIDATE CONSTRAINT` to avoid a long exclusive scan.

3. **Prevent orphan holders (FM-4).** `ADD CONSTRAINT computers_takeover_holder_fkey FOREIGN KEY
   (takeover_by) REFERENCES users(id) ON DELETE RESTRICT` — preserves the invariant and forces an
   explicit release before account deletion. *Risk:* deletion fails while that user holds a screen;
   `ON DELETE SET NULL` would break (2) or need a trigger → ADR + DEVIATION note.

4. **Close TOCTOU/stale writes (FM-1, FM-2).** Companion code change, not schema: serialize with
   `SELECT ... FOR UPDATE` or a conditional `UPDATE ... WHERE id = $1 AND takeover_by IS $cond`
   checking `rows_affected`→409. Additive option: internal `takeover_revision bigint NOT NULL DEFAULT
   0`, outside `COMPUTER_COLUMNS` so the wire is unchanged. *Risk:* touches usecases.rs/infra, needs
   concurrency tests; a constraint alone cannot enforce exclusivity — this delivers the golden's 409.

5. **State the trust boundary (FM-3).** `COMMENT ON COLUMN` noting RLS is space-only and holder
   checks live in `computer_takeover`/`computer_release`; optional additive trigger auditing
   system-path hold changes to 0030. *Risk:* trigger cost — guard with `WHEN (NEW.takeover_by IS
   DISTINCT FROM OLD.takeover_by ...)`; a real policy needs a user GUC → ADR, out of scope.

6. **Support reclamation (FM-6).** Additive partial index `CREATE INDEX computers_takeover_held_idx
   ON computers (takeover_at) WHERE takeover_by IS NOT NULL` for a stale-hold reaper. The TTL,
   `SystemReason` and auto-release are a **behaviour change** → ADR/DEC-03 review (too-short TTL
   breaks manual sign-in). *Risk:* index cost negligible; auto-release mid-step is the real risk.

7. **Tests first (REQ-029 in every name).** `req_029_concurrent_takeover_yields_one_holder`,
   `req_029_stale_update_cannot_clear_hold`, `req_029_pair_check_rejects_half_state`,
   `req_029_holder_delete_policy`, `req_029_migration_is_idempotent`, plus replay of
   `computer-takeover-conflict.json` to confirm the 409/CONFLICT wire stays unchanged.

## Recommended order for v0.4.0

Ship 1, 2, 4, 6 (schema portion) and 7 together — additive, restoring exclusivity with no wire
change. Treat 3 (FK deletion policy), 5 (trigger/user-scoped RLS) and the reaper policy in 6 as
ADR-gated DEVIATION candidates needing owner approval before they alter behaviour (AGENTS.md:
externally visible changes go to docs/PARITY.md). Keep the golden and contract test as the
regression gate throughout.
