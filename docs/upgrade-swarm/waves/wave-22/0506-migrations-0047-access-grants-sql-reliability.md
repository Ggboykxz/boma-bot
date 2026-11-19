# Wave 22 / Worker 0506 — `migrations/0047_access_grants.sql` (LENS: reliability)

Proposal for v0.4.0. **Additive only:** every change adds a column, constraint, policy, index or comment in a NEW later migration; `0047_access_grants.sql` is never edited in place, so applied sqlx checksums stay valid. CONFIRMED = read/verified this session; GUESS = needs an oracle run or an operator/spec decision. No code outside this report was changed.

## Current structure (CONFIRMED — read this session)

`0047_access_grants.sql` is 17 lines: `email text PRIMARY KEY, reason text NOT NULL, granted_by text NOT NULL, created_at timestamptz NOT NULL DEFAULT now()`, then `ENABLE` + `FORCE ROW LEVEL SECURITY` and one PERMISSIVE policy `access_grants_system_only USING (boma.is_system()) WITH CHECK (boma.is_system())`. No CHECKs beyond NOT NULL, no FK, no index beyond the PK, no expiry/tombstone, no `updated_at`.

Related surfaces (CONFIRMED):
- `crates/app/src/usecases.rs:9088-9094`: `reason` trimmed, non-empty, <= 200 chars; `email` is `Email::new(trim().to_ascii_lowercase())`; `created_at` is the app clock.
- `crates/domain/src/account.rs:33`: `Email::new` lowercases; `0051_lowercase_users_email.sql` backfills `users.email`; recovery/invitation compare addresses byte for byte (REQ-040).
- `crates/infra/src/stores/admin.rs:133-229`: grants listed `ORDER BY email`, inserted `ON CONFLICT (email) DO NOTHING` then re-`SELECT`ed, revoked with a hard `DELETE`.
- `crates/domain/src/tenancy.rs:19-49`: `SystemReason` is an 8-value closed enum (Boot, Authentication, WebhookDelivery, ScheduledJob, DataMigration, SharedPageRead, ScimDirectorySync, AccountExport) — enforced in Rust, not SQL (defect D-09).
- `migrations/0002_m1.sql:38-42`: `is_system()` is just `COALESCE(current_setting('boma.system_reason', true), '') <> ''` — any non-empty GUC passes.
- `docs/adr/024-operate.md:139` / `docs/STATE.md:542`: consumption of `access_grants` by a sign-in gate is deferred (M11+); today it is an unconsumed allow-list.
- Worker #324 proposal A2/A3/A5: audit append is best-effort (`let _ = …`) and revoke hard-deletes, so audit is the sole history.

## Reliability failure modes

### Grants (G)

- **G1 (HIGH, CONFIRMED):** email shape/case is checked only in Rust. A writer bypassing `Email::new` can store `Foo@x.com`; the PK treats it as distinct from `foo@x.com`. Sign-in matches lowercase byte-for-byte, so the grant never activates — and revoking the lowercase row leaves the mixed-case grant active ("access survives revocation").
- **G2 (MED, CONFIRMED):** `reason` (non-empty, <= 200 chars) and `granted_by` (a `UserId`) have no DB backstop: a typo'd reason persists, a deleted granter leaves an unresolvable string. Rejection is only on the admin path, not at the storage boundary.
- **G3 (MED, CONFIRMED):** `created_at` has a DB default but the writer passes an app-clock string cast to `timestamptz` (`admin.rs:180,186`) — two clocks. No `updated_at`, so a reason/granter edit leaves no in-table "when".
- **G4 (MED, CONFIRMED):** insert is PK + `ON CONFLICT DO NOTHING` then re-`SELECT`; a concurrent revoke between them yields no row → `Unavailable` (an outage label for a benign race), and `Conflict` can be returned for a grant legitimately revoked/re-created (#324 R2). The table offers no CAS signal.
- **G5 (LOW, CONFIRMED):** revoke is a hard `DELETE`, no in-table history. If the best-effort audit append is lost (#324 A1/A3), the grant and its revocation are both unprovable.

### RLS (R)

- **R1 (HIGH, CONFIRMED):** `is_system()` trusts any non-empty, possibly *session-scoped* GUC. One mis-scoped pooled connection gets unrestricted read/write on the deployment-wide allow-list; the closed enum is not checked in SQL (D-09).
- **R2 (HIGH, CONFIRMED):** the policy is PERMISSIVE (default); a future additive policy ORs with it and widens access. Nothing pins "system-only".
- **R3 (MED, CONFIRMED):** `FORCE RLS` binds the owner too. Migration DML with an empty `boma.system_reason` sees 0 rows, so a backfill/`VALIDATE` can pass vacuously; the migration must set the GUC or fail loudly.
- **R4 (LOW, GUESS):** no startup assertion that the pool role is neither superuser nor `BYPASSRLS`; RLS would be silently inert if that changes.

### Expiry (E)

- **E1 (HIGH, CONFIRMED):** no `expires_at`. A grant is valid forever until deleted; a time-bounded contractor/partner grant cannot be represented.
- **E2 (HIGH, CONFIRMED/GUESS):** when the M11+ gate lands (ADR-024) it will consult this table. With no expiry and with lost/erased revocations (G5), a lapsed or already-ended person's grant stays active — the exact failure the header promises one row can prevent.
- **E3 (LOW, CONFIRMED):** no retention path; granted vs revoked rows are indistinguishable in-table, so age cleanup and "who still holds a grant" audits are impossible.

## Concrete hardening (additive, prioritized; with risks)

All items land in a NEW forward-only migration (e.g. `0060_access_grants_hardening.sql`) in `BEGIN/COMMIT`, with `SET LOCAL boma.system_reason = 'DataMigration'` before any DML, and catalog guards (`to_regclass`, `pg_policies`, `IF NOT EXISTS`) so it is retry-safe (R3).

- **H1 (G1) — email contract.** `ADD CONSTRAINT access_grants_email_lower CHECK (email = lower(email)) NOT VALID`, plus a bounded shape CHECK (`length(email) BETWEEN 3 AND 320`, one `@`, dot in domain); audit, then `VALIDATE`. *Risks:* legacy mixed-case rows block VALIDATE. Backfilling `lower(email)` is a data change; two rows collapsing to one needs a dedupe decision — out of additive scope, record as DEVIATION/decision rather than silently deleting. No `UNIQUE(lower(email))` twin PK without an ADR.
- **H2 (G2/G3) — column contract.** `CHECK (btrim(reason) <> '' AND length(reason) <= 200)` and a bounded `granted_by` CHECK via `NOT VALID → VALIDATE`; `ADD COLUMN updated_at timestamptz` (nullable, no default) with a `COMMENT ON COLUMN` saying writers must set it; document `created_at` as app-supplied canonical via `boma.rfc3339`. *Risks:* legacy empty/overlong reason blocks VALIDATE; a trigger rewriting `created_at` changes stored values → DEVIATION, so keep the writer authoritative.
- **H3 (E1/E2/E3) — lifecycle columns.** `ADD COLUMN expires_at timestamptz`, `revoked_at timestamptz`, `revoked_by text` (all nullable) + comments defining the future gate predicate `revoked_at IS NULL AND (expires_at IS NULL OR expires_at > now())`; partial index `access_grants_active_expiry ON access_grants (expires_at) WHERE revoked_at IS NULL AND expires_at IS NOT NULL` for expiry sweeps. No current row's meaning changes. *Risks:* inert until the M11+ gate consumes them (GUESS); revocation stays a hard `DELETE` until the store changes — switching to tombstones changes uniqueness/INSERT semantics → ADR + DEVIATION, keep out of this migration.
- **H4 (R1/R2) — restrictive guard.** Add RESTRICTIVE policy `access_grants_system_restrict USING (current_setting('boma.system_reason', true) IN (the 8 SystemReason values)) WITH CHECK (same)` matching `SystemReason::ALL`. RESTRICTIVE ANDs with the existing PERMISSIVE policy, so no future additive policy can widen access and an unknown GUC is denied at the DB layer (D-09 for this table, without editing `is_system()`). *Risks:* GUESS — every legitimate reason must be seeded; a missed one fail-closes a system path (outage, not leak). Keep the list in lockstep with `SystemReason::ALL`, test all 8; `FORCE` means the migration's own DML needs the GUC (R3).
- **H5 (R4) — migration/ops assertions.** Fail the migration if neither `current_setting('boma.system_reason', true)` nor `boma.space_id` is set before DML (no silent 0-row validation); CI check that every `boma.is_system()`-only table has ENABLE + FORCE + a RESTRICTIVE policy; startup check that the pool role is not superuser/`BYPASSRLS`. *Risks:* the role check can break a local superuser connection → flag it or document the required role; guards must be idempotent (re-run test below).
- **H6 (G4) — tests/observability.** REQ-058-tagged integration tests: mixed-case insert rejected at DB; duplicate insert yields `Conflict`; session-scoped GUC cannot read the table; unknown reason denied by H4; a future permissive policy cannot widen past H4; expired/revoked rows excluded by the documented gate predicate; migrate twice from empty. Counter for RLS denials and audit-append failures. *Risks:* more testcontainers time; never log `email` (PII) — log a reason code only.

## Verification (proposed; NOT run — read-only)

- `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`; `sqlx migrate run` from empty and a second run; assert `0047`'s `_sqlx_migrations` checksum is unchanged.
- Replay the `req_058_*` grant tests (`crates/testkit/src/build.rs`, `crates/api/tests/rpc_surface.rs`, `apps/web/tests/admin.spec.ts`) plus H6; run the H1/H2 audit queries on a seeded copy, record offender counts in `docs/STATE.md`.

## Constraints / parity

Never edit `0047` in place; additive migration only. Keep fail-closed, system-only RLS and the transaction-local GUC contract. H1/H2 reject values the app never wrote today (tighter DB boundary, not an API change), but any dedupe/backfill or tombstone switch is externally visible → `docs/PARITY.md` + operator approval. H3/H4 are latent until the M11+ gate; record that decision in `docs/STATE.md`, mark it GUESS, and keep no PII or ids in logs.
