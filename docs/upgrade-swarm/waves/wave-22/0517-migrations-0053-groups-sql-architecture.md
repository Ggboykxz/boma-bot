# 0517 — `migrations/0053_groups.sql` — architecture proposal (v0.4.0)

- Worker #517 · Lens: architecture · Target: v0.4.0 · Read-only review; no source was changed.
- Module: `migrations/0053_groups.sql` (43 lines) at `before-m5` (v0.3.0-rc5): the schema for
  REQ-010 group chats.
- Read for context (CONFIRMED by direct reads): `0053`, `0001_init.sql`, `0002_m1.sql`,
  `0037_pages.sql`, `0047_access_grants.sql`, `0054_external_agents.sql`,
  `spec/01-requirements.md` (REQ-010), `spec/03-api-contract.md` (`groups.*`),
  `spec/golden/group-create.json`, `crates/infra/src/stores/groups.rs`, `crates/infra/src/db.rs:20`
  and the sibling proposal `0347-...-stores-groups-rs-architecture.md`. Forward-looking items are
  proposals (GUESS until implemented and tested).

## Current structure (CONFIRMED)

- `groups(id, space_id, name, thread_id, created_at)` (`:6-14`): `thread_id` FKs `threads` with
  `ON DELETE CASCADE`, `name` is trimmed-non-empty, and `groups_space(space_id)` (`:16`) is the only
  index. Unlike later tenant tables (`pages`, `bot_external_agents`, `app_meta`) it has **no**
  `updated_at` and **no** `COMMENT ON` of any kind.
- `group_memberships(group_id, member_kind, member_id, created_at)` (`:18-24`): composite PK
  `(group_id, member_kind, member_id)`, `member_kind IN ('person','bot')`, and
  `group_memberships_bot(member_kind, member_id)` (`:26`). `member_id` is **plain text with no FK**,
  so the polymorphic person/bot reference has no database-enforced integrity.
- Tenancy is deliberately inherited, not stored on memberships (`:38-43`): the policy is a correlated
  scalar subselect `(SELECT space_id FROM groups WHERE groups.id = group_memberships.group_id) =
  boma.current_space_id() OR boma.is_system()`. Both tables use direct `ENABLE`/`FORCE`/`CREATE
  POLICY` (`:28-31`), matching `0054` rather than the `0005` `DO` loop.
- The comment at `:39` cites a `boma.in_space` helper: **grep over `migrations/` finds it nowhere
  except this comment**. The analogous child-table policy in `0037_pages.sql:44-58` uses `EXISTS
  (SELECT 1 FROM pages p WHERE ...)`; this one uses a scalar subselect. Both are safe today only
  because `groups.id` is a PK (GUESS: no duplicate can make the scalar form error).
- Two load-bearing invariants live in comments/Rust, not the schema: `:10` says the shared thread's
  `bot_id` is the "first bot member", and `group-create.json:23-26` exposes `memberBotIds` in that
  order. But `groups.rs:82-98` inserts every membership with `group.created_at`, and `bots_for`
  orders `created_at, member_id` (`:32-34`) — so "first" is lexicographic id, not insertion order
  (sibling doc `0347`, item 3). `find_for_bot` does `ORDER BY g.created_at LIMIT 1` (`:187-192`)
  while the schema permits one bot in many groups (sibling doc `0347`, item 5).

## Guiding principle for v0.4.0

Additive only: ship a new forward-only migration (e.g. `0060_groups_hardening.sql`) that `ADD`s
columns/constraints/indexes/comments and leaves `0053` and all existing row values untouched.
Preserve the RLS contract `space_id = boma.current_space_id() OR boma.is_system()` (ADR-002 /
DEC-05) and the `boma.rfc3339(created_at)` alias the store selects. Any externally visible change
— especially member ordering or a new create-time rejection — is a DEVIATION for `docs/PARITY.md`
and needs owner approval before shipping (AGENTS.md). `sqlx::migrate!` is wired at
`crates/infra/src/db.rs:20`; GUESS: its `Migrator` wraps each file in a transaction, so an index
built `CONCURRENTLY` must be split into its own no-transaction migration.

## Concrete upgrades (5), each with a risk

1. **Pin the "thread owner is the first bot member" invariant in the schema.** Add a nullable
   `groups.owner_bot_id text REFERENCES bots(id)` plus a `group_memberships.bot_ordinal integer`,
   backfilled from today's exact read order so `memberBotIds` in the golden stays byte-identical;
   a later trigger can assert `thread_id`'s bot is the ordinal-0 member. *Risk:* the backfill must
   reproduce lexicographic `created_at, member_id` order or it silently reorders the API — a
   DEVIATION; a trigger under `FORCE ROW LEVEL SECURITY` with a lax `search_path` is fragile, and
   this overlaps worker #347's module. *Mitigation:* land a regression test first asserting create
   order == read order on ids chosen to break lexicographic sorting, derive the backfill from that
   test, add the column behind owner approval, and keep `SECURITY INVOKER` + `SET search_path =
   pg_catalog` in any trigger.

2. **Repair and pin the RLS inheritance contract, do not rewrite it.** Delete the stale
   `boma.in_space` sentence, add `COMMENT ON TABLE`/`COMMENT ON POLICY` describing that a membership
   inherits its group's tenant so door-keeping stays single-sourced, and add a regression test that
   both policies still exist with the ADR-002 expression. Aligning the subselect to `EXISTS` (as in
   `0037`) is a *separate*, semantics-preserving option. *Risk:* restating a policy is a DROP/CREATE
   — not strictly additive — and a test-name grep can pass while the expression quietly drifts;
   `FORCE RLS` means a mistake denies every member read. *Mitigation:* ship only comments + tests in
   this release, keep any `EXISTS` restatement for a later ADR, and cross-check with a two-space
   differential test under `boma.space_id`.

3. **Add `updated_at` metadata to `groups`.** `ADD COLUMN updated_at timestamptz NOT NULL DEFAULT
   now()` with a minimal `BEFORE UPDATE` trigger, matching `pages`, `connector_installs` and
   `bot_external_agents`, and optionally an audit `updated_at` on memberships. This gives REQ-011's
   activity view a change stamp without touching `created_at`. *Risk:* a hidden write can surprise
   a system-context update; `SELECT *` consumers would see a new column; using the stamp for
   ordering would change visible list order. *Mitigation:* keep the explicit column list in
   `groups.rs` unchanged, use `IF NOT EXISTS` on a re-runnable companion migration, and keep the
   field metadata-only in v0.4.0.

4. **Give polymorphic members real referential integrity.** `bots`/`users` delete with `ON DELETE
   CASCADE` but leave `group_memberships` rows pointing at dead ids (no FK to cascade). Add nullable
   typed columns `bot_id`/`person_id` with FKs to their parents and a consistency `CHECK` tying the
   chosen column to `member_kind`, first `NOT VALID`, after auditing for orphans. *Risk:* one FK
   cannot express a polymorphic target; backfill may discover existing orphans; a `NOT NULL`
   requirement would break the store's current single-column inserts, and a cleanup trigger could
   fire in the wrong tenant under `FORCE RLS`. *Mitigation:* keep `member_id` authoritative and the
   new columns nullable for this release, audit with `LEFT JOIN ... WHERE parent.id IS NULL`, and
   validate only when the count is zero.

5. **Make the "oldest first" list index-shaped and revisit one-bot-one-group.** Add
   `groups_space_created(space_id, created_at)` (keeping `groups_space`) so `groups.list`'s
   tenant-scoped `ORDER BY created_at` needs no sort. Separately, consider a partial UNIQUE index
   on `group_memberships(member_id) WHERE member_kind = 'bot'` to match `find_for_bot`'s `LIMIT 1`.
   *Risk:* the composite index may be ignored and adds write cost; the unique index changes
   behaviour (a second group for a bot fails) and the spec is silent on one-group-per-bot — GUESS,
   DEVIATION material. *Mitigation:* prove the index with `EXPLAIN (ANALYZE, BUFFERS)` on a seeded
   space, drop it if unused, and raise the uniqueness question in `docs/STATE.md` under "Decisions
   needed" while #347 keeps `LIMIT 1` unchanged.

## Cross-cutting risks / sequencing

- Sequence 2 → 3 → 5(index) → 4 → 1 → 5(uniqueness): pin the contract and add metadata first,
  measure the index, prove data hygiene, then the ordering invariant, and the behaviour-changing
  unique index last. Every item is an independent `ALTER`/`CREATE`, so partial application is safe.
- Keep `0053` byte-for-byte unchanged; this is critical because `sqlx` records migration hashes.
  Coordinate items 1/4/5 with worker #347 (`stores/groups.rs`) and the port in `ports.rs`; do not
  duplicate those edits. No new dependency is needed.
- All claims here are structural; nothing was applied or benchmarked. Verify on a scratch copy:
  apply the companion migration to a seeded database, run `cargo fmt --check`, `clippy -D warnings`,
  `tests/` integration plus a `REQ-010`-named test, replay `group-create.json`, and run
  `tools/trace_check.py` before claiming success.

## Out of scope for this module

The Rust store adapter (#347), the `GroupStore`/`NewGroup` port, mention/fan-out and bot-to-bot
handoff (`usecases.rs`, REQ-068), the HTTP DTOs, and the `boma` RLS helper functions belong to other
workers. This proposal only hardens the structure of the groups schema and its policies.
