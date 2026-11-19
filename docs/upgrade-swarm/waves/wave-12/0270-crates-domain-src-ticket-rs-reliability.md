# 0270 — `crates/domain/src/ticket.rs` — reliability proposal (v0.4.0)

Worker #270 · MODULE `crates/domain/src/ticket.rs` · LENS reliability · READ-ONLY review of
`rebuild-workspace/.worktrees/before-m5` (v0.3.0-era source). Proposals only; no code changed.
**CONFIRMED** = read this session; **GUESS** = inference needing a test.

Scope: this 139-line pure-domain module (REQ-038) — `TICKET_TITLE_MAX=200`,
`TICKET_DESCRIPTION_MAX=4_000`, `TICKET_COMMENT_MAX=10_000`; flat `TicketState`
(`Backlog|Todo|InProgress|Done`) with `as_str`/`parse`/`ALL`; `validate_ticket_title`;
`validate_ticket_text`. Consumers: `domain/src/lib.rs:109-110`, `app/src/usecases.rs:1827-1928`
(`tickets_create`, `_move`, `_comment`), `infra/src/stores/tickets.rs:86-185`. Anchors: REQ-038
(feature-flagged, spec/01-requirements.md:311-314), spec/02-data-model.md:122, spec/08-ui-ux.md:138-144.

## Already right (keep)

- Pure domain: no IO, async, clock or store; `TicketState` is `Copy` with stable wire values and a
  round-trip test over `ALL` (CONFIRMED:113-122). Validators are `Result`-typed via
  `DomainError::InvalidName`, reject empty/whitespace titles, cap char counts, reject control chars
  except `\n`/`\t` (CONFIRMED:63-106). `BOT-<n>` keys come from `INSERT ... ON CONFLICT ...
  RETURNING` in one transaction, unique under concurrency (CONFIRMED, stores/tickets.rs:101-112).

## Failure modes

### FM-1 — invalid transitions unenforced (high)

The module models four states but **no transition relation**: no `can_move`, no edge table;
`parse` checks vocabulary membership only (CONFIRMED:45-55). `ALL` is a flat array, board order
by convention only (CONFIRMED:38). The app parses a target *string* into the store's unconditional
`UPDATE tickets SET state=$2 WHERE id=$1` (CONFIRMED, usecases.rs:1871-1889,
stores/tickets.rs:146-166). So any→any is accepted, including `Done → Backlog` and no-op self
moves; the spec's board order is a UI convention the domain cannot keep (CONFIRMED mechanism,
GUESS on observed misuse). `move_to` is last-writer-wins with no version or expected-source
guard, so concurrent moves overwrite silently (GUESS on timing; unguarded UPDATE CONFIRMED).
`tickets_move` reads back in a *separate* query (usecases.rs:1886-1890): a concurrent move in
between returns a state this call never wrote — a TOCTOU window (CONFIRMED shape).

### FM-2 — expiry / staleness has no domain concept (medium)

No `expires_at`, lease or `updated_at` rule exists here, and the spec defines no ticket TTL
(CONFIRMED:311-314). This is **not** a missing feature to invent: the gap is that assignment
"records intent" and queues background work (usecases.rs:1819-1821, spec/08-ui-ux.md:140-142) with
no staleness token, so enqueued work can outlive a reassignment or a move back to `Backlog` and
still land on the board (GUESS — runner-dependent). An indefinite `InProgress` ticket cannot be
distinguished from an active one (CONFIRMED absence). Expiry needs a spec decision; see Non-goals.

### FM-3 — duplicate "redemption" on retry / no idempotency (medium-high)

No request/idempotency key reaches `tickets_create`, `_move` or `_comment` (CONFIRMED). Each call
allocates a fresh id (`tkt`/`tcm`, usecases.rs:1854, :1917), so a retry after a dropped response
creates duplicate rows even though the key counter is atomic (CONFIRMED mechanism, GUESS on a real
retry). This is the board's duplicate-redemption analogue: "at most once per user action" is not
representable. Denormalised `comment_count` starts at `0` (stores/tickets.rs:114-116) and is
maintained separately in `comment`, so it can drift under partial failure (GUESS — needs a test).
`move_to` reports `rows_affected > 0` only, so a replay is indistinguishable from a first write
and always bumps `updated_at` (CONFIRMED).

### FM-4 — boundary validation gaps and reflected error text (medium)

`TicketState::parse` is case-sensitive and does not trim (CONFIRMED:45-55), so `" in_progress"`
and `"IN_PROGRESS"` are refused (acceptable, but the error is raw). `parse` and both validators
embed the rejected value or `what` into `DomainError::InvalidName` via `format!` (CONFIRMED:51-53,
91-105); that reason is echoed verbatim by its Display (as in proposal 0226) — a log-injection /
reflected-text path for crafted titles and state strings (CONFIRMED data flow, GUESS on a sink).
Length uses `chars().count()` (scalar values, not grapheme clusters) and can disagree with any
byte limit at the HTTP/DB boundary (CONFIRMED mechanism). Empty *description* is allowed here
while empty *comment* is rejected one layer up (usecases.rs:1907-1911): a split rule a second
caller could miss (GUESS).

## Concrete hardening (v0.4.0, small steps)

1. **P1 — `can_move(from, to)` in domain.** `const fn` allow-list (forward edges + explicit reopens
   + self edges); call it in `tickets_move` before the write. No wire change. Risk R1.
2. **P2 — guard the write.** Add `move_to_if(context, id, from, to)`: `UPDATE ... WHERE id=$1 AND
   state=$2 ... RETURNING state`, removing the read-back. Closes FM-1 TOCTOU/last-writer-wins. R2.
3. **P3 — optimistic version.** Precondition on `version`/`updated_at`; typed conflict mapped to a
   stable API error. Defer if P2's source guard suffices. R2.
4. **P4 — idempotency key port.** Optional key in create/comment/move persisted as
   `(space_id, key, operation) -> result_id` with a unique constraint; replay on retry. Bound and
   keep secret-free. R3.
5. **P5 — sanitise errors.** Stop echoing raw input: cap/escape control chars, or return a stable
   classification (`InvalidName("too_long")`) and keep the raw value out of logs/wire. R4.
6. **P6 — unify emptiness.** Move the "comment must not be empty" rule into a domain
   `validate_ticket_body(..., allow_empty)`; document that description may be empty. R4.
7. **P7 — measurement contract.** Keep scalar-value count but assert the HTTP/DB byte limit is at
   least `4 * max`; add `proptest` for both validators (never panic, accept iff grammar holds). R5.
8. **P8 — reconciliation test.** Assert `comment_count` equals real comment rows after a fault-
   injected partial `comment` failure; recompute from the table or drop the column. R6.

## Risks / trade-offs

- R1. Allowed edges reject previously accepted moves — externally visible; record restrictions in
  `docs/PARITY.md` with approval. Default to forward + explicit reopen edges.
- R2. P2/P3 change an app port and both stores (Postgres + testkit), plus a P3 migration. Keep the
  RPC error body byte-identical.
- R3. P4 adds a table and changes endpoints; the spec has no explicit retry need — list it in
  `docs/STATE.md` first. Never reuse a security token as the key.
- R4. P5/P6 change error text and rule location; update fixtures and test that a newline in a title
  cannot forge a log line.
- R5. `chars().count()` is already O(n); cap body size at the HTTP boundary.
- R6. P8 may need a column rewrite; ship as a consistency/metric check first. English only; keep
  `cargo fmt --check`, `clippy -D warnings` and REQ-038 tests green each step.

## Tests (traceable names)

`req_038_state_can_move_allows_forward_edges`, `req_038_state_can_move_rejects_illegal_jump`,
`req_038_move_to_if_rejects_stale_source`, `req_038_move_is_idempotent_under_replay`,
`req_038_create_is_idempotent_under_retry`, `req_038_validation_error_never_reflects_input`, `req_038_empty_comment_rejected_in_domain`, `proptest` for both validators (P7).

## Non-goals

No ticket TTL/expiry, lease or auto-archive (spec silent — needs a `docs/STATE.md` decision); no
new states or wire values; no change to the `BOT-<n>` format; no background-run queue redesign
(FM-2 reported, not fixed); no weakening of tenant scoping or the feature flag.

## Recommendation

Wave A (domain-only): P5, P6, P7, P8-tests + P1's `can_move` and tests. Wave B (cross-crate): P2
with a concurrent-move fault test, then P4 after a STATE.md decision. Defer P3 (migration) and all
expiry work. Facts CONFIRMED by reading; GUESS impact needs the tests above. No commands run.
