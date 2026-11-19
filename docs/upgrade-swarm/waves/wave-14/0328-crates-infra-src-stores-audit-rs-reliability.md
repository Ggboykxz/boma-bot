# v0.4.0 reliability proposal — `crates/infra/src/stores/audit.rs` (worker #328, lens: reliability)

- **Status:** PROPOSAL (read-only review; no source changed). CONFIRMED = verified by reading source in this worktree; GUESS = needs a runtime/test or oracle run.
- **Scope:** the Postgres `AuditStore` adapter — `append` (insert) and `list_for_space` (export/list back) for the escape-hatch audit trail (D-05, REQ-045).

## Summary

The store is a thin, correct-by-convention SQL adapter: one transaction-scoped insert and one RLS-scoped read. Its reliability problem is that it trusts every caller completely. Secrets are persisted verbatim and redacted only on export (never in `target`), every write failure collapses into `Unavailable` with no retry or outbox, and several call sites deliberately discard the result — so the "append-only" trail can silently develop holes. There is no retention implementation at all despite the migration promising one. No `#[cfg(test)]` exists in this file (CONFIRMED).

## Failure modes

### Dropped events (D)

- **D1 (HIGH, CONFIRMED):** `append` maps any driver error to `AppError::Unavailable` via `tenant::unavailable` (audit.rs:117, tenant.rs:153-156). There is no retry, buffer, outbox or dead-letter. A transient DB blip loses the event permanently, and multiple call sites discard the error entirely: `let _ = audit_private_use(...)` (usecases.rs:7230-7237), `let _ = ...audit.append(...)` on account deletion (account_export.rs:439-451), `let _ = self.record_sign_in_failure(...)` (accounts.rs:154). Security-relevant rows (private MCP use, account deletion, failed sign-in) are therefore best-effort in practice.
- **D2 (MED, CONFIRMED):** a duplicate `id` (PK violation) is indistinguishable from an outage: `tenant::is_unique_violation` exists (tenant.rs:158-163) but is unused here. `append` is not idempotent and gives callers no "stored / not stored" signal, so a retry after an ambiguous timeout cannot be made safe.
- **D3 (LOW, CONFIRMED):** each write is three round trips (`set_config` x2 + INSERT + COMMIT) inside `with_tenant`; more failure surface than a single statement, and a failure rolls back the lone insert with no local replay.

### Secret leakage / redaction (S)

- **S1 (HIGH, CONFIRMED):** the store binds `event.target` and `event.metadata` verbatim (audit.rs:112-113). Redaction happens only on the read/export path in the app layer (`boma_domain::redact_audit_value`, usecases.rs:8562, rpc.rs:3478) and **only on `metadata`** — `target` is exported unchanged (usecases.rs:8561). Plaintext secrets can sit at rest in `audit_events`, visible to any operator/backup and to direct SQL readers. This conflicts with spec/04-nonfunctional.md:83 ("audit rows and exports never contain secret material").
- **S2 (MED, CONFIRMED):** the redactor is a ~10-prefix substring denylist (`sk-`, `ghp_`, `Bearer `, ...; domain/audit.rs:79-81). Unknown token shapes, secrets embedded in URLs without a known prefix, and anything in `target` bypass it. The store provides zero defence in depth.
- **S3 (MED, GUESS):** `tenant::unavailable` logs `error = %error` at WARN (tenant.rs:154). sqlx errors normally carry SQL text/constraint names, not bound values; confirm no secret-bearing parameter can surface in logs before relying on it.
- **S4 (LOW, CONFIRMED):** `AuditEventView`'s doc says "the caller redacts or skips it" (ports.rs:632) — a convention, not a type invariant. Both the Postgres and memory stores return raw `metadata`.

### Retention / growth (R)

- **R1 (HIGH, CONFIRMED):** append-only with no delete, TTL, partitioning or archival. Migration 0030 promises "M9 retention" (0030_audit_events.sql:5) but a grep of `migrations/` finds no `DELETE`/partition for `audit_events`. Unbounded growth → disk pressure → more `Unavailable` writes, feeding D1. The 4 000-row export cap (usecases.rs:8539) is a read limit, not retention.
- **R2 (MED, CONFIRMED):** account deletion keeps audit rows (spec/02-data-model.md:12) with `actor_user_id` intact and `space_id` SET NULL (0045_audit_cors.sql:26-32), making them deployment-scoped/operator-only — but PII persists indefinitely with no pseudonymisation, in tension with the log/export personal-data goal.
- **R3 (LOW, CONFIRMED):** no bound on `target` length or serialized `metadata` size; a large/attacker-influenced value bloats the table and inflates a `fetch_all` of up to 4 000 rows.

### Query contract (Q)

- **Q1 (MED, CONFIRMED):** `list_for_space` ignores its `space_id` argument (`_space_id`, audit.rs:33) and relies solely on RLS via the tenant context. A mismatch silently returns the context's rows; a caller asking for space A while acting in space B gets B's events. Today's callers pass `context.space_id()` (usecases.rs:8509/8534), which masks it.
- **Q2 (LOW, CONFIRMED):** `kinds_prefix` becomes `LIKE '{text}%'` (audit.rs:37); a prefix containing `%`/`_` acts as a wildcard and broadens the filter beyond the intended kind set.
- **Q3 (LOW, CONFIRMED):** `limit: i64` is bound directly (audit.rs:48); a negative value means `LIMIT -1` (unbounded) in Postgres. The use case caps at 4 000, but the store does not.

## Concrete hardening (prioritized)

1. **P0 — redact and bound on write (closes S1/S2/S4/R3):** apply `boma_domain::redact_audit_value` to `metadata` and a scalar-valued equivalent to `target` inside `append`, before binding; reject or truncate `target`/`metadata` over a configured byte cap and record a `truncated` flag. Keeps plaintext out of the table and backups. At-rest data shape changes → ADR + `docs/PARITY.md` entry.
2. **P0 — no silent drops (closes D1/D2):** classify failures with a typed error (reuse `is_unique_violation` for terminal duplicate-id; everything else retryable). Add bounded retry with jitter for transient `Unavailable`, or a small DB outbox for durable replay. At minimum, make the three security-critical call sites stop using `let _ =` and emit a drop counter/metric.
3. **P1 — retention policy (closes R1/R2):** versioned migration + scheduled job that archives/purges rows older than a configurable window (range-partition `created_at` for cheap `DROP PARTITION`), and pseudonymise `actor_user_id` on account deletion under a documented policy with a legal-hold escape.
4. **P1 — fix the query contract (closes Q1/Q2/Q3):** honour `space_id` with an explicit predicate alongside RLS (or remove it from the port); escape `%`/`_` in `kinds_prefix` (or use `left(kind, length($1)) = $1`); clamp `limit` to `0..=4_000` in the store.
5. **P2 — observability:** append counters/histogram by class (ok, duplicate, retry, dropped) and redaction/truncation counts; structured logs with kind and byte sizes only, never values. Verify S3 before assuming logs are value-free.

## Risks / trade-offs

- Write-path redaction irreversibly alters stored data and export output; an over-broad denylist can blank legitimate targets (e.g. a tool named `ghp_...`), harming audits. Keep the redactor conservative and deterministic; if verbatim export is required, record a `redacted` flag instead of mutating `target`.
- Retry/outbox gives at-least-once semantics; idempotency relies on caller-generated `id` being stable across retries — verify before coding, or switch to a deterministic hash of event fields.
- Retention cuts against "append-only" and "deleting a user keeps audit rows"; needs operator approval and an ADR (AGENTS.md externally visible behaviour change) plus export/legal-hold coordination.
- Partitioning changes schema and query plans; test migrations against a real Postgres with RLS enabled under `--locked`.
- Adding an explicit `space_id` predicate can return empty rows if a caller passes a mismatched id — ship with a test asserting argument equals context.
- Clamping `limit` changes the port contract for any future caller wanting more than 4 000 rows (none today, CONFIRMED); document it.

## Verification to add

Real-Postgres integration tests: redaction-on-write (S1), duplicate-id classified as terminal (D2), negative limit returns empty (Q3), wildcard prefix escaped (Q2), cross-tenant/mismatched-space list returns empty (Q1), retention purge after the window (R1). All claims above are CONFIRMED from source reading except items explicitly marked GUESS.
