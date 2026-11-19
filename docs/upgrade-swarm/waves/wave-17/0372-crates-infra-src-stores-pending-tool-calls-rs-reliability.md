# Worker #372 — `crates/infra/src/stores/pending_tool_calls.rs` — LENS: reliability — v0.4.0 proposal

- **Status:** PROPOSAL (read-only review; no source changed). CONFIRMED = verified by reading `pending_tool_calls.rs`, `tenant.rs`, `migrations/0021_pending_tool_calls.sql`, `migrations/0022_pending_tool_calls_status_check.sql`, `crates/app/src/ports.rs`, `crates/domain/src/card.rs`, `crates/app/src/usecases.rs` (`park_tool_calls`, `apply_pending_tool_calls`, `answer_card`) and `crates/testkit/src/memory.rs` in this worktree; GUESS = needs a test/oracle run.
- **Scope:** Postgres adapter for `PendingToolCallStore` (`create`, `find_pending`, `mark_applied`, `mark_refused`) and its `pending_row_state` classifier (REQ-016/REQ-017).
- **Spec note:** REQ-013/REQ-014 define lease expiry for *runs*, but the spec is silent on expiry for approval cards/pending calls (GUESS). Any TTL/status addition is a DEVIATION: ADR + `docs/PARITY.md` + operator sign-off first.

## Summary

The per-row SQL is correct in isolation: the `(run_id, call_id)` PK with `ON CONFLICT DO NOTHING` (77–83) is an idempotent register, and `UPDATE ... WHERE status='pending'` (149, 185) is an atomic state CAS. Reliability breaks at the seams: the conflict path discards a *divergent* replay payload, the failed-UPDATE classification happens in a second transaction, `find_pending` has no claim, and there is no expiry, no orphan reaping and no bound on pending rows. Exactly-once *execution* currently rests on the effects ledger in `usecases.rs:7106–7130`, not on this store — and a lost CAS is swallowed by the caller.

## Failure modes

### Double approval & state races (R)

- **R1 (LOW, CONFIRMED):** concurrent `create` for the same key is safe: one `RETURNING` row, one `None` → `Conflict` (98–103); callers absorb it (`usecases.rs:6906–6911`, `7000–7005`).
- **R2 (MED, CONFIRMED):** `DO NOTHING` ignores *content*. A replay carrying a different `name`/`arguments`/`thread_id`/`space_id` under the same key is silently dropped; the stale row wins and the wrong tool/arguments are applied. The in-memory store has the same blind spot (`memory.rs:3122–3129`).
- **R3 (HIGH, CONFIRMED):** `mark_applied`/`mark_refused` are atomic against each other, but a loser reaches `pending_row_state` (166, 201) and then `apply_pending_tool_calls` logs the error as a warning and still appends a tool result (`usecases.rs:7160–7181`). A worker that lost the race reports success and emits a second, fabricated tool message.
- **R4 (MED, CONFIRMED):** `pending_row_state` opens a *second* `with_tenant` transaction (208–228; `tenant.rs:116–142`), so classification is not atomic with the failed UPDATE. A concurrent `create`/`delete` between them can mislabel the cause; `Conflict` also conflates "already answered" with "row exists but is still pending".
- **R5 (MED, CONFIRMED):** `find_pending` (114–121) has no `FOR UPDATE ... SKIP LOCKED` and no claim. Two `answer_card` workers read the same pending set and both may execute a tool; only the effects key dedups them. A provider issuing a fresh `call_id` for a retry defeats even that key (new row, new `tool:` key).
- **R6 (LOW, GUESS):** `created_at`/`answered_at` are caller-supplied strings (85–91, 156–157) with no `answered_at >= created_at` guard; clock skew can corrupt ordering and audit trails.

### Orphaned calls (O)

- **O1 (HIGH, CONFIRMED):** FK `ON DELETE CASCADE` (migration 0021:8–11) cleans rows when the run/space/thread is deleted, but a crash between `create` and card creation (`usecases.rs:6906→6914`) leaves a `pending` row with no card and a live run. Nothing reaps or reports it; the run can never be approved.
- **O2 (MED, CONFIRMED):** no `card_id`/`claimed_at` column links a row to the card that gates it, so card cancellation cannot retire the call.
- **O3 (MED, CONFIRMED):** no bound on `find_pending`; rows accumulate for the life of the run.

### Expiry (E)

- **E1 (HIGH, CONFIRMED mechanism / GUESS decision):** no `expires_at`, no TTL, and the check constraint allows only `pending|applied|refused` (migration 0022:5–7; `card.rs:105–112`). Stale approvals are re-served indefinitely, unlike run leases.
- **E2 (HIGH, GUESS):** even if expiry is added, `mark_applied` must reject expired rows; otherwise a late approval applies an action after its window closed.

### Atomicity (A)

- **A1 (MED, CONFIRMED):** the tool is executed before `mark_applied` and a mark failure is only warned (`usecases.rs:7154–7167`). On replay the row is still pending, the effects ledger returns `Conflict`, and the caller overwrites the tool message with the `"the effect was already applied"` note — losing the real result.
- **A2 (MED, CONFIRMED):** `create` and card creation are separate transactions (`usecases.rs:6906`, `6914`); the gap is O1.

## Concrete hardening (prioritized)

- **P0 — atomic classification (R4):** replace UPDATE + `pending_row_state` with one statement in the existing transaction, e.g. `WITH upd AS (UPDATE ... RETURNING ...) SELECT ... FROM upd UNION ALL SELECT status FROM pending_tool_calls WHERE ... LIMIT 1`, or do both statements on the same connection and return the *actual* current status. Distinguish `NotFound` (no row) from `Conflict` (row not pending) and include the stored status in a traced field.
- **P0 — reject divergent replays (R2):** after a `create` conflict, read the existing row inside the same transaction and compare a fingerprint of `(name, arguments, thread_id, space_id)`; identical → absorb as today, different → return a dedicated error and emit a metric. No schema change needed if computed in Rust.
- **P0 — stop false-success on lost CAS (R3/A1):** have the caller treat a `mark_applied` `Conflict` as terminal, and on replay reuse the stored `result` from the already-applied row instead of fabricating a note. Prefer a store method that returns the existing row on conflict (port change; land with the Memory adapter).
- **P1 — claim instead of bare read (R5):** add an intermediate `applying` status plus `claimed_at` (DB clock) and an atomic claim `UPDATE ... WHERE status='pending' AND (claimed_at IS NULL OR claimed_at < $cutoff) RETURNING`, so only one worker applies. Never hold the DB transaction across the external tool call; reclaim only after a lease-style timeout.
- **P1 — lifecycle & bounds (O1–O3, E1–E2):** add `expires_at`, `card_id`, a `cancelled`/`expired` status, `find_pending` filtering on `expires_at > now()` with a `LIMIT`, and a bounded `find_stale_pending(cutoff)` for a reaper/metric. `mark_applied` CAS must also require `expires_at IS NULL OR expires_at > now()`. Requires migration + domain enum + `rows.rs` parser updates, ADR and operator approval.
- **P2 — observability & parity (R6):** counters for absorbed replay-CREATE, lost CAS, and stale-pending age; move expiry/`answered_at` to DB `now()` with a backfill, and assert the Memory adapter behaves identically.

## Risks & mitigations

- Port signature/method changes touch the Postgres and Memory adapters and test mocks; land all together via ADR. Internal interface change → not a public DEVIATION, but confirm.
- New column/status changes the data contract and the `status` check constraint: backfill, migrate in a low-traffic window, keep `pending|applied|refused` readable, record as DEVIATION before shipping.
- A claim status must be crash-safe (timeout reclaim) and must not serialize with the effects ledger in a way that skips execution; test crash-before-mark explicitly.
- Adding a fingerprint must keep *identical* replays absorbed — a regression here breaks REQ-017 replay tolerance.
- Do not widen transaction scope over external tool execution; `with_tenant` transactions stay per call (`tenant.rs:116–142`).
- Expiry changes what members can approve → spec/ADR/operator sign-off; do not guess the TTL value.

## Acceptance tests (suggested names)

- `req_017_divergent_replay_payload_is_rejected`
- `req_017_concurrent_applies_only_one_changes_state`
- `req_017_concurrent_approve_refuse_loser_does_not_emit_tool_result`
- `req_017_failed_mark_update_reports_accurate_status`
- `req_017_replay_after_mark_failure_keeps_original_result`
- `req_017_orphan_pending_call_is_detectable`
- `req_017_expired_call_cannot_be_marked_applied` (after ADR)
- `req_017_find_pending_is_bounded` (after ADR)
