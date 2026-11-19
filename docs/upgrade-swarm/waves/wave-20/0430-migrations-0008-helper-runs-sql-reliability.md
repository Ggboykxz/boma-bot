# Wave 20 / Worker 0430 — `migrations/0008_helper_runs.sql` (LENS: reliability)

Proposal for v0.4.0, additive only: every item adds a new numbered migration, column, index,
constraint or comment; nothing already applied is edited, dropped or destructively back-filled.

## Current state (CONFIRMED — read this session)

`0008_helper_runs.sql` is 11 lines. It adds one nullable self-FK column to `runs`
(`parent_run_id text REFERENCES runs (id) ON DELETE SET NULL`) and one partial index
`runs_parent_run_id_idx ON runs (parent_run_id) WHERE parent_run_id IS NOT NULL`.
There is no `CHECK`, no tenant/thread guard, no idempotency key, no outcome column and no
trigger invariant; the header's "the trigger marks it" is a comment, not a database rule.

Related surface read this session (CONFIRMED):
- `0002_m1.sql` creates `runs` (`space_id`, `thread_id`, `trigger`, `client_nonce`, ...) with
  ENABLE + FORCE RLS, policy `space_id = boma.current_space_id() OR boma.is_system()`, and a
  partial unique index `(thread_id, client_nonce)`.
- `crates/infra/src/db.rs` embeds migrations with `sqlx::migrate!`, forward-only.
- `crates/app/src/usecases.rs::delegate_helper_run` sets `trigger=HelperResult`,
  `client_nonce: None`, `parent_run_id=Some(parent.id)`. `delegate_bot_run` (REQ-010) also sets
  `parent_run_id`, with `trigger=BotMessage` and a *different* `bot_id` on the same space/thread.
- `complete_helper_run` uses RLS-scoped `find`, inserts a `role=tool` message, appends an event,
  then requeues a `WaitingInput` parent (CONFIRMED in `usecases.rs`, `stores/runs.rs`).
- PostgreSQL docs fetched this session: "Referential integrity checks ... always bypass row
  security" (CONFIRMED).

## Failure modes

### Constraints
1. **Cross-tenant / cross-thread parent.** The FK checks existence only and RI checks bypass RLS,
   so a child can reference a parent in another space. `complete_helper_run`'s RLS-scoped `find`
   then returns `None` and exits `Ok(false)`: the result message is written but the parent is
   never requeued and stays `WaitingInput` indefinitely (silent stuck run).
2. **Self-reference and cycles.** Nothing forbids `parent_run_id = id` or A→B→A. App-layer checks
   (REQ-010 refuses nested delegation) do not cover the reassignment path in ADR-031. A cycle
   makes completion/requeue behaviour undefined.
3. **No trigger invariant.** Any row may carry `parent_run_id` with the wrong trigger. For
   `helper_result` the claim query `trigger IN ('user','helper_result','retry')` (CONFIRMED) picks
   it up; a stray value makes it invisible to that path or surface as ordinary chat.
4. **Duplicate delegation.** `client_nonce: None` means the nonce unique index cannot dedupe a
   retried `delegate.helper` call. Two helper runs report twice, two `role=tool` messages land and
   the parent is requeued twice.
5. **Split completion.** Message insert, event append and parent status change are separate
   awaits, not one transaction. A crash between them yields a result with no requeue, or a
   requeue with no result.

### RLS
6. **FK bypass is a covert channel.** Another tenant's run id can be probed through FK errors
   even though reads return nothing. This is the design of `0002` (its header says so), but it
   means the FK is not a tenant boundary — only app code and RLS are.
7. **`ON DELETE SET NULL` under FORCE RLS.** The RI action runs with the owner's identity; with
   FORCE RLS and no `boma.space_id` set in that transaction, child rows may be filtered and the
   link left dangling — the same stuck-parent failure as (1). GUESS until a live test (H6) runs.

### Ordering
8. **Applied-migration checksum.** Editing `0008` in place breaks `sqlx` verification on deployed
   databases; all hardening must be new numbered files. App code writing a new column ships only
   after the migration is applied (two-phase deploy).
9. **Partial apply and locks.** `ADD COLUMN … REFERENCES` and a non-concurrent `CREATE INDEX`
   take ACCESS EXCLUSIVE and block worker claims; `CREATE INDEX CONCURRENTLY` cannot run inside
   sqlx's migration transaction (GUESS — verify against the vendored sqlx version). A half-applied
   migration re-run fails on duplicate column/index.

## Concrete hardening (additive only)

### H1 — Tenant-safe parent (closes 1, 6)
New migration: `CREATE UNIQUE INDEX runs_id_space_id_key ON runs (id, space_id)` and a second,
`NOT VALID` composite FK `(parent_run_id, space_id) REFERENCES runs (id, space_id) ON DELETE SET NULL`,
validated in a later migration after an audit query. Keep the original FK intact.
- **Risk:** medium. The redundant unique index adds one index write per run insert; a legitimately
  cross-space delegation would start failing (GUESS — REQ-010 shares the thread/space, verify
  before validating). RI still bypasses RLS: this closes integrity, not the covert channel.

### H2 — No self-parent (closes 2, partially)
`ALTER TABLE runs ADD CONSTRAINT runs_parent_not_self CHECK (parent_run_id IS NULL OR parent_run_id <> id) NOT VALID`,
then validate after `SELECT count(*) FROM runs WHERE parent_run_id = id` returns zero.
- **Risk:** low. Only self-links are stopped; longer cycles still need an app guard or a deferred
  constraint trigger (additive follow-up). If legacy self-links exist, log them — never delete.

### H3 — Delegation idempotency key (closes 4)
Add nullable `delegation_key text` plus partial unique index
`(parent_run_id, delegation_key) WHERE delegation_key IS NOT NULL`; the app derives the key from
parent id + tool-call id. Existing rows keep NULL and are unaffected.
- **Risk:** medium. Requires a coordinated app change in the same release; an unstable key would
  reject legitimate sequential helpers. Cover with a `req_019_*_delegate_is_idempotent` test that
  replays the tool call.

### H4 — Completion audit columns (closes 3, 5)
Add `helper_outcome text` and `helper_result_message_id text` with a unique index on the latter
where not null, plus `COMMENT ON` documenting that a row with `parent_run_id` carries
`helper_result` (REQ-019) or `bot_message` (REQ-010), and that outcome + requeue are not atomic.
A recovery query can then find helpers with `completed_at` set but no result message.
- **Risk:** low/medium. Duplicates the payload text; the trigger invariant stays app-enforced
  unless a `CHECK` is added later `NOT VALID` — do not add it until REQ-010's trigger is confirmed.

### H5 — Ordered, idempotent, low-lock migrations (closes 8, 9)
One change per numbered file; `SET lock_timeout` and guard statements with
`DO $$ … EXCEPTION WHEN duplicate_column / duplicate_table THEN NULL; END $$;`. Add a recovery
index `(status, created_at) WHERE parent_run_id IS NOT NULL` only together with H4's recovery query.
- **Risk:** low. Guards can swallow a genuine name collision — catch only those two duplicate
  SQLSTATEs. Non-concurrent index creation still blocks briefly; schedule at low traffic.

### H6 — Lock/RLS regression test (closes 7, 9)
Integration test, no schema change: delete a parent under a system context and assert every
child's `parent_run_id` is NULL; repeat with no tenant setting and assert the FK action still fires.
- **Risk:** none to data. If the second assertion fails, escalate before shipping H1/H4.

## Additive-only guarantee

No `DROP`, no type change, no destructive `UPDATE`, no edit of `0008_helper_runs.sql`. New
constraints arrive `NOT VALID` and are validated only after audit queries; new columns are
nullable/defaulted.

Verification: apply to a fresh DB and a seeded copy, assert existing rows unchanged, record audit
counts and H6's result in `docs/STATE.md`.

Status: proposal only — no file outside this report was modified.
