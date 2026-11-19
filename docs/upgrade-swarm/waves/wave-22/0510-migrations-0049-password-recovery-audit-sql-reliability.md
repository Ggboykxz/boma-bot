# v0.4.0 reliability proposal — `migrations/0049_password_recovery_audit.sql` (worker #510, lens: reliability)

- **Status:** PROPOSAL (read-only review; no source changed). CONFIRMED = verified by reading this worktree; GUESS = needs a runtime/oracle run.
- **Scope:** the 16-line `0049` constraint rewrite, read with `0030` (table/RLS), `0045` (widened list, NULL-space policy), `0048` (reset tokens), `0052` (next copy of the list), the writer `crates/infra/src/stores/audit.rs` and emitters `crates/app/src/accounts.rs:221-334`. Companion: worker #509 (architecture).
- **Constraint:** additive only — every item below is a *new* numbered migration, `COMMENT`, test, or call-site error handling; `0049`, `0045` and the kind literals stay untouched.

## Current structure (CONFIRMED — read this session)

`0049` is two statements: `DROP CONSTRAINT audit_events_kind_check`, then re-`ADD` with the **entire** literal list from `0045` plus `security.password_recovery_issued` / `security.password_recovery_completed`. `0052` repeats the same copy to add a third kind — the copy-paste cadence is real. No `-- no-transaction` marker, so `sqlx::migrate!` (`crates/infra/src/db.rs:20`) applies the pair in its default per-migration transaction (transactional default is GUESS-from-library; absence of the marker is CONFIRMED). The domain enum `crates/domain/src/audit.rs:16,18,56-57` is the real source of truth; the CHECK is a hand-maintained duplicate.

Both kinds are emitted in `accounts.rs`: "issued" at :259-271 (only for an existing address, after the token insert) and "completed" at :321-333 (after consume, revoke-all, password hash/set, session delete). Both are spaceless (`space_id: None`), actor = `user.id`, `target = "password_reset"`, `metadata = {}`, `created_at = now` from the app clock, and **both appends are `let _ = … .await` — errors are discarded**. Writer maps `(None, user)` → `TenantContext::System { ScheduledJob }` (`audit.rs:97-100`), which is exactly what `0045`'s `WITH CHECK` requires for a NULL-space row, and binds `target`/`metadata` verbatim (`:112-113`). Spec anchors: REQ-040 (`01-requirements.md:325-331`), REQ-045 (`04-nonfunctional.md:108-109`: append-only JSONL without secrets), `04-nonfunctional.md:83` ("audit rows and exports never contain secret material"), `02-data-model.md:27`.

## Failure modes

### Redaction (S)

- **S1 (HIGH, CONFIRMED):** the CHECK constrains only `kind`; nothing at the DB level stops a future writer from putting the raw reset token, the printed link, or the token hash into `target`/`metadata` for these two kinds. Today the code writes constants (safe), but the security audit becomes a secret-at-rest sink the moment call-site code drifts — contradicting `04-nonfunctional.md:83`. Same containment gap as worker #474 (S1).
- **S2 (MED, CONFIRMED):** `actor_user_id` holds `user.id` today (good), but it is plain text with no FK; an email or other PII is representable and survives account deletion. These rows are NULL-space, so the member-facing redaction/export path never touches them — leak detection is Postgres/operator-only.
- **S3 (MED, CONFIRMED):** `BOMA_AUTH_PRINT_RESET_LINK=true` writes the raw link and user id to stdout (`accounts.rs:248-258`). The audit row is clean, but responders may treat it as the complete record; the two channels are easy to conflate. The migration cannot fix the log sink — document the boundary.

### RLS / tenancy (T)

- **T1 (HIGH, CONFIRMED structural; GUESS exploitability):** `0049` does not touch RLS; protection is entirely inherited from `0045`. That policy hides NULL-space rows from members and permits their insert only under `boma.is_system()` — so the writer's `(None, user)` → System mapping is load-bearing. Nothing per-kind ties `security.password_recovery_*` to system-only/NULL-space: a future emitter that supplies a `space_id` exposes a security event to members, or, in a member context, is refused and then silently dropped (C1).
- **T2 (MED, CONFIRMED structural):** `list_for_space` (`audit.rs:41-45`, `kind LIKE`) can never return these rows — 0045's `USING` excludes NULL-space for members. A "security audit" built on the product API reports zero recovery events with no error, indistinguishable from "none happened".
- **T3 (LOW, CONFIRMED):** the DROP+ADD has no constraint window only while applied through the migrator (see transaction note above). A manual `psql` run, or a future `-- no-transaction`, leaves a failed ADD with **no** kind CHECK at all. Re-applying `0049` out of order would also erase `security.invite_accepted` from `0052`.

### Completion (C)

- **C1 (HIGH, CONFIRMED):** both appends are best-effort (`let _ = …`, `accounts.rs:259,321`). A rejected insert — stale CHECK missing these kinds, RLS `WITH CHECK` failure, transient DB error — is swallowed: the password reset succeeds and no audit row exists. The migration widens acceptance but cannot make the write durable; loss is silent.
- **C2 (HIGH, CONFIRMED structural):** the list is duplicated from `0045` and restated again in `0052`. Any future widening migration regenerated from an older base that omits the two recovery kinds *narrows* the CHECK: existing rows survive, but every future `issued`/`completed` write starts failing, and C1 converts that into silent data loss. This is the most likely regression for this file.
- **C3 (MED, CONFIRMED):** no invariant pairs issued↔completed. One-shot consumption lives in `password_resets` (`0048`, `consume` UPDATE); a completed without an issued, or vice versa, is representable and undetectable through the member API (T2). Only a system/Postgres reader can correlate by actor + time.
- **C4 (LOW, CONFIRMED):** the only audit index is `(space_id, created_at DESC)` (`0030:16`); it cannot serve `kind LIKE 'security.%'` over NULL-space rows. Growth → slower system lookups → more timeouts → more swallowed appends (C1).

## Concrete hardening (prioritized, all additive)

1. **P0 — freeze the literal set (C2/T3):** add `req_040_audit_kind_check_covers_recovery_kinds` in `crates/infra/tests/database.rs`: read `pg_get_constraintdef('audit_events_kind_check')` and assert the literal set equals `AuditKind::as_str()` over the whole domain enum, including `security.invite_accepted` from `0052`. *Risk:* test-only; needs an enum iterator or a frozen list, and must be updated whenever a kind is added — that is the intended tripwire. Same mechanism proposed by #474.
2. **P0 — stop swallowing the two appends (C1):** in `accounts.rs:259,321`, propagate `append` errors (fail the reset with a typed error) or persist to a transactional outbox drained by a job, so the security event shares the operation's fate. *Risk:* a DB hiccup now fails a reset that used to succeed — externally visible; needs `docs/PARITY.md`, an ADR and operator sign-off. Does not modify `0049`.
3. **P1 — assert system-only placement (T1/T2):** add `COMMENT ON COLUMN audit_events.kind` stating these two kinds are spaceless and system-written, plus tests that a member-context insert of either kind is refused, `boma.is_system()` with `space_id NULL` succeeds, and `list_for_space` never returns them. *Risk:* metadata/test only; cannot defend GUC `SET boma.system_reason` or a superuser (residual identical to #474 T1).
4. **P1 — payload containment (S1/S2):** new migration adds `CHECK (octet_length(target) <= <cap>)`, `CHECK (octet_length(metadata::text) <= <cap>)`, and for these kinds `CHECK (NOT (metadata ?| ARRAY['token','token_hash','link','recovery_link']))`. *Risk:* rejects legitimate future metadata keys — calibrate caps/keys, record rejections in `docs/PARITY.md`; a denylist cannot match unknown secret shapes, so treat it as a speed bump, not a guarantee. No at-rest data is rewritten.
5. **P1 — query path for security kinds (C4/T2):** `CREATE INDEX audit_events_security_kind_created ON audit_events (kind, created_at DESC) WHERE space_id IS NULL` (or `WHERE kind LIKE 'security.%'`) so system lookups of recovery events do not seq-scan. *Risk:* extra write cost on every append; measure, and deconflict with the index proposed in #473/#474 before coding.
6. **P2 — keep the rewrite atomic and documented (T3/C2):** never apply this file with `-- no-transaction` or outside `boma migrate`; note in `docs/STATE.md` that any future `kind` migration must be generated from the latest list and gated on item 1. *Risk:* process change only; no code effect.

## Additive-only guarantee

Items 1, 3, 6 add tests/metadata/process; items 4-5 add `CHECK`s and an index; item 2 changes call sites outside the migration. Nothing rewrites `0049`, permanently drops a constraint, mutates existing rows, changes a column type, or alters the `0045` RLS policy. Redaction hardening constrains future writes only. Item 2 is the sole behavior change and is explicitly gated on `docs/PARITY.md` + ADR.

## Verification to add

Real Postgres, RLS forced: (a) oversized `target`/`metadata` and secret-named metadata keys rejected (S1/S2); (b) member insert of both recovery kinds refused, system NULL-space insert succeeds, member `list_for_space` returns none (T1/T2); (c) fresh DB and a copy of the current schema both migrate, existing row counts/`created_at` unchanged, `pg_get_constraintdef` lists all 17+1 kinds (C2/T3); (d) with item 2 coded, an injected audit-store failure fails the reset and leaves no half-completed state (C1); (e) EXPLAIN shows item 5's index used for a `security.%` lookup (C4). All claims CONFIRMED from source reading except those marked GUESS. Status: proposal only — no file outside this report was modified.
