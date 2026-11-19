# v0.4.0 architecture proposal — `migrations/0009_reactions.sql`

Provenance: `docs/upgrade-swarm/briefs-upgrade.jsonl` id 431 (module `migrations/0009_reactions.sql`,
lens `architecture`), worker #431. Read the `before-m5` worktree, v0.4.0 upgrade swarm. READ-ONLY on
all product files; this document is the only artefact written. English per repo disk convention.

Status: statements marked **CONFIRMED** were read from source this session; the upgrades and their
risks are **GUESS** proposals for the owning agent to validate. **All proposals are additive:** they
land as new forward-only migrations numbered after `0059` (new objects/constraints), never by
editing `0009` in place or dropping columns; any removal is flagged as a separate decision.

## Current structure (CONFIRMED)

- One table `message_reactions`: `thread_id`, `message_id`, `space_id`, `user_id` (all `NOT NULL`
  FKs with `ON DELETE CASCADE`), `emoji text NOT NULL`, `created_at timestamptz NOT NULL`; PK
  `(message_id, user_id, emoji)` encodes "one reaction per message/person/emoji" (REQ-008).
- `message_reactions_message_idx ON (message_id)` — a strict prefix of the PK, so it duplicates
  the PK's leading-column access path for the store's point lookups.
- RLS: `ENABLE` + `FORCE` plus one policy `<table>_tenant_isolation`, predicate
  `space_id = boma.current_space_id() OR boma.is_system()` in both `USING` and `WITH CHECK`,
  applied through a `DO $$ FOREACH ... IN ARRAY ARRAY['message_reactions']` loop using `format('%I')`.
  `boma.current_space_id()` reads `boma.space_id`; `boma.is_system()` reads `boma.system_reason`
  (`0002_m1.sql`); the system bypass is deliberate (D-09), not a defect.
- `space_id` and `thread_id` are denormalized copies of values already on the parent `messages`
  row (`0002_m1.sql`: `messages(space_id NOT NULL, thread_id NOT NULL, ...)`), but only `message_id`
  is in the PK. No constraint ties the copies to the parent, yet `space_id` is what RLS trusts.
- `emoji` has no in-database rule beyond `NOT NULL`; validation lives only in the Rust store
  (`valid_emoji`: trim, 1..=16 code points, no control chars). The store writes `thread_id` and
  `created_at` but never reads them (corroborated by wave-17 proposal 0379).
- The DO-loop/RLS block is boilerplate repeated near-verbatim in ~15 sibling migrations (0004,
  0005, 0010, 0011, 0014, 0017, 0018, 0021, 0025, 0026, ...), each restating the invariant.

## Architectural tensions (GUESS)

- T1 — **Tenant identity can drift.** RLS filters on the local `space_id`, but nothing proves it
  equals the parent message's space. A bad writer on the system path (or a future backfill) can
  assign a reaction to the wrong tenant or hide a valid one; the same holds for `thread_id`.
- T2 — **Index shape does not match the bulk read.** `list_for_messages` filters
  `message_id = ANY($1) AND space_id = $2`; the only secondary index is on `message_id` alone,
  which the PK already covers. There is no tenant-leading index for that query.
- T3 — **The `emoji` integrity rule lives outside the database.** Any non-store writer (system
  path, admin tool, future data migration) can insert arbitrary strings the API will later surface
  as reactions, bypassing the Rust boundary check entirely.
- T4 — **The isolation invariant is copy-pasted, not single-sourced.** ~20 inline copies of the
  same predicate mean a future correction must be applied by hand to every table, and a new table
  can silently receive a subtly different policy.

## Concrete upgrades (additive only, highest value first)

### U1 — Bind the denormalized tenant columns to the parent message
In a new migration add `CREATE UNIQUE INDEX messages_id_space_id_idx ON messages (id, space_id)`
and `messages_id_thread_id_idx ON messages (id, thread_id)` (the keys are logically unique via
`messages.id`, but PostgreSQL requires a matching unique index for a composite FK), then
`ALTER TABLE message_reactions ADD CONSTRAINT message_reactions_message_space_fk FOREIGN KEY
(message_id, space_id) REFERENCES messages (id, space_id) ON DELETE CASCADE` and the analogous
`(message_id, thread_id)` FK. Add them `NOT VALID`, run a drift query
(`SELECT ... FROM message_reactions mr JOIN messages m ON m.id = mr.message_id WHERE mr.space_id
IS DISTINCT FROM m.space_id OR mr.thread_id IS DISTINCT FROM m.thread_id`) and `VALIDATE` only on
zero rows.
*Risk:* the two unique indexes add write cost to a hot table, and the new FKs add a second cascade
path to the same parent; verify zero drift or validation fails, and do not "repair" drift by
rewriting rows without an owner decision. Deriving `space_id` from the parent and dropping the
copies is **not additive** — out of scope. Effort: M. Test: REQ-043 cross-tenant integration test
asserting a mismatched insert on the system path is rejected.

### U2 — Add the tenant-scoped index the read path actually uses
Add `CREATE INDEX message_reactions_space_message_idx ON message_reactions (space_id, message_id)`
(optionally `INCLUDE (emoji, user_id)` to make `list_for_messages` index-only) in a new migration.
*Risk:* an extra index raises insert/delete cost on a write-heavy table; confirm with
`EXPLAIN (ANALYZE, BUFFERS)` at realistic page sizes before keeping it. Dropping the now-redundant
`message_reactions_message_idx` is a separate, non-additive cleanup — flag it, never bundle it.
Effort: S. Test: REQ-008 query-plan assertion or benchmark captured in `docs/PARITY.md`.

### U3 — Make the emoji rule a database constraint
Add `ALTER TABLE message_reactions ADD CONSTRAINT message_reactions_emoji_check CHECK
(char_length(emoji) BETWEEN 1 AND 16 AND emoji ~ '^[^[:cntrl:]]+$') NOT VALID`, validate after a
drift check, plus `COMMENT ON COLUMN message_reactions.emoji IS 'REQ-008: trimmed, 1..=16 code
points, no control chars; mirrors valid_emoji in the reactions store.'`
*Risk:* the SQL rule must mirror Rust's exact pre-trim `valid_emoji` semantics; `char_length`
counts code points, not graphemes, so this intentionally does **not** tighten Unicode handling.
Any stricter rule (NFC, grapheme count) is a DEVIATION per wave-17 U4 and needs the parity decision
first. Effort: S. Test: REQ-008 test that a direct SQL insert of an empty, control, or 17-char
emoji fails.

### U4 — Single-source the tenant RLS policy in a `boma` helper
Add `boma.enable_tenant_rls(p_table regclass)` in a new migration (the `boma` schema already holds
helpers), doing ENABLE/FORCE plus `CREATE POLICY` with the standard predicate via `format('%I')`,
idempotently (`DROP POLICY IF EXISTS` first). Convert only **future** migrations to call it; leave
`0009` and its siblings untouched this wave.
*Risk:* dynamic DDL must stay injection-safe and `search_path`-pinned; one shared helper means one
bug affects every adopting table, so add a regression test comparing `pg_policies` for each adopting
table with the helper's output. Keep the `OR boma.is_system()` bypass. Effort: M. Test: REQ-043
policy-definition test plus the existing cross-tenant suite.

## Sequencing and gates (GUESS)

1. U1 first — it closes the only tenant-isolation hole; confirm zero drift before the FKs land.
2. U2 and U3 next, independent of each other; U3 needs the parity check on the emoji rule.
3. U4 last — it changes how *future* migrations are written, so land it once the earlier work is
   stable and the helper's test exists.
4. Non-goals this wave: no edits to `0009`, no dropping of `message_reactions_message_idx`,
   `thread_id` or `created_at`, no PK change, no tighter emoji validation. Each is a separate owner
   decision to record in `docs/STATE.md` / `docs/PARITY.md`.

For each step: `cargo fmt --check`, `cargo clippy -- -D warnings`, REQ-008/REQ-043 integration tests
against a real database, and the migration idempotency check (`boma migrate` twice) before commit.
Claims in `docs/STATE.md` stay GUESS until the owning agent actually runs these commands.
