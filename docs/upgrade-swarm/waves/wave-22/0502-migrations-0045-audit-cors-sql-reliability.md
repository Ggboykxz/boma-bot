# 0502 — `migrations/0045_audit_cors.sql` — reliability proposal (v0.4.0)

Worker #502 · lens: reliability · READ-ONLY review of the `before-m5` tree. Proposals only; no source
changed. **Additive only** — every fix below is a new statement in a new migration; 0045 itself is
never edited (editing an applied file changes its SQLx checksum and breaks startup on deployed DBs).
**CONFIRMED** = read in this tree this session; **GUESS** = needs a runtime/Postgres test.

Anchors: `0045_audit_cors.sql:12-45`; `0030_audit_events.sql:6-22` (table, index, FORCE RLS);
`0049_password_recovery_audit.sql:3-16` and `0052_invite_accept_audit.sql:2-16` (CHECK rewrites);
`0002_m1.sql:26-42` (`current_space_id`, `is_system` GUC helpers); `crates/infra/src/tenant.rs:110-126`
(only `boma.space_id` / `boma.system_reason` are set); `crates/infra/src/stores/audit.rs:40-49,74-121`;
`crates/infra/src/db.rs:4,20` (forward-only sqlx migration); `docs/adr/024-operate.md:4-6`;
`spec/02-data-model.md:12,27`; `spec/04-nonfunctional.md:83`; `spec/05-known-defects.md:20` (D-05).

## What is already right (keep, do not re-propose)

- Widening `kind` and making `space_id` NULLable with `ON DELETE SET NULL` is exactly what makes the
  trail survive space/account deletion (024-operate ADR; `spec/02:12`). CONFIRMED.
- `WITH CHECK` still forbids a *member* from inserting a deployment-scoped (NULL-space) row: only
  `is_system()` can (0045:42). CONFIRMED.
- The whole file is one sqlx transaction (`db.rs:4,20`) and ALTER TABLE / DROP POLICY take
  ACCESS EXCLUSIVE, so there is no window with no policy. FORCE RLS from 0030 fails closed with no
  GUC. CONFIRMED (PG behaviour; add a test).

## Failure modes

### 1. Redaction / secret material at rest

- **R1 (HIGH, CONFIRMED):** 0045 touches only `kind`/`space_id`; `target text` and `metadata jsonb`
  remain unconstrained. Redaction happens only on read and only on `metadata` (wave-11 #212 FM-3/4,
  wave-14 #328 S1/S2), so any SQL reader, backup or future forwarding sink sees plaintext secrets.
  `spec/04:83` ("audit rows and exports never contain secret material") is unmet at rest. A migration
  cannot regex-redact sanely, but it is the correct place to add defence-in-depth bounds.
- **R2 (MED, CONFIRMED):** no CHECK on `metadata` shape or `target`/`metadata` size. An unbounded
  attacker-influenced value bloats an append-only table and the 4 000-row export (`audit.rs:43-49`),
  slowing reads and raising timeouts → more `Unavailable` writes (compounds #328 D1). A size CHECK
  must use `length(metadata::text)` (`pg_column_size()` is not immutable) — verify on Postgres (GUESS).

### 2. RLS (tenant isolation and audit integrity)

- **L1 (HIGH, CONFIRMED):** the policy has no `FOR` clause, so under FORCE RLS it applies to **ALL**
  commands. A member session that can SELECT/INSERT in its space can also UPDATE or DELETE those rows
  if it ever reaches raw SQL (injection or a compromised app role). "Append-only" is a store
  convention (`audit.rs:1-3`), not a database invariant — evidence can be rewritten silently.
- **L2 (HIGH, CONFIRMED):** `WITH CHECK` ends in `OR boma.is_system()` (0045:44), which makes the
  whole predicate true in system context — no cross-check of event space versus actor. `actor_user_id`
  is free text with no DB-side identity: `tenant.rs` sets only `boma.space_id` and
  `boma.system_reason`, so a writer can attribute an event to any user id. Non-repudiation is weak.
- **L3 (MED, CONFIRMED):** `USING` repeats `OR boma.is_system()` twice (0045:39-40) — redundant today,
  but a future hand-edit that drops the outer term would hide every deployment-scoped row from
  operators. Simplify when the policy is next touched.
- **L4 (MED, CONFIRMED):** `boma.current_space_id()` / `boma.is_system()` read session GUCs any SQL
  role may `SET`; a single injected `SET boma.space_id` defeats RLS on every tenant table. Cross-cutting —
  note here, fix at the connection-role layer (app role must be the only DB principal).
- **L5 (MED, GUESS):** `ON DELETE SET NULL` on space delete relies on RI triggers bypassing RLS;
  confirm with a real-Postgres test that the child UPDATE is not rejected under FORCE RLS.

### 3. Constraints (value hygiene)

- **C1 (HIGH, CONFIRMED):** the closed `kind` CHECK is copied verbatim in 0045, 0049 and 0052. Every
  new kind requires a drop-and-recreate with a *complete* current list; a stale copy silently deletes
  already-valid kinds, so those writes fail a CHECK and are reported as `Unavailable` — silent audit
  loss for the affected flows. Brittle by construction, and three migrations already prove the drift
  risk is live.
- **C2 (MED, CONFIRMED):** nothing ties `kind` to scope. Deployment-only kinds
  (`security.sign_in_failed`, `security.csp_violation`) can be inserted with a non-NULL `space_id`
  and become visible in-space, contradicting 0045's own comment (35-37).
- **C3 (MED, CONFIRMED):** `actor_user_id` has no length bound and, per 024-operate, survives user
  deletion; PII persists with no pseudonymisation path (echoes #328 R2).
- **C4 (LOW, CONFIRMED):** `id` and `target` have no length bound; no immutability trigger on the
  table; no `IF NOT EXISTS` (acceptable — the sqlx tracker prevents re-runs).

### 4. Query/index reliability

- **Q1 (MED, CONFIRMED):** the only index is `(space_id, created_at DESC)` (`0030:16`). The export
  filters `kind LIKE 'prefix%'` (`audit.rs:43`), which that index cannot serve; system/deployment
  scans grow with the append-only table. Additive index on `(space_id, kind text_pattern_ops,
  created_at DESC)` or a trigram GIN on `kind`.

## Concrete hardening — all additive, in one new migration (e.g. `0060_audit_events_hardening.sql`)

- **P0 (L1):** drop the ALL policy; add `audit_events_select` `FOR SELECT USING (...)` and
  `audit_events_insert` `FOR INSERT WITH CHECK (...)`; with no UPDATE/DELETE policy those are denied.
  Add a BEFORE UPDATE/DELETE trigger raising an exception, exempted only via a documented system
  retention function. Risk: a purge job needs a DELETE path — ship that function in the same release;
  a superuser can still disable triggers, so this is defence in depth, not a hard guarantee.
- **P0 (C1):** replace the inline CHECK with an `audit_event_kinds(kind text PK, scope text)` registry
  plus FK from `audit_events.kind`, backfilled with the current 18 kinds from 0052. New kinds become
  one INSERT, ending the copy drift. Risk: registry rows must exist before code emits them (deploy
  ordering); per-insert FK lookup is negligible. Minimum alternative if the registry is too broad:
  a CI test asserting the CHECK covers every `AuditKind::as_str()` wire value.
- **P1 (R1/R2/C4):** additive CHECKs `jsonb_typeof(metadata) = 'object'`, `length(target) <= N`,
  `length(actor_user_id) <= N`; pair with write-time redaction in the app (#328 H4, out of scope
  here). Preflight with a read-only query; bound generously. Risk: existing oversized/odd rows fail
  the migration — clean or grandfather them first.
- **P1 (C2):** add `scope` to the registry and a CHECK that deployment-only kinds carry `space_id
  IS NULL`. Risk: the mapping must be complete; a dual-scope kind needs an explicit value.
- **P1 (Q1):** add the kind-pattern index. Risk: extra write cost on the append path (low volume);
  measure before/after with the same command.
- **P2 (L2):** add a `boma.current_user_id()` GUC helper (`0002`-style), set it in `tenant.rs`, and a
  RESTRICTIVE member policy requiring `actor_user_id = boma.current_user_id()`. Risk: touches app
  write paths and a shared helper — coordinate across waves; system-context rows stay exempt.
- **P2:** record the downgrade statements (DROP POLICY/TRIGGER/CONSTRAINT/INDEX/TABLE) in docs per
  spec 04 §4.9 and add a CI gate forbidding edits to applied migrations.

## Verification (test-first; REQ-045 in the test name)

Real-Postgres integration tests: in-space UPDATE and DELETE of an audit row are denied (L1); a
system-context INSERT with a spoofed actor is rejected once L2 lands; a deployment-kind row with a
space is rejected (C2); oversized `target` rejected at write (R2); kind registry covers all wire
values (C1); space delete nulls `space_id` and keeps the row visible only to system (L5); kind-prefix
export uses the new index (Q1). Migration runs under `--locked`, all claims CONFIRMED by source
reading except items explicitly tagged GUESS; no DB/test command was run (READ-ONLY worker).

## Open questions (route to docs/STATE.md)

- Registry table in v0.4.0, or CI wire-value test as the interim answer to C1? Who owns the
  retention/purge function the append-only trigger exempts (L1)? Does `boma.current_user_id()` land
  for L2, or is it deferred? Preflight: no deployed rows with non-object `metadata` / oversized `target`.
