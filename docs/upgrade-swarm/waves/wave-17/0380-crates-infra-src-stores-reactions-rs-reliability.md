# Worker #380 — `crates/infra/src/stores/reactions.rs` — LENS: reliability — v0.4.0 proposal

- **Status:** PROPOSAL (read-only review; no source changed). CONFIRMED = verified by reading `reactions.rs`, `stores/rows.rs` (`ReactionRow`, `ReactionAttributionRow`), `tenant.rs`, `migrations/0009_reactions.sql`, `crates/app/src/ports.rs` (1153–1195), `crates/domain/src/message.rs` (144–153), `crates/app/src/usecases.rs` (`react_to_message`, 7784–7803) and `crates/api/src/rpc.rs` (1668–1717) in this worktree; GUESS = needs a test/oracle run.
- **Scope:** the Postgres adapter for `ReactionStore` (`react`, `list_for_messages`) and its emoji validation (REQ-008).
- **Spec note:** the RPC carries an explicit `add|remove` action, not a blind toggle (`rpc.rs:1683–1700`); `user_id` is taken from the auth context, never the body (`usecases.rs:7793`). Any change to that contract is a DEVIATION: ADR + `docs/PARITY.md` + operator sign-off first.

## Summary

The single-row SQL is sound: one transaction per call (`with_tenant`, `tenant.rs:116–142`), a PK-guarded idempotent `INSERT ... ON CONFLICT DO NOTHING` (75), FKs with `ON DELETE CASCADE` on all four parents (migration 0009:7–10), and a caller-attributed read-back (101–113). Reliability breaks at the seams: Add/Remove is a last-writer-wins race whose returned "truth" can diverge from the committed state, emoji keys are unnormalized so visually identical reactions coexist, the space predicate is applied only to the write path (not DELETE/read-back), and every DB error collapses to one opaque `Unavailable` with no retry or bound. There is no drift detector for stranded rows and no cap on the batch read.

## Failure modes

### Toggle races (T)

- **T1 (HIGH, CONFIRMED):** an Add and a Remove for the same `(message, user, emoji)` are two independent transactions with no row lock. Under READ COMMITTED the read-back (101–113) sees a *fresh* snapshot, so a client can receive `reacted=true, count=1` while a concurrent Remove commits immediately after: the returned "truth" was true at read time but is false in the final state. Same-key Adds are safe (PK + `DO NOTHING`), but Add-vs-Remove is genuine last-writer-wins with a misleading reply.
- **T2 (MED, CONFIRMED):** the existence check (56–64) and the `INSERT ... SELECT` (71–76) are separate statements. A concurrent message delete makes the INSERT affect zero rows; the call still returns 200 with an empty list instead of `NotFound`. No orphan is created (FK cascade + the guarded SELECT), but the reply is semantically stale.
- **T3 (MED, GUESS):** cancellation is safe in principle — Add/Remove are idempotent, so a dropped future rolls back and a lost reply is safely retried — but a client that retries an Add after a committed-but-unacknowledged Remove can flip the state back. Without an idempotency key the product cannot distinguish a retry from a new intent.
- **T4 (LOW, CONFIRMED):** all driver errors map through `tenant::unavailable` (153–156), which discards the distinction between a transient deadlock/serialization failure (retryable) and a hard failure. Retries are left entirely to the client.

### Duplicate reactions (D)

- **D1 (MED, CONFIRMED):** `valid_emoji` (196–203) trims but does not normalize. Canonically equivalent strings (e.g. `👍` vs `👍️` with U+FE0F, ZWJ orderings) hash to different PK keys and render as two separate emoji with split counts — a "duplicate" by any user's definition.
- **D2 (LOW, CONFIRMED):** the validator admits any short non-control string ("`yes`", "`a b`"), so the key space is arbitrary text, not emoji. Cardinality is bounded per user but "duplicate" has no canonical meaning.
- **D3 (LOW, CONFIRMED):** `ON CONFLICT DO NOTHING` (75) makes a replayed Add fully idempotent — good — but silently discards it; the caller only learns the outcome from the full truth list, which is sufficient here.

### Orphan cleanup (O)

- **O1 (LOW, CONFIRMED):** FKs cascade on message/user/space/thread delete (migration 0009:7–10), so normal deletes leave no orphans. No equality constraint ties `message_reactions.space_id` to the parent message's `space_id`, however; the Add path derives it correctly from `messages` (72), but any future message-space rewrite would strand reactions behind RLS. `REACTIONS` rows for departed space members also remain and still count (policy, not necessarily a bug).
- **O2 (MED, GUESS):** there is no reconciliation job or drift metric. A raw SQL path that bypasses FKs (TRUNCATE, a future migration) would leave reaction rows with no parent, invisible to normal reads but consuming space and skewing any admin count. Nothing detects it.
- **O3 (MED, CONFIRMED):** the DELETE (87–88) and the aggregate read (101–107) omit the `space_id` predicate that the existence check and Add path carry; isolation rests solely on RLS. `FORCE ROW LEVEL SECURITY` is on (migration 0009:24), but a superuser bypasses it — the code itself notes RLS is only defense in depth (52–55).

### Cross-cutting (X)

- **X1 (LOW, CONFIRMED):** `list_for_messages` increments a `u32` with `entry.0 += 1` (167) — a debug-build panic / release wrap if a key ever overflows, and no bound on `message_ids` (140–143) despite the "page-full" comment (146).
- **X2 (LOW, CONFIRMED):** count conversion saturates silently at `u32::MAX` (119) and malformed stored IDs fail the *entire* batch (182); neither is traced.

## Concrete hardening (prioritized)

- **P0 — serialize per message (T1):** take a transaction-scoped advisory lock keyed on the message id (`pg_advisory_xact_lock(hashtext($message_id))`) at the top of the body, before the existence check. Add/Remove then cannot interleave, and the read-back always matches the committed final state. `with_tenant` already scopes the lock to the transaction, so it releases on commit/rollback. Risk: a hot message serializes its reactors — measure contention; use `hashtextextended` if collisions matter.
- **P0 — explicit space predicate everywhere (O3):** add `AND space_id = $n` to the DELETE and the aggregate SELECT, binding the already-extracted `space_id_key`. Pure defense in depth, no behavior change, no migration. Risk: negligible; strengthens isolation if RLS is ever bypassed.
- **P1 — classify and retry transient DB errors (T4):** in `tenant::unavailable`/`react`, recognize SQLSTATE `40P01` (deadlock) and `40001` (serialization failure) and retry the whole `with_tenant` body a bounded number of times with jittered backoff; Add/Remove are idempotent so replay is safe. Risk: retry must restart the transaction, so this touches `with_tenant` and needs a timeout budget; do not retry on `is_unique_violation` (159–164) or a `NotFound`.
- **P1 — drift detection (O2):** expose a bounded admin query `SELECT ... FROM message_reactions r WHERE NOT EXISTS (SELECT 1 FROM messages m WHERE m.id = r.message_id)` plus a metric, and schedule it low-traffic. With cascades it should return zero; treat any row as a data-integrity alert. Risk: a full scan — run in batches/off-hours; ideally add an index or drive it from recent deletions.
- **P1 — normalize emoji keys (D1):** apply Unicode NFC normalization (and a consistent variation-selector policy) in `valid_emoji`/before keying, plus a one-off dedupe migration and an ADR. This merges two currently distinct keys → externally visible count change → DEVIATION, operator approval required. Risk: normalization can merge intended-distinct sequences; validate against the golden set first.
- **P2 — saturating/bounded arithmetic (X1/X2):** replace `entry.0 += 1` with `saturating_add(1)`, cap `message_ids.len()` with a clear error or chunk the query, and trace on count saturation and on `MessageId::new` failure. Internal-only, no wire change. Risk: a hard cap rejects a previously accepted (if unreasonable) batch — set it generously.
- **P2 — idempotency key (T3):** carry a client nonce in the React request so a replayed intent is absorbed. Port + API + wire + Memory adapter change; schedule only after the P0/P1 items and an ADR.

## Risks & mitigations (cross-cutting)

- The advisory lock and retry both change transaction shape; keep `with_tenant` as the single place that opens/commits transactions (`tenant.rs:116–142`) and never hold a DB transaction across unrelated awaits.
- Emoji normalization and a request nonce are externally visible → ADR, `docs/PARITY.md`, operator sign-off; do not ship silently.
- Do not widen `timeouts`: per AGENTS.md every outbound call needs a bound; ensure the pool/statement timeout covers the lock wait, or the advisory lock can become a denial-of-service lever.
- Memory adapter parity: every behavior change here must be mirrored in `crates/testkit/src/memory.rs` and asserted by the same REQ-named test.

## Acceptance tests (suggested names)

- `req_008_concurrent_add_and_remove_return_committed_truth`
- `req_008_add_is_idempotent_under_retry`
- `req_008_reaction_delete_and_read_are_space_scoped`
- `req_008_transient_deadlock_is_retried_within_budget`
- `req_008_orphan_reaction_is_detected`
- `req_008_equivalent_emoji_collapse_to_one_key` (after ADR)
- `req_008_list_for_messages_is_bounded`
