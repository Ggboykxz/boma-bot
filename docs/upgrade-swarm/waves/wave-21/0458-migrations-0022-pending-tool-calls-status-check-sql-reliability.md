# Worker #458 — `migrations/0022_pending_tool_calls_status_check.sql` — LENS: reliability — v0.4.0 proposal

- **Status:** PROPOSAL (read-only review; no source changed). CONFIRMED = verified by reading `migrations/0022_pending_tool_calls_status_check.sql`, `migrations/0021_pending_tool_calls.sql`, `migrations/0024_memory_shared_bot_id_nullable.sql`, `migrations/0055_signin_requests.sql`, `crates/infra/src/db.rs`, `crates/infra/src/stores/pending_tool_calls.rs`, `crates/infra/src/tenant.rs` (via 0371/0372), `crates/domain/src/card.rs`, `spec/01-requirements.md` (REQ-017) and sibling reviews 0371/0372 in this worktree; GUESS = needs a DB/test run.
- **Scope:** one statement — `ALTER TABLE pending_tool_calls ADD CONSTRAINT pending_tool_calls_status_check CHECK (status IN ('pending','applied','refused'))`.
- **Additive only:** nothing below removes or weakens this statement; hardening is added as clauses, complementary constraints, tests and (where noted) a later additive migration. Keep the constraint name as a stable anchor.

## Current guarantee (CONFIRMED)

The constraint is named (good: future migrations can reference it), the column is `NOT NULL DEFAULT 'pending'` (0021:14), and the three values match `PendingToolCallStatus::{Pending,Applied,Refused}` byte-exactly (`card.rs:105–137`). The store never binds `status` on INSERT (uses the default) and flips it only via `WHERE status='pending'` CAS updates (`pending_tool_calls.rs:119,148,185`). Migrations are embedded, forward-only and transactional (`db.rs:20,72–74`): a failure aborts `boma migrate` and is retried on every boot.

## Failure modes

### Constraint / migration (M)

- **M1 (HIGH, CONFIRMED mechanism / GUESS data):** plain `ADD CONSTRAINT ... CHECK` is added as *valid* immediately: PostgreSQL takes `ACCESS EXCLUSIVE` and scans every row. 0021 allowed arbitrary strings, so any pre-existing out-of-vocabulary row (bug, direct SQL, or forward-compat write from a rolled-back newer binary) makes the ALTER fail. The forward-only migration never records, so every restart crashes at boot — an availability failure with no in-migration repair path. No bad rows are expected from current code (GUESS: confirm with `SELECT status, count(*) FROM pending_tool_calls GROUP BY status` on a real DB).
- **M2 (MED, CONFIRMED):** the scan holds `ACCESS EXCLUSIVE`, blocking all reads/writes; queued `with_tenant` acquisitions can hit their `acquire_timeout`, turning a schema upgrade into request errors on a live DB.
- **M3 (MED, CONFIRMED):** the vocabulary is duplicated three ways — this CHECK, the Rust enum (`card.rs:117–136`) and raw literals in the store (`'pending'/'applied'/'refused'` at 119/148/185). Any v0.4.0 lifecycle change (0372 proposes `applying`/`cancelled`/`expired`) needs a follow-up migration that drops/recreates this *named* constraint plus enum + store + Memory adapter edits; code-DB deploy order matters and a rollback can be rejected by the stricter DB.

### Invalid states (S)

- **S1 (MED, CONFIRMED):** the CHECK validates the *value* only, not transitions or cross-column consistency. `applied` with `result IS NULL`, `refused` with a non-NULL `result`, `answered_at` set while `pending`, `answered_at < created_at`, and `applied → refused` all pass. The app CAS prevents these today; the DB layer does not, so a bug or manual write can persist them and downstream `into_pending_tool_call` (`pending_tool_calls.rs:35–49`) will trust the row.
- **S2 (MED, CONFIRMED limitation):** a semantically invalid state is invisible here — a tool already executed but its row still `pending` (defect A1/R3 in 0372) satisfies the CHECK. No value constraint can detect an external side effect; this is a limitation to document, not fix in this file.
- **S3 (LOW, GUESS):** `text IN` uses the column collation. Under a nondeterministic (e.g. case-insensitive ICU) collation a case variant could pass the DB CHECK yet be rejected by `PendingToolCallStatus::parse`, contradicting the "byte-exact" comment. The default collation is likely deterministic; confirm before changing.

### RLS (T)

- **T1 (LOW, CONFIRMED by PostgreSQL semantics, GUESS on our error surface):** RLS from 0021:21–36 is DML-only; DDL and constraint validation are internal integrity operations that bypass it, so one global vocabulary is enforced across tenants — desirable. But a failing `VALIDATE` must be diagnosed in system context: a tenant-scoped `GROUP BY status` applies `space_id = boma.current_space_id()` and hides the offending row.
- **T2 (MED, CONFIRMED):** a check violation raised inside `with_tenant` is flattened by `tenant::unavailable` (`tenant.rs:153–156`) to `AppError::Unavailable`. A permanent integrity rejection then looks transient and can trigger retries that can never succeed. Today the store never writes `status` directly, so the trigger is future code or direct SQL — still worth classifying.
- **T3 (LOW, GUESS):** the constraint is tenant-agnostic by design; do not couple it to `space_id`/RLS in this file. Any per-tenant status rule is a new feature needing ADR + REQ-017 sign-off.

## Concrete hardening (additive, prioritized, with risks)

- **P0 — non-fatal validation (M1/M2):** add `NOT VALID` to the `ADD CONSTRAINT`, then `VALIDATE CONSTRAINT pending_tool_calls_status_check` after a system-context pre-check confirms only the three values (log a counted aggregate, no row data). The end state is still a fully valid constraint; only the exclusive-lock window changes. Risk: `NOT VALID` briefly leaves legacy bad rows unconstrained — mitigate by validating in the same low-traffic window; on a fresh DB the table is empty so validation is cheap.
- **P0 — verify the vocabulary (M1, S3):** REQ-017-named test asserting a bad status is rejected with SQLSTATE `23514`, all three valid values round-trip, and the constraint exists by name in `information_schema`. These runtime `sqlx` queries are not compile-time checked, so tests are the only guard.
- **P1 — cross-column consistency (S1):** in a later additive migration add `NOT VALID` constraints, then backfill/validate: `(status='pending' AND answered_at IS NULL) OR (status IN ('applied','refused') AND answered_at IS NOT NULL)`; `(status='applied' AND result IS NOT NULL) OR (status<>'applied' AND result IS NULL)`; `answered_at IS NULL OR answered_at >= created_at`. Risks: existing violating rows need non-destructive quarantine before validate; future statuses must revisit these predicates (ADR).
- **P1 — byte-exact guarantee (S3):** if collation is nondeterministic, replace the expression with `status COLLATE "C" IN (...)` in a new migration that keeps the name `pending_tool_calls_status_check` verbatim. Risk: any name drift breaks future migrations; treat the name as a compatibility constant.
- **P1 — error classification (T2):** map SQLSTATE `23514`/`23503` to a deterministic rejected/conflict error at the adapter boundary instead of `Unavailable`, keeping driver detail in `tracing` only (REQ-054). Risk: error shape is part of the stable API → `docs/PARITY.md` entry, Memory-adapter parity and a REQ-017-named regression test.
- **P2 — single-source drift guard (M3):** a CI check comparing the DB constraint's literal list, `PendingToolCallStatus::as_str()` and the store's SQL literals; fail on divergence.

## Risks & mitigations

- Keep DDL inside the embedded forward-only migrator; do not widen transaction scope or add a down migration silently (`db.rs:4–5`).
- Do not introduce a new status in v0.4.0 without REQ/ADR/operator sign-off: each needs this constraint dropped/recreated, the enum, store literals and Memory adapter updated in one coordinated deploy.
- Pre-check/backfill must be non-destructive: quarantine bad rows, never silently rewrite data; log aggregates only (no tenant PII).
- Do not change the 0021 RLS policy (FORCE RLS + system bypass); this migration adds data-integrity enforcement, not access rules.
- No public interface, wire field or data-format change; all items are internal hardening unless the error-shape mapping (P1, T2) is deemed externally visible — then file PARITY first.
