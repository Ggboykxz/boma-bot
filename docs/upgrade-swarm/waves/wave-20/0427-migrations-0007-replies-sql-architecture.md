# v0.4.0 architecture proposal — `migrations/0007_replies.sql`

Lens: architecture. Scope: REQ-008 replies/quotes schema. Worker #427.

Status: statements marked **CONFIRMED** were read from source this session; the upgrades and their
risks are **GUESS** proposals for the owning agent to validate. English. Additive only: no edits to
0007 itself, no DROP COLUMN / DROP CONSTRAINT anywhere.

## Current structure (CONFIRMED)

- The file is 8 lines: a header comment plus two `ALTER TABLE messages ADD COLUMN` statements —
  `reply_to_message_id text REFERENCES messages (id) ON DELETE SET NULL` and `reply_quote text`.
- `messages` (0002_m1.sql:168-188) already carries `space_id`, `thread_id`, `seq`, a unique
  `(thread_id, seq)` and a partial nonce index; it has RLS from its own migration. 0007 adds no
  RLS, index, CHECK or COMMENT — it is a pure payload extension.
- Migration path: `sqlx::migrate!("../../migrations")` embedded in the binary, forward-only and
  checksummed (`crates/infra/src/db.rs:3-5,20`). Editing 0007 in place would break `boma migrate`
  on any deployed database, so every change below belongs in a NEW numbered migration.
- Writer: `send_message` (`crates/app/src/usecases.rs:4308-4335`) copies the wire `Option<String>`,
  validates only the id shape (invalid → `NotFound`), and clones `reply_quote` verbatim.
- Readers: `crates/infra/src/stores/messages.rs:65-99,144-149,174-181` insert and return both
  columns; `rows.rs:106-107,339-340` map them to `MessageId`; `account_export.rs:123` copies both.
  No SQL anywhere filters, joins or orders by `reply_to_message_id` — it is carried, never queried.

## Architectural tensions

- T1 — the FK is too wide. It references `messages(id)` globally, not `(space_id, thread_id, id)`,
  so "reply to an earlier message in the same thread" is not a database invariant; the app does not
  check it either. With RLS, a cross-space id becomes a pointer the tenant cannot read — an
  existence oracle through FK-violation timing.
- T2 — the pair is not a state. Two independent nullable columns admit `(id, NULL)`,
  `(NULL, 'quote')`, self-reply and forward-in-time reply; only "both NULL for a non-reply" is
  documented, in a comment the database cannot enforce.
- T3 — two owners of the quote text. `reply_quote` is a client-supplied snapshot of another row's
  `blocks`; nothing ties them. Safe today (no edit path), but any future edit or redaction makes the
  quote diverge silently, including in account exports.
- T4 — no index. Harmless while nothing queries by target id; the first "replies to M" feature
  (notifications, counts, threading) becomes a sequential scan on a growing `messages` table.
- T5 — `reply_quote text` is unbounded and unchecked at the boundary, yet it is copied into every
  page and export, so a hostile client can amplify response and export size.
- T6 — the contract lives only in prose. Without `COMMENT ON COLUMN`, introspection and generated
  docs cannot discover the snapshot semantics that the header comment alone carries.

## Concrete upgrades (new migration files only)

### U1 — Scope the reference to the tenant and thread
Add `UNIQUE (space_id, thread_id, id)` on `messages`, then a self-FK
`(space_id, thread_id, reply_to_message_id) REFERENCES messages (space_id, thread_id, id) ON DELETE SET NULL`.
MATCH SIMPLE keeps NULL replies valid; the composite column list enforces "same space, same thread"
at the database boundary, closing T1. Keep the original 0007 FK (dropping it is not additive) and
document the deliberate redundancy.
*Risk:* every existing cross-thread or cross-space reply blocks validation — audit with a SELECT
first and agree a repair policy. Two overlapping FKs mean two checks on delete; measure hot-thread
delete cost. *Effort:* M. *Test:* REQ-008-named integration test asserting cross-thread and
cross-space targets are rejected while a same-thread target succeeds.

### U2 — Make the optional pair a total state
In a new migration add CHECKs: a quote implies a target; a target implies a non-empty quote;
target `<> id` (no self-reply). Use `NOT VALID` then `VALIDATE CONSTRAINT` so a large existing
table is not locked for a full scan at deploy time. A "target precedes reply" rule needs a subquery
or helper function — defer it until the spec actually demands it.
*Risk:* these reject writes the boundary accepts today — externally visible, so audit existing rows
and record a DEVIATION in `docs/PARITY.md` rather than shipping silently. The empty-quote rule must
match what the composer actually sends. *Effort:* S. *Test:* one REQ-008-named test per illegal
pair, plus a fresh-vs-upgraded migration path test.

### U3 — Bound and document the snapshot
Add `CHECK (char_length(reply_quote) <= N)` with N confirmed against the spec and the original as
oracle, plus `COMMENT ON COLUMN messages.reply_to_message_id / reply_quote` spelling out snapshot
ownership and the ON DELETE SET NULL behavior (0001_init.sql already shows COMMENT ON TABLE).
*Risk:* any finite N can reject text the original accepts — GUESS until checked against the oracle;
if it cannot be confirmed, enforce the cap only at the app boundary and ship the COMMENTs alone.
*Effort:* S. *Test:* boundary test at N and N+1 plus golden replay of an existing long quote.

### U4 — Add the reader's index, only when a reader exists
`CREATE INDEX CONCURRENTLY ... ON messages (reply_to_message_id) WHERE reply_to_message_id IS NOT NULL;`
A partial index skips the majority of non-reply rows.
*Risk:* `CONCURRENTLY` cannot run inside a transaction block — the SQLx migration needs the
`-- no-transaction` directive, or accept a short lock with a plain CREATE INDEX. Pure write cost
today since no query uses the column; stage it behind the first real reader feature. *Effort:* S.
*Test:* `EXPLAIN` on a seeded thread proving index use before and after.

### U5 — Pin deletion semantics at the DB boundary
No schema change: add a REQ-008-named integration test that deletes the target and asserts both
columns become NULL on the reply, that deleting the reply leaves the target intact, and that a
chain of replies settles correctly; add `COMMENT ON CONSTRAINT` for the existing FK's ON DELETE
action. *Risk:* SET NULL on a self-FK walks dependent rows; long chains may be slow on delete —
measure with a chain fixture before claiming it is free. *Effort:* S.

## Sequencing and gates

1. Audit the live schema and existing rows first; the T1/T2 audit decides whether U1/U2 can land.
2. U2 (cheapest invariant win) → U1 (bigger lock, needs the unique index) → U5 → U3 (parity
   decision) → U4 only when a real reader appears.
3. For each step: `cargo fmt --check`, `cargo clippy -- -D warnings`, REQ-008 integration tests
   against real Postgres, and `boma migrate` on both a fresh DB and a DB upgraded from 0007.
4. Every claim in `docs/STATE.md` stays GUESS until an owning agent actually runs the above.
