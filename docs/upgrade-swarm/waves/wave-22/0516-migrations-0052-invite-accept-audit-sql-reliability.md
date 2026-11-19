# v0.4.0 reliability proposal — `migrations/0052_invite_accept_audit.sql` (worker #516, lens: reliability)

- **Status:** PROPOSAL (read-only review; no source changed). CONFIRMED = verified by reading this worktree; GUESS = needs a runtime/oracle run.
- **Scope:** `0052_invite_accept_audit.sql` (16 lines) — the drop-and-recreate of `audit_events_kind_check` adding `security.invite_accepted` (REQ-042), read with `0030_audit_events.sql`, `0045_audit_cors.sql`, `0049_password_recovery_audit.sql`, writer `usecases::invite_accept` (`crates/app/src/usecases.rs:8874-8954`), `PostgresAuditStore::append` (`crates/infra/src/stores/audit.rs:74-122`) and `AuditKind` (`crates/domain/src/audit.rs:8-71`).
- **Constraint:** additive only. Fixes are **new** numbered migrations (current max `0059`, so e.g. `0060_invite_accept_audit_hardening.sql`), `COMMENT`s and tests; nothing rewrites 0052 or earlier files.

## What is CONFIRMED here

0052 copies the full `0049` literal set verbatim and appends one value — the 17 prior kinds are all still present, so there is no *present* completeness gap (CONFIRMED by line-by-line comparison). The writer emits `AuditKind::InviteAccepted` with `space_id = invitation.space_id`, `actor_user_id = caller`, `target = invitation.id`, `metadata = {"role": ...}` — no token or email (usecases.rs:8941-8949). RLS is untouched by 0052; the 0045 policy still applies.

## Failure modes

### Audit completeness (highest risk for this file)

- **C1 (HIGH, CONFIRMED):** the append result is discarded — `let _ = app.stores.audit.append(...).await;` (usecases.rs:8938-8950). Any failure (DB unavailable, RLS rejection, CHECK violation) leaves the acceptance unaudited while the RPC still returns `Ok`. There is no retry, no outbox, no log.
- **C2 (HIGH, GUESS exploitability; CONFIRMED structure):** rollout ordering trap. The domain enum and writer already emit `security.invite_accepted`, but the CHECK rejects that value until 0052 is applied. If code ships first, **every** accept append fails and is swallowed by C1 — a silent, unrecoverable audit hole for the whole window.
- **C3 (MED, CONFIRMED):** hand-copied closed literal list in 0030/0045/0049/0052. A future widening that accidentally omits a prior kind silently breaks that writer; because of C1 the loss is invisible. 0052 itself is correct today, but the pattern has no test.
- **C4 (MED, CONFIRMED):** no atomicity across membership insert, `invitations.accept` flip and audit append (usecases.rs:8903-8950, three store calls). A crash after the flip loses the audit row; `id` is caller-generated with no UNIQUE on `(kind, target)`, so replays/already-member accepts can write duplicate `invite_accepted` rows (the `accept` bool is also discarded at 8937). Audit counts therefore need not equal acceptances.
- **C5 (LOW, CONFIRMED):** the DROP + ADD sit in one migration transaction, so the constraint is never briefly missing *within* that transaction; but the migration is not idempotent — a re-run after partial failure errors on ` DROP CONSTRAINT`. Acceptable for versioned migrations, note it.

### Redaction

- **R1 (LOW, CONFIRMED):** this event carries no secret-shaped material: `target` is an opaque invitation id, `metadata` only a role, and tokens are stored hashed elsewhere. Exposure specific to 0052 is small.
- **R2 (MED, CONFIRMED structural):** redaction is export-only, walks `metadata` only and leaves `target` unchanged (`redact_audit_value`, domain/audit.rs:97-117; export usecases.rs:8561-8562). If a future edit adds an email/token to `target` or a non-denylisted key, it persists at rest and exports verbatim; the denylist (audit.rs:79-81) matches ~10 prefixes. No DB-level containment and no length caps on `target`/`metadata` (overlaps 0474 S1/S2).
- **R3 (LOW, GUESS):** `actor_user_id` is the joining user id, no FK; if a caller ever binds an email instead it survives account deletion unredacted.

### RLS / tenancy

- **T1 (CONFIRMED, low for this file):** the writer uses `TenantContext::Member { space_id, user_id }` (both `Some`), so `WITH CHECK (space_id = boma.current_space_id() ...)` of 0045 is satisfied — 0052 adds no new RLS surface.
- **T2 (MED, CONFIRMED structural):** any accept path that becomes actorless/spaceless falls into the `System` branch (audit.rs:93-100) and needs `boma.is_system()`; an unscoped write is rejected and, via C1, silently lost. The policy rests on GUCs (`current_setting(..., true)`); `FORCE RLS` does not stop a superuser — connection-role concern, outside this file.
- **T3 (LOW, GUESS):** a member can insert `actor_user_id` freely (no FK); wrong-actor rows are representable if the app path drifts.

## Concrete hardening (prioritized; all additive)

1. **P0 — close C2/C1 without a user-visible failure.** Release gate: apply 0052 *before* the writer build (record in `docs/STATE.md`). Add a new table `audit_outbox (id text PK, payload jsonb, created_at timestamptz, attempts int)` plus a store method that enqueues in the same transaction as the membership/accept write and a drained retry; keep `append` as the fast path but never discard a permanent failure — log at `error` with `kind`/`id`. *Risk:* a new table + job is scope; alternatively return the append error, which changes REQ-042 behavior on audit outage (DEVIATION → `docs/PARITY.md` + approval).
2. **P0 — lock the literal set (C3).** Real-Postgres test reads `pg_constraint` for `audit_events_kind_check` and asserts its literal set equals the set of `AuditKind::as_str()`, named `req_045_audit_kind_check_matches_domain_enum`; run it in CI on a fresh DB built from all migrations. Add a `COMMENT ON CONSTRAINT` noting every widening must copy the previous set. *Risk:* test-only; catches drift at CI, not at runtime.
3. **P1 — exactly-once accept audit (C4).** `CREATE UNIQUE INDEX CONCURRENTLY audit_events_invite_accept_once ON audit_events (target) WHERE kind = 'security.invite_accepted';` — `target` is the invitation id, flipped exactly once. *Risk:* pre-existing duplicate rows abort creation (count first); it also suppresses the deliberate audit of already-member accepts (usecases.rs:8934-8936) — decide explicitly and record, or drop to a non-unique index plus a count test.
4. **P1 — shape guard for this kind (R2/R3).** `COMMENT ON COLUMN audit_events.target` / `...metadata` stating this kind must contain only an invitation id and a role; if enforcement is wanted, an additive `CHECK (kind <> 'security.invite_accepted' OR (jsonb_typeof(metadata) = 'object' AND metadata - 'role' = '{}'::jsonb))`. *Risk:* a CHECK rejects any future metadata field and needs another migration to relax — prefer COMMENT + test unless the security gate demands enforcement.
5. **P1 — bound payloads (R2):** additive `octet_length` CHECKs on `target`/`metadata` as proposed in 0474 item 1; calibrate caps from observed rows. *Risk:* rejects long legitimate values; record any rejection in `docs/PARITY.md`.
6. **P2 — RLS regression tests (T1/T2/T3):** assert a cross-space accept append is refused under a member context, that a system-context append can write the row, and that an unscoped read returns nothing; name with REQ-045/REQ-043. *Risk:* test-only; cannot defend against GUC `SET`.

## Additive-only guarantee

Items 1-5 add a table, `COMMENT`s, named `CHECK`s and a partial index; item 6 is tests. No `DROP`, no column type change, no destructive `UPDATE`, no back-fill, no rewrite of 0052. Applied-migration hash chain is preserved by new numbered files. DB-level redaction that mutates at-rest values is deliberately left as a proposal needing an ADR and `docs/PARITY.md` entry, per 0474.

## Verification

Fresh-DB apply of all migrations + a copy of the current schema: assert kind set matches the enum (item 2), accept writes exactly one row with correct kind/space/actor/target (C1/C4), duplicate/replay index behavior (item 3), no oversized or malformed payload accepted (items 4-5), and RLS cases (item 6). Simulate a failing audit store to confirm the outbox/error path (C1). Compare row counts and `created_at` before/after the new migrations. All statements above are CONFIRMED from source reading except those marked GUESS. Status: proposal only — no file outside this report was modified.
