# v0.4.0 reliability proposal — `migrations/0030_audit_events.sql` (worker #474, lens: reliability)

- **Status:** PROPOSAL (read-only review; no source changed). CONFIRMED = verified by reading this worktree; GUESS = needs a runtime/test or oracle run.
- **Scope:** `0030_audit_events.sql` — the `audit_events` table, its single index, RLS enable/force and the `audit_events_tenant_isolation` policy, read together with its follow-ups (`0045_audit_cors.sql`, writer `crates/infra/src/stores/audit.rs`) and REQ-045 / D-05.
- **Constraint:** additive only. Every change below is a *new* numbered migration (e.g. `0061_audit_events_hardening.sql`), `COMMENT`, or test; nothing rewrites 0030 itself. 0030 and 0045 stay an inseparable pair (0045 already widens `kind`, makes `space_id` nullable with `ON DELETE SET NULL`, and rewrites the policy for deployment-scoped rows — CONFIRMED).

## Current structure (CONFIRMED — read this session)

22-line forward-only migration. `id text PRIMARY KEY` (caller-generated), `space_id text NOT NULL REFERENCES spaces(id) ON DELETE CASCADE`, `actor_user_id text` (nullable, no FK), `kind text NOT NULL CHECK (kind IN ('mcp.private_call'))`, `target text NOT NULL`, `metadata jsonb NOT NULL DEFAULT '{}'`, `created_at timestamptz NOT NULL DEFAULT now()`; index `(space_id, created_at DESC)`; `ENABLE`/`FORCE ROW LEVEL SECURITY`; policy `USING/WITH CHECK (space_id = boma.current_space_id() OR boma.is_system())`. The writer binds all seven columns verbatim — notably a **caller-supplied** `created_at` (audit.rs:105-115), bypassing the `DEFAULT now()`. Spec anchors: REQ-045 (JSONL export, stable shape); 04-nonfunctional.md:83/108-109 ("audit rows and exports never contain secret material", append-only, JSONL without secrets); 02-data-model.md:12 (deleting a user keeps audit rows); D-05 (one event per private-MCP use).

## Failure modes

### Redaction (S)

- **S1 (HIGH, CONFIRMED):** no DB-level redaction or containment. `target`/`metadata` are stored verbatim; redaction is export-only in the app layer, walks `metadata` only, and leaves `target` unchanged (usecases.rs:8561-8562). Plaintext secrets can sit at rest, visible to any operator/backup/direct SQL reader, contradicting 04-nonfunctional.md:83. A bypassing `INSERT` has zero defence in depth.
- **S2 (MED, CONFIRMED):** `target text` and `metadata jsonb` have no length/byte cap; a large or attacker-influenced value bloats the table and inflates a 4 000-row export and every backup.
- **S3 (MED, GUESS):** `actor_user_id` has no FK and can hold arbitrary PII (e.g. an email); it persists after account deletion with no pseudonymisation.
- **S4 (LOW, CONFIRMED):** the denylist is ~10 secret prefixes (`sk-`, `ghp_`, `Bearer `, …; domain/audit.rs:79-81); unknown shapes and secrets in `target` bypass it. The migration cannot fix the matcher but can stop unbounded plaintext at rest.

### Retention (R)

- **R1 (HIGH, CONFIRMED):** the header promises "M9 retention (REQ-045)" that reads but never rewrites the table; no `DELETE`, TTL, partition, or archival exists anywhere in `migrations/` for `audit_events` — only a 4 000-row *read* cap. Unbounded growth → disk pressure → more failed appends.
- **R2 (HIGH, CONFIRMED structural):** as shipped, `ON DELETE CASCADE` erased a space's audit rows, contradicting 02-data-model.md:12; 0045 compensates with `SET NULL`. But "append-only" is convention only: no `REVOKE`, no trigger prevents `UPDATE`/`DELETE` by any role that reaches the table.
- **R3 (MED, CONFIRMED):** `created_at` is bound from the caller, not `DEFAULT now()`, with no sanity bound; clock skew or a future timestamp breaks time-based retention/partition routing and mis-orders the export.
- **R4 (LOW, CONFIRMED):** any future purge fights "append-only"; needs a legal-hold escape and export coordination before deletion is allowed.

### RLS / tenancy (T)

- **T1 (MED, CONFIRMED structural; GUESS exploitability):** the policy rests entirely on `boma.current_space_id()` / `boma.is_system()` reading GUCs via `current_setting(..., true)`. Any caller able to `SET boma.space_id` / `boma.system_reason` bypasses isolation; `FORCE RLS` does not stop a superuser. Connection-role concern, outside this file.
- **T2 (LOW, CONFIRMED):** the 0030 policy itself is well formed and fail-closed (no tenant context → no rows); enabling RLS before creating the policy means a partial migration leaves deny-all (acceptable). It is not idempotent — a re-run after partial failure errors; fine for versioned migrations, note it.
- **T3 (MED, GUESS):** `WITH CHECK` lets a `boma.is_system()` caller insert any `space_id`; a member can insert any `actor_user_id` (no FK). Cross-space / wrong-actor rows are representable if the app path drifts.
- **T4 (LOW, CONFIRMED):** `(space_id, created_at DESC)` serves the member export, but after 0045 NULL-space rows are not reachable by that btree for the system export path — review before choosing a partition key.

## Concrete hardening (prioritized, all additive)

1. **P0 — bound payloads (S2):** new migration adds `ALTER TABLE audit_events ADD CONSTRAINT audit_events_target_size_chk CHECK (octet_length(target) <= <cap>)` and `..._metadata_size_chk CHECK (octet_length(metadata::text) <= <cap>)`, with caps configurable in Rust. `COMMENT ON COLUMN` states that callers must redact before insert. *Risk:* rejects legitimate long targets — calibrate from observed data, record any rejection in `docs/PARITY.md`.
2. **P0 — enforce append-only (R2):** `REVOKE UPDATE, DELETE ON audit_events FROM <app-writer-role>` (grant `SELECT, INSERT` only), plus a `BEFORE UPDATE OR DELETE` trigger that `RAISE EXCEPTION`s unless `boma.is_system()` with a logged reason, giving one controlled retention path. *Risk:* needs the exact role names; blocks admin cleanup — coordinate with the retention job and record an ADR.
3. **P1 — retention (R1/R4):** range-partition by `created_at` (or dated archival table + scheduled purge) with a configurable window and a nullable `legal_hold boolean DEFAULT NULL` column. *Risk:* changes schema and query plans; cuts against "append-only" — operator sign-off + ADR + export/legal-hold coordination; test with RLS forced.
4. **P1 — bound `created_at` (R3):** `CHECK (created_at BETWEEN TIMESTAMPTZ '2020-01-01' AND now() + interval '5 minutes')`, or have the writer use `DEFAULT now()`/server clock. *Risk:* rejects skewed clients — choose a generous window; document as a safety edge.
5. **P1 — tenancy assertion (T3):** do **not** add an `actor_user_id` FK (users are user-global; `ON DELETE SET NULL`/`RESTRICT` would break R2/02-data-model.md:12). Instead add integration tests asserting a cross-space insert is refused under a member context and that `boma.is_system()` is the only path that can write NULL-space rows. *Risk:* test-only; cannot defend against GUC `SET` (T1).
6. **P2 — idempotent policy re-assertion (T2/T4):** a `DO $$` block that enables+forces RLS and re-creates the policy idempotently, keeping the 0045 NULL-space rules verbatim; add tests for unscoped reads, bad-GUC reads, cross-tenant reads/writes and system export. *Risk:* low (metadata/test); must mirror 0045 exactly or it will silently widen access.

## Additive-only guarantee

Items 1-4 add named `CHECK`s, a `REVOKE`, a trigger and a nullable column; 5-6 are tests/metadata. No `DROP`, no type change, no destructive `UPDATE`, no back-fill. Applied-migration hash chain is preserved by new numbered files. Redaction-on-write at the DB level is deliberately left as a proposal only — it mutates at-rest data and needs an ADR + `docs/PARITY.md` entry before coding.

## Verification to add

Real-Postgres tests (RLS forced): oversized `target`/`metadata` rejected (S2); member `UPDATE`/`DELETE` refused while system retention path succeeds (R2); future `created_at` rejected (R3); unscoped/cross-tenant reads empty and cross-space insert refused (T1/T3/T2); system export reaches NULL-space rows after 0045 (T4). Apply new migrations to a fresh DB and to a copy of the current schema; assert existing row counts/`created_at` values unchanged. All claims above are CONFIRMED from source reading except those marked GUESS. Status: proposal only — no file outside this report was modified.
