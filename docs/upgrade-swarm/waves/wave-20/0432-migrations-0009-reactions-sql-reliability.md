# 0009_reactions.sql — reliability upgrade proposal (v0.4.0)

Worker #432 · lens: reliability · REQ-008
Scope: ADDITIVE ONLY — every item below is a new object added by a **new** forward-only
migration (`ADD CONSTRAINT`, `CREATE INDEX CONCURRENTLY`, `COMMENT ON`, a guard trigger, tests).
Nothing in 0009 is rewritten or dropped. Do **not** edit 0009 in place: `sqlx::migrate!` checksums
applied files (CONFIRMED, `crates/infra/src/db.rs:20`), so an in-place edit breaks deploys.
Evidence status: CONFIRMED for file/tree reads this session; GUESS where inferred from conventions.

## Current structure (CONFIRMED)

- `message_reactions` has four cascading FKs (thread, message, space, user), `emoji text NOT NULL`,
  `created_at timestamptz NOT NULL` (no default), PK `(message_id, user_id, emoji)`, and exactly one
  extra index on `message_id` (redundant with the PK prefix).
- RLS enabled + FORCED with `<table>_tenant_isolation` using/with-check
  `space_id = boma.current_space_id() OR boma.is_system()`; helpers read `boma.space_id` /
  `boma.system_reason` GUCs (`0002_m1.sql:27-42`).
- Writer (`crates/infra/src/stores/reactions.rs`): Add is `INSERT … SELECT … FROM messages WHERE
  id=$3 AND space_id=$4 ON CONFLICT (message_id, user_id, emoji) DO NOTHING`; Remove is a
  `DELETE` on `(message_id, user_id, emoji)` with no explicit space predicate; truth is
  `GROUP BY emoji` with `count(*)`/`bool_or`. Emoji validity is checked only in Rust (`valid_emoji`:
  trimmed, 1–16 chars, no control chars), then stored as `emoji.trim()`.

## Failure modes (ranked)

1. **Emoji validity is boundary-only (dedup-adjacent).** The PK guarantees "one row per
   (message, user, emoji)" but says nothing about the emoji's shape. A direct/backfill/system-path
   write can insert `''`, a 10 kB string, or control characters; hydration then produces broken
   emoji keys, unbounded row growth, and a `count(*)` that saturates the Rust `u32` cast
   (`unwrap_or(u32::MAX)`) silently. No DB invariant enforces what every Rust writer assumes.
2. **RLS bypass is a plain GUC (defense-in-depth gap).** `boma.is_system()` is true whenever
   `boma.system_reason` is a non-empty string; any connection can `SET` that two-part GUC, and
   `FORCE RLS` still does not bind a superuser/BYPASSRLS role. Cross-tenant safety currently rests
   on the query's `space_id` predicate, not the policy — the Remove path notably has no explicit
   `space_id` filter and relies on RLS. A future writer on a system path that omits the predicate
   reads/writes every tenant's reactions silently.
3. **Cascade deletes are unindexed on 3 of 4 FK columns.** Only `message_id` is indexed (and that
   index duplicates the PK prefix). Deleting a `thread`, `space`, or `user` forces PostgreSQL to
   scan `message_reactions` for referencing rows — a full scan plus long locks and bloat on large
   spaces. Four cascade paths reach this table (space→message, space→thread→message, user→,
   thread→message); PostgreSQL allows them, but the delete storm lands in one transaction.
4. **`space_id`/`thread_id` are correct only by writer convention.** The Add query derives them
   from `messages`, but the DB has no constraint tying them to the referenced message; a drifted
   `space_id` makes a row invisible to tenant RLS (orphaned data, wrong reaction counts) with no
   error. The PK omits both columns, so it cannot catch the mismatch.
5. **Migration is not idempotent.** `CREATE TABLE` (no `IF NOT EXISTS`) and `CREATE POLICY` (no
   guard) fail on re-run; 0001 explicitly documents idempotency as a harness property, so a retry
   after a partially applied deploy can wedge `boma migrate`.
6. **Cascade + FORCE RLS interaction is unverified.** RI cascade actions are generally exempt from
   RLS, but with `FORCE` on a global `users` delete the policy predicate is `NULL` (no tenant set)
   unless `is_system()` is true; a normal-tenant cascade whose parent's rows share the tenant is
   expected to pass. GUESS — confirm empirically, do not assume.

## Proposed hardening (each independently additive; new migration)

### U1 — DB-level emoji contract
`ALTER TABLE message_reactions ADD CONSTRAINT message_reactions_emoji_check
CHECK (btrim(emoji) <> '' AND char_length(emoji) <= 16 AND emoji = btrim(emoji));`
Use `ADD CONSTRAINT … NOT VALID` then `VALIDATE CONSTRAINT` to avoid a long exclusive lock;
validation scans without blocking writes. Pre-flight confirm no stored row violates it (all app
writes go through `valid_emoji`, so expected zero). Risk: LOW. Keep the bound aligned with Rust's
`chars().count() <= 16` — `char_length` counts the same for BMP/code points; review astral emoji
before landing. A future normalization change would need a new constraint, not an edit.

### U2 — Index the cascade FK columns
`CREATE INDEX CONCURRENTLY message_reactions_space_idx ON message_reactions (space_id);`
`CREATE INDEX CONCURRENTLY message_reactions_user_idx ON message_reactions (user_id);`
`CREATE INDEX CONCURRENTLY message_reactions_thread_idx ON message_reactions (thread_id);`
Rationale: removes full scans on space/user/thread deletion and makes the RLS predicate indexable.
Risk: LOW — storage + one more index write per reaction; measure with `EXPLAIN` on a seeded space.
Use `CONCURRENTLY` (no write lock) outside a transaction block. Do not drop the redundant
`message_reactions_message_idx` — that is not additive.

### U3 — Make tenant/thread membership structural
`ALTER TABLE messages ADD CONSTRAINT messages_id_space_key UNIQUE (id, space_id);` then
`ALTER TABLE message_reactions ADD CONSTRAINT message_reactions_message_space_fkey
FOREIGN KEY (message_id, space_id) REFERENCES messages (id, space_id) ON DELETE CASCADE;`
(mirror with `threads(id, space_id)` for `thread_id` if desired). Rationale: turns the writer's
"defense in fact" into a durable invariant; a row whose `space_id` drifts can no longer exist.
Risk: MEDIUM — pre-flight drift query (`SELECT … JOIN messages USING (id) WHERE space_id <> …`)
must return zero or the migration blocks; adds a uniqueness index on `messages`; extra cascade
path is harmless. Defer if cross-space message moves are ever planned.

### U4 — Guard the migration against partial re-application
Wrap the new DDL in `DO $$ … IF NOT EXISTS (SELECT 1 FROM pg_constraint/pg_indexes/pg_policies) …
END $$;` so a retried deploy is a no-op. Rationale: closes failure mode 5 for the new migration
without touching 0009. Risk: LOW — catalog checks add complexity; keep names stable.

### U5 — Document the contract in the catalog
`COMMENT ON TABLE message_reactions IS 'REQ-008 … one row per (message,user,emoji); space_id is the
RLS tenant key; is_system() bypass is intentional';` plus `COMMENT ON COLUMN` for `emoji`,
`space_id`, `created_at`. Rationale: non-obvious semantics (dedup key, system bypass, boundary-only
validation today) become discoverable to SQLx offline metadata and future migrations. Risk: NONE.

### U6 — Negative-path tests (additive test files, no code change)
Add store tests named with REQ-008: concurrent duplicate Add yields one row; cross-tenant Add/Remove
is denied under tenant RLS; deleting a message/thread/space/user leaves zero orphan reactions
(failure mode 6); an invalid emoji is rejected by U1's constraint. These pin the guarantees the
schema alone cannot express.

## Verification gate (implementing agent)

1. Apply 0001–0009 + the new migration on empty **and** seeded DBs; re-run the new migration, assert
   no-op (U4). 2. U1 pre-flight returns zero violating rows; U3 drift query returns zero.
3. `EXPLAIN ANALYZE` a space/user deletion before/after U2. 4. Run U6 tests + replay the REQ-008
   reactions round-trip (`crates/infra/tests/m1.rs:886`) and RPC golden; confirm counts unchanged.
5. `cargo sqlx prepare --check` (offline metadata still valid) and the migration harness.

## Explicitly out of scope (would break additive-only or change behavior)

- Dropping the redundant `message_reactions_message_idx` or rewriting the `DO $$ FOREACH` policy
  block into a shared helper (the ~30x duplication noted in the 0010 architecture proposal).
- Narrowing/removing `boma.is_system()`, adding a membership check for group chats (REQ-010), or
  changing cascade behavior (e.g. RESTRICT) — all observable behavior changes.
- Auto-touch triggers on `created_at`, or any backfill that rewrites existing reaction rows.
