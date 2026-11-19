# Wave 20 / Worker 0428 — `migrations/0007_replies.sql` (LENS: reliability)

Proposal for v0.4.0. Additive only: every item below *adds* an index, constraint, trigger,
setting or comment; nothing already applied is dropped, rewritten or back-filled. Changes belong in
a new numbered migration (e.g. `0060_replies_reliability.sql`), not in `0007`, to preserve the sqlx
migration hash chain.

## Current shape (CONFIRMED — read this session)

`0007_replies.sql` is 8 lines: a header comment plus two `ALTER TABLE messages ADD COLUMN`
statements — `reply_to_message_id text REFERENCES messages (id) ON DELETE SET NULL` and
`reply_quote text`. No `UP`/`DOWN`, no `-- no-transaction`; migrations are embedded with
`sqlx::migrate!` (`crates/infra/src/db.rs:20`) and sqlx wraps each file in a transaction (library
behaviour, not re-verified here — GUESS), so the two statements are atomic.

Related surface (CONFIRMED):
- `messages` is defined in `0002_m1.sql:168-184`: `id text PK`, `space_id`, `thread_id`, `seq`,
  `role`, `blocks`, `bot_id`, `author_user_id`, `run_id`, `client_nonce`, `created_at`,
  `UNIQUE (thread_id, seq)` plus a partial unique index on `(thread_id, client_nonce)`.
- `messages` is a tenant table (`0002_m1.sql:264`) with `ENABLE`/`FORCE ROW LEVEL SECURITY` and
  `messages_tenant_isolation USING/WITH CHECK (space_id = boma.current_space_id() OR
  boma.is_system())` (`0002_m1.sql:270-277`). Helper functions in `boma` read the session settings.
- Only index touching ordering is `messages_thread_seq_idx (thread_id, seq DESC)` (`0002_m1.sql:292`);
  there is **no** index on `reply_to_message_id`.
- Spec: REQ-008 (`01-requirements.md:66-72`, "reply to an earlier message"), `Message` fields
  (`02-data-model.md:42`), `threads.send` input `replyToMessageId?/replyQuote?`
  (`03-api-contract.md:41-43`). The file comment itself says the target is "same thread".

## Failure modes

### Constraints
- **C1** No constraint binds the two columns. `reply_quote` may be non-NULL while
  `reply_to_message_id` is NULL — an orphan quote the client cannot anchor. The header states a
  non-reply has NULL for *both*, so the invariant is documented but unenforced.
- **C2** Nothing forbids `reply_to_message_id = id` (self-reply) or a target with `seq >=` the
  current message's `seq`, i.e. a reply pointing *forward*, contradicting REQ-008's "earlier".
- **C3** `reply_quote text` is unbounded. A hostile client can store a very large quote that is
  echoed to every `threads.subscribe` consumer and inflates messages pages and SSE payloads.
- **C4** The FK is unnamed, so its generated name (`messages_reply_to_message_id_fkey`) is the only
  handle for later tooling; no `ON UPDATE` action (default `NO ACTION`; low risk — ids are an
  immutable text PK).

### RLS
- **R1** PostgreSQL referential-integrity checks bypass RLS. A row in space A can therefore point
  at a message id in space B if the id is known or guessed: the *referencing* row is tenant-scoped,
  but the *reference* is not. Under member RLS a join cannot read the foreign row (it is filtered),
  yet a system path join can, and the in-tenant invariant is broken.
- **R2** `ON DELETE SET NULL` is also an RI action: it nulls `reply_to_message_id` but leaves
  `reply_quote` populated. Deleting a target converts a valid reply into exactly the orphan state
  from C1 — and would make a "quote requires target" `CHECK` cause the *delete* to fail.
- **R3** Any guard trigger is `SECURITY INVOKER` by default: under RLS its lookup of the target is
  filtered (helpful), but under a system path it sees every tenant, so checks must compare
  `thread_id`/`space_id` explicitly rather than relying on visibility.

### References
- **R4** PostgreSQL does not index the referencing side of an FK. Every message delete makes the
  RI action scan `messages` for rows pointing at it: O(n) per delete, latency spikes and lock
  contention on large threads. No "find replies to M" query can use an index either.
- **R5** Both `ALTER TABLE`s take `ACCESS EXCLUSIVE` on `messages`. The columns are nullable with no
  default, so this is a metadata-only change (no rewrite), but with no `lock_timeout` a deploy can
  wait indefinitely behind a long transaction.

## Concrete hardening (additive only)

1. **Partial index for the FK** — `CREATE INDEX messages_reply_to_message_id_idx ON messages
   (reply_to_message_id) WHERE reply_to_message_id IS NOT NULL;`. Fixes R4; mirrors the existing
   partial-index convention. *Risk*: low; one extra index write per reply insert/update.
2. **Guard trigger** `boma.messages_reply_guard()` (BEFORE INSERT OR UPDATE ON messages,
   `SECURITY INVOKER`): when `reply_to_message_id IS NOT NULL`, select the target and reject unless
   `target.thread_id = NEW.thread_id AND target.space_id = NEW.space_id AND target.seq < NEW.seq`.
   Fixes C2 and R1/R3. *Risk*: medium — one extra lookup per reply; same-transaction targets are
   visible to their own inserter, so batch writes still work; do not rely on RLS visibility on
   system paths. A target deleted before an old row is re-updated leaves the pointer NULL — skip.
3. **Named `CHECK`s, `NOT VALID` then `VALIDATE`** — `messages_reply_no_self CHECK
   (reply_to_message_id IS NULL OR reply_to_message_id <> id)` and `messages_reply_quote_requires_target
   CHECK (reply_quote IS NULL OR reply_to_message_id IS NOT NULL)`. Fixes C1/C2. *Risk*: medium —
   only validate after an audit query shows zero offenders; ship item 4 in the same migration or
   deletes will start failing (R2).
4. **Delete companion trigger** on messages: for rows whose `reply_to_message_id = OLD.id`, set
   `reply_quote = NULL`, leaving the existing FK `SET NULL` to clear the pointer. Preserves current
   delete semantics while satisfying item 3. *Risk*: medium — extra UPDATEs on bulk delete;
   RLS-visible on member paths (same-tenant rows only), explicit space check on system paths.
5. **Length bound** `CHECK (char_length(reply_quote) <= N)`, with `N` taken from the original's
   boundary validation — value **GUESS**, unconfirmed. *Risk*: **DEVIATION** — it can reject input
   the original accepts. Proposal only; do not ship in v0.4.0 without sign-off in `docs/PARITY.md`.
6. **Deploy safety / documentation** — add `SET LOCAL lock_timeout` (and `statement_timeout`) at the
   top of the new migration to fail fast instead of queueing (R5); add `COMMENT ON COLUMN` for the
   two columns restating REQ-008 and the same-thread/earlier invariant. A stable FK constraint name
   would need `RENAME CONSTRAINT`, which is not additive — document the generated name instead.

## Additive-only guarantee & verification

No `DROP`, no column rewrite, no `UPDATE` of existing rows, no change to `ON DELETE SET NULL`.
Items 1/2/3/4 tighten writes: any client relying on cross-thread replies, self-replies or orphan
quotes would see new errors — record as a DEVIATION in `docs/PARITY.md` if observed.

- Apply migrations twice on a fresh database and assert idempotency
  (`crates/infra/tests/database.rs:38-56`).
- REQ-008 integration tests in `crates/infra/tests/m1.rs`: same-thread reply accepted; cross-thread,
  cross-space, self and forward-seq targets rejected; deleting a target NULLs both columns;
  guard behaviour checked as member and as system path.
- Audit query for orphans/self-replies before `VALIDATE CONSTRAINT`; EXPLAIN on a seeded delete to
  confirm the partial index is used. Mark results CONFIRMED in `docs/STATE.md`.

Status: proposal only — no file outside this report was modified.
