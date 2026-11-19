# Worker #334 — `crates/infra/src/stores/cards.rs` — LENS: reliability — v0.4.0 proposal

- **Status:** PROPOSAL (read-only review; no source changed). CONFIRMED = verified by reading `cards.rs`, `rows.rs`, `tenant.rs`, `migrations/0004_cards.sql`, `crates/app/src/ports.rs`, `crates/app/src/usecases.rs` (`answer_card`, `apply_pending_tool_calls`) and `spec/01-requirements.md` in this worktree; GUESS = needs a test/oracle run.
- **Scope:** Postgres adapters for `CardStore` (`list_pending`, `find`, `create`, `answer`) and `EffectStore` (`record`), the exactly-once effect ledger (REQ-007, REQ-017, REQ-018).

## Summary

The individual SQL statements are correct in isolation — the conditional `UPDATE ... WHERE status='pending'` and the `(thread_id, run_id)` partial unique index plus the `external_effects` primary key give row-level and key-level idempotency. Reliability breaks at the *seams*: the multi-step `answer_card` flow spans several independent `with_tenant` transactions with no shared commit, the dedup index stops at `run_id IS NULL`, and pending cards have no lifecycle (no expiry, no cleanup on run deletion). Double-approval at the card row is guarded; double-*effect* and stuck-state outcomes are not.

## Failure modes

### Double-approval races (R)

- **R1 (LOW, CONFIRMED):** two concurrent `answer` calls on the same card are safe — `UPDATE ... WHERE id=$1 AND status='pending'` (133–142) is atomic; the loser gets `None` → `Conflict` (151). No `SELECT ... FOR UPDATE` needed. The pre-check in `answer_card` (`usecases.rs:7666–7675`) is a TOCTOU stale read, but it fails closed.
- **R2 (HIGH, CONFIRMED):** the dedup guard `cards_thread_run_key` is `WHERE run_id IS NOT NULL AND status='pending'` (migration 0004:19–21). Two concurrent `create` calls for a card with `run_id = None` both succeed (no unique violation → no `Conflict`). If such a card is an approval, each gets its own effect key `approve:<card_id>` (`usecases.rs:7686`) and both tool applications can execute — a real double-execution vector.
- **R3 (MED, GUESS):** `create` maps *any* unique violation to `Conflict` (110–113), including a future PK/collision on a different constraint. A replay and a genuine id are indistinguishable to the caller.

### Atomicity (A)

- **A1 (HIGH, CONFIRMED):** `answer_card` is not one transaction. Each store call opens/commits its own `with_tenant` (`tenant.rs:116–142`). If `cards.answer` commits (7677) and `effects.record` then fails with a non-`Conflict` error (7708), the card is closed but the effect is missing. A retry hits the pending-status check (7672) and returns `Conflict` before ever reaching `effects.record` — the approved effect is permanently lost and the run stays parked. No recovery path exists.
- **A2 (HIGH, CONFIRMED):** mirror case — if `effects.record` succeeds (7704) but a later step (`set_run_status`, pending-call resolution, 7713–7757) fails, the card is answered and the effect recorded, yet the run is never resumed; retry again short-circuits at 7672. Stuck run with an "applied" effect that never performed work.
- **A3 (HIGH, CONFIRMED):** `apply_pending_tool_calls` records the effect row *before* executing the tool (`usecases.rs:7105–7154`). A crash between record and execute leaves the effect marked applied; the `Conflict` guard (7125) then prevents re-execution on replay. The side effect is silently skipped.
- **A4 (MED, CONFIRMED):** `find` (7670) and `answer` (7677) are separate round-trips; between them the card can be answered/deleted. The conditional `UPDATE` absorbs this, but the redundant read doubles failure surface and yields an inconsistent error: port docs promise `NotFound` for a missing card yet `answer` returns `Conflict` for both missing and already-answered (150–151).
- **A5 (LOW, CONFIRMED):** `space_id` for `EffectStore::record` comes from `context.space_id().ok_or(NotFound)?` (177); a `System` context silently cannot record effects. Acceptable today, but undocumented for system-driven approval paths.

### Stale cards (C)

- **C1 (HIGH, CONFIRMED):** `run_id REFERENCES runs(id) ON DELETE SET NULL` (migration 0004:11). Deleting a run leaves a `pending` card with `run_id = NULL`: the unique guard no longer applies (R2) and answering it records no effect and resumes no run (7680). The member sees an approval "succeed" while nothing happened.
- **C2 (MED, CONFIRMED):** `list_pending` has no expiry, filter or `LIMIT` (33–40). Answered cards are never pruned; one pending card per run accumulates across a thread (REQ-007 allows concurrent runs), so the list grows unbounded and is eventually re-served to members long after relevance.
- **C3 (MED, GUESS):** `created_at` is caller-supplied and bound as a string cast to `timestamptz` (104), while `answered_at = now()` uses the DB clock (134). Two clocks: ordering by `created_at` can be skewed across replicas; a bad caller string is rejected by Postgres only at write time.
- **C4 (LOW, CONFIRMED):** no soft-delete/cancel status. `rows::card` only accepts `pending|answered|declined` (rows.rs:421–430); a `cancelled`/`expired` state cannot be represented, so stale cards cannot be retired in-domain.

## Concrete hardening (prioritized)

- **P0 — close the lost-effect window (A1/A2/A3):** make answer + effect-intent one atomic unit. Preferred: an outbox — in one `with_tenant` transaction, flip the card and insert an `external_effects` row (or a dedicated `effect_outbox`) with the deterministic key; a worker/continuation performs the actual tool call after commit. Keep effect keys as-is (`approve:<card_id>`, `tool:<run_id>:<call_id>`) so existing idempotency holds. **Do not** hold the DB transaction across external tool execution.
- **P0 — make retries recover, not short-circuit (A1/A2):** add a finalize/resume path keyed by card id: when a card is already closed, load its recorded effect and complete any unfinished resume steps before returning the existing result. Only `Conflict` on a *genuinely* different answer. This turns the current silent loss into convergent recovery.
- **P0 — seal the NULL-run gap (R2/C1):** add a unique index for run-less cards scoped to approval-like kinds, e.g. `UNIQUE (thread_id, kind) WHERE status='pending' AND run_id IS NULL AND kind IN ('approval','purchase')`, and stop relying on `SET NULL` for pending cards (close them on run deletion). Migration + ADR required.
- **P1 — accurate error contract (A4/R3):** check existence once and return `NotFound` vs `Conflict` as the port documents; restrict the unique-violation → `Conflict` mapping to the named constraints (`cards_thread_run_key`, `external_effects_pkey`) so unrelated violations surface as `Unavailable`.
- **P1 — stale-card lifecycle (C1/C2/C4):** add `expires_at` (DB clock) and a `cancelled`/`expired` status; have `list_pending` filter expired rows and add a bounded sweep. Decide run-deletion policy (decline vs cascade) — record any externally visible change in `docs/PARITY.md` and get operator approval (DEVIATION).
- **P1 — bound reads & single clock (C2/C3):** add `LIMIT` + keyset pagination to `list_pending`; move `created_at` default to `now()` in the DB (keep the supplied value only where a retry needs it) so `ORDER BY` is replica-safe.
- **P2 — observability & tests:** warn+metric whenever a recorded effect is skipped as a replay (today silent at `usecases.rs:7705–7707`), and add the integration tests below.

## Risks & mitigations

- Transactional/outbox API changes port signatures and all `EffectStore` implementors (infra + test mocks); land interface + mock updates together, via ADR. Internal port change → not a public DEVIATION, but confirm.
- The new partial unique index may reject legitimate concurrent *question* cards if mis-scoped; restrict to approval/purchase kinds (REQ-007 intentionally allows concurrent chat cards).
- Closing cards on run deletion and adding TTL change externally visible `list_pending`/answer behavior → ADR + `docs/PARITY.md` + operator sign-off before shipping.
- Enlarging transaction scope must never span an external tool call; outbox keeps the commit local and the execution post-commit.
- `created_at` becoming DB-generated changes stored values; keep a migration/backfill and treat as a data-format DEVIATION if clients read it.

## Acceptance tests (suggested names)

- `req_017_concurrent_answers_only_one_wins`
- `req_017_answer_committed_effect_failure_is_recoverable`
- `req_017_recorded_effect_survives_later_resume_failure`
- `req_017_null_run_approval_cannot_be_created_twice`
- `req_007_run_deletion_does_not_leave_answerable_stale_card`
- `req_007_list_pending_is_bounded_and_db_ordered`
- `req_017_answer_missing_card_returns_not_found`

## Decisions needed (GUESS → operator/spec)

1. On run deletion, should pending cards be declined, cascaded, or left (C1)? GUESS: decline.
2. Is a TTL/expiry for pending approvals acceptable (DEVIATION) (C2/C4)?
3. Are run-less approval/purchase cards actually created, i.e. is R2 reachable in production today?
4. May the internal `EffectStore`/`CardStore` ports gain a transactional/outbox method (P0)?
5. Should `created_at` become DB-generated for replica-safe ordering (C3)?
