# v0.4.0 architecture proposal — `crates/infra/src/stores/messages.rs` (messages store / REQ-001)

Worker #357 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no code changed).

Status of claims: every "current state" statement below is CONFIRMED by direct reads of
`messages.rs`, `rows.rs`, `ports.rs`, `tenant.rs`, `events.rs`, `usecases.rs` and `0002_m1.sql` in
this worktree. Forward-looking items are proposals (GUESS until implemented, tested and measured).

## Current structure (CONFIRMED)

- `PostgresMessageStore` holds only a `PgPool` (`messages.rs:23-33`). Unlike `PostgresEventStore`
  (`events.rs:18-32`) it has no `IdGenerator`: the id arrives in `NewMessage.id`, generated in the
  use case (`usecases.rs:4322`). The store owns the *sequence* (REQ-005) but not the *identity*;
  the idempotency key is the partial unique index `(thread_id, client_nonce)` (REQ-002), not the PK.
- `insert` (`:37-132`) runs one `with_tenant` transaction: it increments `threads.next_message_seq`
  with `UPDATE ... RETURNING`, then inserts the row. Any unique violation maps to
  `AppError::Conflict` (`:103`), and the caller performs a *second* transaction `find_by_nonce`
  (`:121`) to turn the replay into `StoredMessage { created: false }`.
- The same 12-column projection is written three times verbatim (`:71-73`, `:145-147`,
  `:175-177`). `count_above` (`:224-227`) adds an explicit `space_id` predicate as "defense in fact";
  the other four methods rely on RLS alone.
- `page` (`:163-209`) is a keyset scan (`seq < before ORDER BY seq DESC LIMIT`) and emits a cursor
  whenever `len == limit`, even when no older rows exist.
- `set_text` (`:240-261`) hardcodes `[{"type":"text","text":...}]` in the store, updates by id only,
  ignores `rows_affected` (a no-op returns `Ok(())`) and relies on RLS for tenant scoping.
- REQ-001's "one message and one run" is a *use-case* invariant across two transactions with two
  independent nonce keys: `send_message` inserts the message (`usecases.rs:4316-4338`) then calls
  `create_or_steer_runs` (`:4366`). A failure between the two leaves a message with `run_id = NULL`,
  which the schema explicitly permits (`0002_m1.sql:177-178`).

## Guiding principle for v0.4.0

Keep the nonce-based contract byte-for-byte (REQ-001/002/005 are frozen) but make the store's
internal structure explicit: one definition per SQL shape, one classification per failure, one
consistent tenant posture, and an honest representation of the cross-port atomicity seam. Any
externally visible change is a DEVIATION to record in `docs/PARITY.md` before shipping.

## Upgrades (5 concrete items)

1. **Define the message projection once.** A `const MESSAGE_COLUMNS: &str` consumed by `insert`'s
   `RETURNING`, `find_by_nonce` and `page`. All three use runtime `sqlx::query_as`, so a `&str`
   constant suffices — no macro offline-metadata problem. Add one round-trip test asserting every
   field survives, including `reply_quote`, `client_nonce` and `boma.rfc3339(created_at)`.
   *Risk:* all columns decode to `String`/`Json`, so a reordered or truncated list can silently
   swap values instead of failing. *Mitigation:* distinct sentinel values per field in the test and
   a byte diff of the emitted SQL against today's three strings before merging.

2. **Classify conflicts instead of mapping every unique violation to `Conflict`.** Distinguish a PK
   collision on `id` from the `(thread_id, client_nonce)` replay; only the latter is a replay. For
   the former, look up by id or return a typed error rather than logging "a message conflicted but
   no message carries that nonce" (`:118-123`) and returning `Unavailable`.
   *Risk:* error mapping is wire-visible; tightening it could change a status code and become a
   DEVIATION. *Mitigation:* leave the nonce path untouched, add a regression test for a fresh nonce
   with a colliding id, and replay the REQ-001/002 goldens.

3. **Shrink the thread row-lock window; consider one-statement allocation.** Today
   `serde_json::to_value(&message.blocks)` (`:60-63`) runs *after* the `UPDATE threads` (`:51-58`),
   holding the serialization point across CPU work and an `.await`. Move all argument preparation
   before the `UPDATE`; optionally fold both into a single `WITH next AS (UPDATE ... RETURNING seq)
   INSERT ... SELECT` statement.
   *Risk:* a CTE must still roll back the increment on replay (no burned sequence), and SQLx type
   inference can make the query harder to maintain. *Mitigation:* keep the two-statement form in
   v0.4.0 if the CTE fights the binder; gate either choice on `tests/concurrency.rs` (`req_005` /
   `req_002`) plus a sends-per-second benchmark on one thread.

4. **One tenant posture for all five methods.** Add the explicit `space_id = context.space_id()`
   predicate used by `count_above` to `find_by_nonce`, `page` and `set_text`, mirroring the RLS
   policy's `OR boma.is_system()` so system paths keep working; document in `ports.rs` why
   `count_above` rejects a System context with `NotFound` (`:218`).
   *Risk:* a predicate that forgets the system branch hides messages from export/audit paths, and
   adding a predicate to a hot SELECT can change plan choice. *Mitigation:* Member *and* System
   integration tests per method, plus `EXPLAIN` on the page query with the existing
   `messages_thread_seq_idx`.

5. **Make `set_text` observable and domain-driven.** Replace the inline `json!` block with the same
   domain serializer `insert` uses, scope by `space_id`/`thread_id`, return `NotFound` when
   `rows_affected == 0`, and propose an additive `replace_blocks` port so a streamed update need not
   encode the block wire shape or silently drop attachments.
   *Risk:* rejecting a no-op and preserving non-text blocks changes streamed-reply behavior →
   DEVIATION. *Mitigation:* keep single-text-block semantics in v0.4.0, add the `NotFound` test, and
   carry `replace_blocks` as a separate ADR instead of a silent behavior change.

## Cross-cutting risks / sequencing

- Sequence 1 → 2 → 3 → 4 → 5: lock the projection and the failure taxonomy with tests first, then
  touch the transaction body, then tenant scoping, then `set_text`.
- The REQ-001 seam (message and run in two transactions) is deliberately *not* collapsed here;
  propose an outbox / single-send port in an ADR and add an orphan-window integration test so the
  limitation is measured, not hidden.
- Gate every step with `cargo fmt --check`, `clippy -D warnings`, `tests/` integration, golden and
  trace checks; no new dependency is needed. Each wire-visible delta goes to `docs/PARITY.md`.

## Out of scope for this module

Run creation and steering (`runs.rs`), thread/event sequence allocation (`threads.rs`, `events.rs`),
row→domain conversion (`rows.rs`) and HTTP DTOs (`wire.rs`) belong to other workers.
