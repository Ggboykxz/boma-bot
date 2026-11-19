# Worker 0518 — `migrations/0053_groups.sql` — Reliability (v0.4.0 proposal)

Scope: `groups`, `group_memberships`, their two indexes and the RLS block (REQ-010 / REQ-043),
and the seams with `0002_m1.sql` (spaces/bots/threads/users), `crates/infra/src/stores/groups.rs`,
`crates/app/src/usecases.rs:9278-9335` and the in-memory mirror `crates/testkit/src/memory.rs`.
Evidence tags: CONFIRMED = read this session, GUESS = needs a test/oracle run. Auditor proposal; no
code or schema changed. Every suggestion is **additive** (new constraints, indexes, `SET DEFAULT`,
`COMMENT`, tests) — no drops, rewrites or wire change unless flagged as a DEVIATION needing an ADR.

## Current state (CONFIRMED)

0053 creates `groups (id PK, space_id FK spaces CASCADE, name, thread_id FK threads CASCADE,
created_at)` with a non-empty-name CHECK and `groups_space (space_id)`, then `group_memberships
(group_id FK groups CASCADE, member_kind IN ('person','bot'), member_id text, created_at DEFAULT
now(), PK(group_id,member_kind,member_id))` plus `group_memberships_bot (member_kind,member_id)`.
Both tables ENABLE + FORCE RLS with the repo-standard space predicate; the membership predicate is a
correlated subquery reading the parent group's `space_id` (0053:28-43). Helpers are STABLE GUC readers
that fail closed when the tenant is unset (`0002_m1.sql:27-42`). The store writes thread + group +
all memberships in one `tenant::in_space` transaction (`groups.rs:49-114`); all reads are in-space
(`groups.rs:116-244`). The app enforces >=1 bot, <=MAX_GROUP_BOTS, dedupe, non-archived and in-space
bots before insert (`usecases.rs:9278-9312`). `threads.bot_id` is `FK bots ON DELETE CASCADE`.

## Failure modes — membership

**FM-1 — `member_id` has no referential target (CONFIRMED schema).** It is a bare `text`; the only
guard is the `member_kind` enum. Deleting a bot/user leaves an orphan membership row that
`find_for_bot`/`list_for_member` can still match. Worse, a membership in a space-A group can point
at a space-B bot: the RLS predicate checks only the group's space, never the member's. Today the
app avoids both by resolving every bot/person in the caller's space (`usecases.rs:9300-9305`), but
the DB enforces nothing, so a system-path write or future call-site drift silently crosses tenants.
GUESS: no integration test inserts a cross-space or dangling membership.

**FM-2 — "first bot member = thread owner" is unenforced and usually false (CONFIRMED code).**
The header says the thread's `bot_id` is the first bot member, and `bots_for` promises "creation
order" via `ORDER BY created_at, member_id` (`groups.rs:30-45`). But create binds the *same*
`group.created_at` to every membership (`groups.rs:82-101`), so `created_at` cannot order them:
the result is lexicographic by `member_id`. With request order `[bot_c, bot_a]`, `threads.bot_id`
is `bot_c` while `bots_for` returns `bot_a` first. Nothing in the DB ties any membership to
`threads.bot_id`. Any consumer trusting "first member = owner" reads the wrong bot.

**FM-3 — app-only cardinality/name guards (CONFIRMED).** `MAX_GROUP_BOTS`, dedupe and the non-empty
name live in the app; `name` has no upper bound, so a system-path writer can bloat storage/indexes.
A DB row cap needs a trigger, not a CHECK — ADR territory, not v0.4.0.

**FM-4 — N+1 on list (CONFIRMED, GUESS severity).** `list_for_member` calls `bots_for` once per group
(`groups.rs:234-238`); internal, no wire change; flagged to the store owner, out of migration scope.

## Failure modes — RLS

**FM-5 — system path bypasses both tables (CONFIRMED policy).** Under a set `boma.system_reason`,
`is_system()` is true, so `USING`/`WITH CHECK` are unconditionally true on `groups` and
`group_memberships`: zero isolation. All groups-store methods use `tenant::in_space`, so this is
safe today, but any future `as_system` read of memberships loses tenant scoping silently.

**FM-6 — correlated subquery is fail-closed but fragile (CONFIRMED).** For a membership whose group
is outside the current space, FORCE RLS hides the group, the subquery returns NULL, and
`NULL = current_space_id()` is NULL (deny). Verify this rather than assume it; a future change that
reads the parent as system would turn the NULL into a cross-tenant match.

**FM-7 — stale comment (CONFIRMED grep).** 0053:38-39 credits `boma.in_space` with keeping the check
"index shaped"; no such SQL function exists (the similar helper is Rust `tenant::in_space`). The
policy is a plain correlated PK lookup — documentation defect only.

## Failure modes — cascades

**FM-8 — deleting the owner bot destroys the whole group (CONFIRMED chain).**
`bots -> threads (CASCADE) -> groups (CASCADE) -> memberships (CASCADE)`. Removing one bot cascades
away the shared thread and every other member's group — a real landmine from `0002_m1.sql:158` and
0053:11,19. Changing the action to RESTRICT is externally visible → ADR + gated DEVIATION.

**FM-9 — `thread_id` is not UNIQUE (CONFIRMED).** Two groups can reference one thread, but
`find_by_thread` uses `fetch_optional` and `find_for_bot` uses `LIMIT 1` (`groups.rs:157-165,186-193`),
so one is silently chosen. GUESS: app IDs make collisions unlikely, but the invariant is implicit.

**FM-10 — DDL availability (GUESS).** `ALTER TABLE ... ADD CONSTRAINT` takes ACCESS EXCLUSIVE with
no `lock_timeout` and no `IF NOT EXISTS`; a long read blocks the deploy.

## Concrete hardening (additive, with risks)

1. **One group per thread (FM-9).** `ALTER TABLE groups ADD CONSTRAINT groups_thread_id_key UNIQUE
   (thread_id);` *Risk:* existing duplicate rows would violate it — verify
   `SELECT thread_id FROM groups GROUP BY 1 HAVING count(*) > 1` returns 0 first. Additive index-backed
   constraint; no wire change.
2. **Non-empty member id (FM-1).** `ALTER TABLE group_memberships ADD CONSTRAINT
   group_memberships_member_id_not_empty CHECK (length(btrim(member_id)) > 0);` *Risk:* verify no
   blank ids exist; rejects only degenerate writes, not valid ones.
3. **Document + test the tenant boundary (FM-5, FM-6, FM-7).** Keep FORCE RLS; add regression tests
   that a system read between tenants returns all memberships (prove the bypass) and that a member
   read of another space's membership returns zero rows (prove fail-closed). Correct the stale
   `boma.in_space` comment in a comment-only follow-up. *Risk:* none to data; tests must use a real
   DB, not the in-memory mirror.
4. **Make the owner invariant visible (FM-2).** `COMMENT ON TABLE groups` / `COMMENT ON COLUMN
   group_memberships.created_at` stating the owner is `threads.bot_id` and that equal timestamps make
   `bots_for` order by `member_id`; add a test asserting consumers never rely on "first member =
   owner". Durable fix (a stable `join_order` column or membership trigger) changes the write path →
   ADR. *Risk:* a `join_order` column needs a backfill; do not add it in v0.4.0.
5. **DB clock.** `ALTER TABLE groups ALTER COLUMN created_at SET DEFAULT now();` — additive, no effect
   on existing rows. *Risk:* keep one clock; do not mix app and DB values in an ordering column.
6. **Bound the DDL.** `SET LOCAL lock_timeout = '5s';` and `IF NOT EXISTS` in the companion migration.
   *Risk:* a busy deploy fails fast; `IF NOT EXISTS` can mask drift — verify first.
7. **ADR-gated (NOT v0.4.0):** cross-space/dangling membership FK needs a polymorphic redesign (typed
   `bot_id`/`person_id` + partial FKs); owner RESTRICT (FM-8), row/name caps (FM-3) and FM-4 likewise.

## Decisions needed (GUESS → operator/spec)

1. Is exactly one group per thread intended? If groups may share a thread, drop item 1 and make
   `find_by_thread` disambiguate instead.
2. May a bot belong to several groups? Both stores assume one (`LIMIT 1` / `.find()`); if yes, the
   read side is ambiguous.
3. On owner-bot deletion: cascade the group (today) or block/transfer ownership?
4. Is a DB-enforced membership cap and name max wanted (FM-3), and what values?

## Recommended order for v0.4.0

Ship 1, 2, 3, 5, 6 together — additive, no wire change. Keep 4 and item 7 ADR-gated. REQ-tagged tests:
`req_010_thread_id_is_unique`, `req_010_membership_id_not_empty`,
`req_010_system_read_bypasses_membership_rls`,
`req_010_member_read_of_other_space_membership_is_empty`, `req_010_created_at_defaults_to_db_clock`,
`req_010_owner_bot_deletion_cascades_group` (documents FM-8); existing REQ-043 tests are the gate.
