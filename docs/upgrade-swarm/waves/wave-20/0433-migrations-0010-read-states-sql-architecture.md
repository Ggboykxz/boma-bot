# 0010_read_states.sql — architecture upgrade proposal (v0.4.0)

Worker #433 · lens: architecture · REQ-009
Scope: ADDITIVE ONLY — each upgrade below is a new statement (`ALTER COLUMN ... SET
DEFAULT`, `ADD CONSTRAINT`, `CREATE INDEX`, `CREATE TRIGGER`, `COMMENT ON`). Nothing in
0010 is rewritten or dropped; sibling migrations are untouched.
Evidence status: CONFIRMED for claims read from the file/tree this session; GUESS where
inferred from surrounding conventions.

## Current structure (CONFIRMED)

- One row per `(thread_id, user_id)`; `last_seq bigint DEFAULT 0 CHECK (>= 0)` is the high-water
  read mark; `space_id` is the RLS tenant key; `updated_at timestamptz NOT NULL` has **no default**.
- RLS is enabled, FORCED, and guarded by `<table>_tenant_isolation` with
  `space_id = boma.current_space_id() OR boma.is_system()`; the same `DO $$ FOREACH` boilerplate
  is repeated verbatim in 0009 and 0011.
- Writers (CONFIRMED: `crates/infra/src/stores/read_states.rs`): `position()` selects by PK prefix;
  `set()` upserts with `INSERT ... SELECT ... WHERE EXISTS (thread in caller space)` and supplies
  `now()` for `updated_at`; the roster counts `m.seq > COALESCE(p.last_seq, 0)` and excludes
  `role='tool'`.
- `messages.seq` is thread-local, allocated from `threads.next_message_seq` (0002, CONFIRMED).
- Semantic tensions: absent row and `last_seq = 0` mean the same thing; `space_id` is trusted from
  the writer, with no DB guarantee it matches the referenced thread; `last_seq` is not bounded by
  the thread's real high-water mark.

## Proposed upgrades (each independently additive)

### U1 — Give `updated_at` a database-level default
`ALTER TABLE thread_read_states ALTER COLUMN updated_at SET DEFAULT now();`
Rationale: closes the "every writer must remember now()" invariant; today a bare `INSERT` from a
backfill, fixture, or future writer fails against `NOT NULL`. Aligns the column with `created_at`
usage in sibling tables (GUESS: they also omit defaults, so a follow-up sweep is worth scoping).
Risk: LOW. No existing row is NULL, so no rewrite. Do **not** add an auto-touch trigger in v0.4.0:
it would silently change `updated_at` on update paths that intentionally set it, which is a behavior
change outside additive scope. Record the trigger idea as a follow-up instead.

### U2 — Enforce the read-mark invariant at the data layer
Add an additive `BEFORE INSERT OR UPDATE` trigger that rejects `NEW.last_seq` greater than the
thread's authoritative high-water mark (`threads.next_message_seq - 1`), always allowing `0`
(markUnread). Use `next_message_seq`, not `MAX(messages.seq)`: messages are deleted on cascade but
the counter is monotonic, so the check survives history pruning.
Rationale: a client sending a future `last_seq` currently zeroes the unread badge and pushes the
divider past the end; the CHECK only rejects negatives. This is defense in depth behind the Rust
boundary check (CONFIRMED: `last_seq < 0` is rejected in the store).
Risk: MEDIUM. (a) Reject vs clamp: reject with a clear SQLSTATE and let the caller retry after a
fresh `threads.head`; clamping would hide a real client bug. (b) A `markRead` racing with a send
can see a stale head — the app must tolerate a retried rejection. (c) Trigger adds one indexed PK
lookup per write. Benchmark before/after; keep the trigger `SECURITY DEFINER` off and let RLS apply.

### U3 — Make the tenant key structurally consistent with the thread
Add `ALTER TABLE threads ADD CONSTRAINT threads_id_space_key UNIQUE (id, space_id);` then
`ALTER TABLE thread_read_states ADD CONSTRAINT thread_read_states_thread_space_fkey
FOREIGN KEY (thread_id, space_id) REFERENCES threads (id, space_id) ON DELETE CASCADE;`
Rationale: today `space_id` is only correct because the writer's `WHERE EXISTS` says so; RLS can
make a row with a drifted `space_id` invisible or misrouted, and the FK to `threads(id)` alone does
not catch the mismatch. This is the "defense in fact" principle from the store made durable.
Risk: MEDIUM. Requires a pre-flight validation query (`SELECT ... WHERE trs.space_id <> t.space_id`)
and a backfill decision before the FK lands; any bad row blocks the migration. The `ON DELETE
CASCADE` is redundant with the existing single-column FKs but harmless. Confirm no other migration
already adds a similar unique constraint before adding one (GUESS: none found in 0001–0059 scan).

### U4 — Index the user/space access path
`CREATE INDEX thread_read_states_space_user_idx ON thread_read_states (space_id, user_id);`
Rationale: the PK `(thread_id, user_id)` serves `position()` and the roster join well, but not
user-centric or space-scoped scans (account export, user deletion, reset-all-in-space), which are
currently full scans. Leading with `space_id` also matches the RLS predicate.
Risk: LOW — extra storage and one more index to maintain on every upsert. Skip if no such
user-centric query exists yet; confirm with `EXPLAIN` on the real access paths first.

### U5 — Document the data contract in the catalog
`COMMENT ON TABLE thread_read_states IS 'REQ-009 ... per-member read mark; no row == 0; '
'space_id is the RLS tenant key; boma.is_system() bypass is intentional';` plus `COMMENT ON
COLUMN ... last_seq IS 'highest seq read; divider sits after this; 0 means nothing read'` and the
same for `updated_at`/`space_id`.
Rationale: the non-obvious semantics (absent row == 0, divider placement, system bypass) live only
in a comment and in Rust code. Catalog metadata makes the contract discoverable to SQLx offline
metadata users and future migrations without changing behavior.
Risk: NONE at runtime. Purely descriptive; keep wording in English per project convention.

## Explicitly out of scope (would break "additive only")

- Rewriting the `DO $$ FOREACH ... ARRAY['thread_read_states']` block into a shared
  `boma.enable_tenant_rls()` helper — the right long-term fix for the ~30x duplicated policy
  (0009/0010/0011 CONFIRMED), but it implies `DROP POLICY` across many migrations.
- Collapsing "no row" and "`last_seq = 0`" into one canonical representation, or narrowing the
  `boma.is_system()` bypass: both change observable behavior.
- Adding a membership check for group chats (REQ-010): the write path currently verifies only
  that the thread is in the caller's space, not that the caller is a member (GUESS: worth a
  dedicated REQ-010 review).

## Verification gate (for the implementing agent)

1. `psql -f 0010...` plus the new statements on an empty DB and on a seeded DB; assert idempotent
   re-run where possible. 2. Pre-flight U3 drift query returns 0 rows. 3. `EXPLAIN` the roster
   query and `position()` before/after U4. 4. Replay the `REQ-009-unread-divider` golden and the
   REQ-009 Playwright tests; add a store test asserting a future `last_seq` is rejected (U2).
5. `cargo sqlx prepare --check` (offline metadata still valid) and the migration test harness.
