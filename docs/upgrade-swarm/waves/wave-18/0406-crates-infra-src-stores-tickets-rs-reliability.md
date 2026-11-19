# v0.4.0 reliability proposal — `crates/infra/src/stores/tickets.rs`

Module: `PostgresTicketStore` (REQ-038 ticket board). Lens: reliability.

Evidence (read this session): `tickets.rs`, `tenant.rs`,
`migrations/0038_tickets.sql`, `crates/domain/src/ticket.rs`,
`crates/app/src/ports.rs` (3176–3300), `crates/app/src/usecases.rs` (1827–1929),
`crates/api/src/rpc.rs` (2795–2883). CONFIRMED = read from source; GUESS = runtime inference.

## Framing

The three named failure modes have no literal token/TTL analogue here. Faithful
mapping: **double redemption** = a retry applying the same intent twice;
**expiry** = clock ownership and staleness of the board view; **state races** =
concurrent moves/claims and the denormalised comment counter. All three are live.

## Failure modes

### Double redemption — duplicate writes on retry (CONFIRMED)

- `tickets_create` (usecases.rs:1854) and `tickets_comment` (usecases.rs:1917) mint a
  *fresh* id per call; the RPC requests (`rpc.rs:2796`, `rpc.rs:2859`) carry no
  idempotency token. A retry after a lost response creates a second ticket
  (burning a second `BOT-<n>` key) or a second comment (bumping the counter twice).
- `tenant::unavailable` maps *every* `sqlx::Error` — unique, FK, CHECK violations —
  to `AppError::Unavailable` (tenant.rs:153). A deterministic conflict looks like
  an outage and invites more retries; `tenant::is_unique_violation` is unused here.
- Key allocation (`create`, 102–111) is `INSERT … ON CONFLICT DO UPDATE …` inside
  the tenant transaction: race-free at READ COMMITTED, but concurrent creators
  abort with a serialisation error if isolation is raised to REPEATABLE READ or
  SERIALIZABLE (GUESS — deployment config not read). `next_seq` is `i32`, unguarded
  against 2^31 tickets/space.
- `move_to` to the *same* state still rewrites `updated_at = now()` (line 156):
  spurious "last changed" churn for any consumer polling that column.

### Expiry — clock provenance and stale reads (CONFIRMED)

- `created_at` is supplier-supplied (`$…::timestamptz`, lines 126 and 195) while
  `updated_at` is database `now()` on move/comment (156, 200). Two clock sources:
  a skewed/forged future `created_at` makes `updated_at` appear earlier. The
  canonical usecase uses `clock.now()`, but the public `TicketStore` trait is
  callable from system paths that bypass it.
- No TTL, archive marker or retention sweep: a `done` ticket stays in `list()`
  forever (CONFIRMED). Whether the spec wants expiry is unknown — GUESS; do not
  invent a field without a decision.
- `tickets_move` calls `get()` in a **separate transaction** (usecases.rs:1886):
  a concurrent move can commit between them, so the reply can report a state
  other than the one this call set (read-after-write race).
- No index on timestamps; `list()` orders by text `key`, so `BOT-10` sorts before
  `BOT-2` past nine tickets (separate UX defect, out of scope here).

### State races — blind writes and TOCTOU (CONFIRMED)

- `move_to` is `UPDATE … SET state = $2 WHERE id = $1`: no expected-state guard,
  no version/fence, no transition rule. Two concurrent moves both return
  `Ok(true)`; last writer wins silently. Two workers can both claim a ticket and
  `assignee_bot_id` is never touched, so the claim is unrecorded. `done → backlog`
  is accepted — the DB CHECK constrains the vocabulary, not the transition; the
  usecase (usecases.rs:1877) parses the vocabulary but adds no transition check.
- `comment` does SELECT-existence, INSERT, then `comment_count + 1` (176–205) in
  one transaction: no orphan can survive, but a delete committing between SELECT
  and INSERT becomes an FK error mapped to `Unavailable` (GUESS).
- `comment_count` has no reconciliation path in this store; any future deletion
  or out-of-band insert drifts it (GUESS — no such API today).
- `create` inserts `ticket.state` verbatim; a non-canonical invalid state yields
  `Unavailable` from the CHECK (migrations/0038:24) instead of a validation error.

## Concrete hardening (value / risk)

1. **Fencing for moves** (high/medium). Add `version bigint DEFAULT 1` and an
   `expected_version` on `move_to`; use `UPDATE … WHERE id=$1 AND version=$3` and
   return `false` on mismatch; add `AND state <> $2` to kill no-op churn. *Risk:*
   port + migration; externally visible conflict = **DEVIATION** needing approval.
   Cheaper fallback: keep the signature, guard with a state read by the usecase.
2. **Idempotency for create/comment** (high/medium). Thread a token through the
   RPC request and derive ids from it, or store an `(actor, token)` unique record;
   a replay returns the original row. *Risk:* RPC shape change = DEVIATION, plus
   migration and retention decision. Minimal variant: treat caller-supplied
   `ticket.id`/`comment.id` as the key and return the existing row on conflict.
3. **Typed conflict errors** (high/low). Use `tenant::is_unique_violation` in
   `create`/`comment` and map to a typed `AppError::Conflict`; reserve
   `Unavailable` for real I/O. *Risk:* error-code change — record as DEVIATION if
   the API contract fixes the current shape.
4. **Single clock** (medium/low). Bind `now()` for `created_at` too, or reject
   `created_at > now()`. *Risk:* timestamp semantics change; differential test
   against the original before claiming parity.
5. **Collapse read-after-write** (medium/low). Return the `TicketSummary` from
   `move_to`'s own transaction (`RETURNING` / `SELECT … FOR UPDATE`), removing the
   second `get()`. *Risk:* port change inside `app`, no wire change.
6. **Counter integrity** (medium/low). `ON CONFLICT (id) DO NOTHING` on the comment
   insert, increment only on a real insert, and add a `COUNT(*)` reconciliation
   test/job. *Risk:* migration only if the column is dropped.
7. **Counter durability** (low/low). `next_seq` to `bigint`; document the
   isolation assumption; integration test with concurrent creates asserting
   unique, gapless keys. *Risk:* migration, no wire change.

## Verification for v0.4.0

- Real-Postgres integration tests with REQ ids in names: concurrent claim converges
  to one winner; replayed ids do not duplicate; invalid state is refused directly
  at the store; a forged future `created_at` cannot invert timestamp order;
  concurrent creates allocate distinct keys.
- Property test: for any interleaving of moves over `TicketState::ALL`, the stored
  state stays in the vocabulary and `comment_count` equals the comment row count.
- Differential oracle against the original for reply shape before shipping 2–4;
  record any difference in `docs/PARITY.md`.

## Decisions needed

- Is expiry/archive in the spec? If not, do not add it.
- Items 1 and 2 change externally visible behaviour: approve before shipping.
  Until then items 3–7 are the in-parity subset.
