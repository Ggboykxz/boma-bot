# 0505 — `migrations/0047_access_grants.sql` — architecture (v0.4.0)

- Worker #505 · Lens: architecture (structure, boundaries, coupling, debt) · Target: v0.4.0.
- Status: PROPOSAL, read-only. CONFIRMED = verified by direct reads in this worktree; GUESS = reasoned
  from code, not run against a live DB. This report is the only file written.
- Module: `0047_access_grants.sql` (17 lines) — the deployment-wide, non-tenant allow-list for REQ-058.
- Read for context (CONFIRMED): `0001_init.sql`, `0002_m1.sql` (`boma.is_system()` :38-42, closed
  `SystemReason` enum, defect D-09), `0046_account_exports.sql`, `0048_password_resets.sql`,
  `0051_lowercase_users_email.sql`, `0030_audit_events.sql`, `crates/infra/src/stores/admin.rs:133-230`,
  `crates/app/src/usecases.rs:9055-9144`, `crates/app/src/ports.rs:4285-4309`,
  `crates/domain/src/audit.rs:39-41`, `spec/01-requirements.md:473-479`, `apps/web/tests/admin.spec.ts`,
  wave-21's `0465` (space-keys DDL) and `0473` (audit-events DDL) proposals for house style.

## Current structure (CONFIRMED)

- Forward-only file, no `IF NOT EXISTS`, no down file: sqlx records the checksum, so 0047 is immutable.
- One table: `email text PRIMARY KEY`, `reason text NOT NULL`, `granted_by text NOT NULL`,
  `created_at timestamptz NOT NULL DEFAULT now()`. The PK on `email` is the only index; it already
  serves the store's `ORDER BY email ASC` (`admin.rs:142`), so there is no read-path index gap today.
- RLS: `ENABLE` + `FORCE` + `access_grants_system_only` with `USING`/`WITH CHECK (boma.is_system())`.
  `is_system()` is true only when `boma.system_reason` is set from the closed enum (0002:38-42); a
  plain member connection sees zero rows and cannot write — the correct REQ-058 deny-by-default.
- All three store operations wrap a `SystemReason::ScheduledJob` system path (`admin.rs:134,173,215`);
  writes are `ON CONFLICT (email) DO NOTHING` + Conflict, reads project exactly
  `(email, reason, granted_by, boma.rfc3339(created_at))`; revoke is a hard `DELETE` (`admin.rs:220`).
- The schema enforces none of the app's rules: lowercased `Email::new` and the 1..200-char reason are
  checked in `usecases.rs:9088-9094` only. A direct system write can store mixed-case duplicates or a
  blank reason. `granted_by` is an unlinked string — a deleted admin leaves a dangling actor.
- Revocation history lives only in `audit_events` (`AdminGrantCreated/Revoked`, `audit.rs:39-41`);
  the grant row carries no correlation id, so "who revoked this and when" needs a text join on
  `target`. There is no `updated_at`, no soft-delete marker, and no `COMMENT ON` anything.

## Guiding principle for v0.4.0

Additive only: never edit 0047 (recorded checksum), never drop, rename, retype or destructively
backfill. Every item is a NEW numbered migration (next free numbers, 0060+), a new object, a nullable
or defaulted column, or a new test. Keep the four-field wire projection, the PK, the policy name and
the `boma.is_system()` predicate byte-stable; any external delta is a DEVIATION for `docs/PARITY.md`.

## Concrete upgrades (5), each with a risk

1. **Pin the app's own invariants in the DB.** New `0060_access_grants_invariants.sql`:
   `CHECK (email = lower(email))` and `CHECK (btrim(reason) <> '' AND char_length(reason) <= 200)`,
   each added `NOT VALID`, then `VALIDATE CONSTRAINT` only after an audit `SELECT` for offenders.
   Also add `COMMENT ON TABLE`/`COMMENT ON COLUMN` stating REQ-058, "no tenant data", system-only
   access, and that revocation is a hard DELETE.
   *Risk:* a legacy mixed-case or overlong row makes VALIDATE fail; 0051 fixed `users.email` but
   nothing proves grants were normalised. *Mitigation:* run the audit SELECT first, record counts in
   `docs/STATE.md`; if offenders exist, keep the CHECK `NOT VALID` and document rather than rewrite.
   (CONFIRMED the app enforces both rules; GUESS no offending rows.)

2. **Give `granted_by` a real relationship without changing the wire shape.** In the same release add
   a nullable mirror `granted_by_user_id text REFERENCES users(id) ON DELETE SET NULL`, plus a COMMENT
   that the legacy `granted_by` text remains the projected value and the id is the audit-side truth.
   Do **not** add an FK on `granted_by` itself: it is NOT NULL, so `ON DELETE SET NULL` would require a
   shape change, and `ON DELETE RESTRICT` would block REQ-044 account deletion.
   *Risk:* two sources of truth can drift. *Mitigation:* land the writer change in the same commit,
   use `SET NULL` (never `RESTRICT`), and add a reconciliation test on seeded rows. (CONFIRMED no FK.)

3. **Correlate a grant with its audit trail.** Add nullable `created_audit_event_id text` and
   `revoked_audit_event_id text`, optionally FK to `audit_events(id) ON DELETE SET NULL` (that table is
   system-only too, so no cross-tenant read path). The writer already emits `AdminGrantCreated` and
   `AdminGrantRevoked`; recording their ids turns "who decided, when" into a join instead of a text
   match on `target`.
   *Risk:* the audit append is fire-and-forget (`let _ =`, `usecases.rs:9102,9137`) and REQ-045
   retention may purge rows, so ids can legitimately be absent. *Mitigation:* keep both columns
   nullable, use `ON DELETE SET NULL`, never project them, and add a test that a present id resolves.

4. **One explicit system-only RLS contract.** New `006N_system_only_rls_helper.sql` defining
   `boma.enable_system_only_rls(p_table text)`: `ENABLE`/`FORCE` + a `<table>_system_only` policy using
   `boma.is_system()`, written `SECURITY DEFINER` with pinned `search_path`, `format('%I')` quoting and
   `REVOKE EXECUTE FROM PUBLIC`. Future system-only tables (the 0046/0048 family) call it; 0047 itself
   stays byte-identical and is NOT rewritten to use it.
   *Risk:* a DDL-running `SECURITY DEFINER` function is a privilege-escalation surface and may collide
   with wave-21's proposed `boma.enable_tenant_rls`. *Mitigation:* pin `search_path`, quote identifiers,
   revoke EXECUTE, add a test that a member context cannot invoke it, and check for the name first
   (GUESS: no such helper exists today). This table has no `space_id`, so it must NOT be forced through
   the tenant helper.

5. **Freeze the contract with a REQ-tagged migration test.** Extend `crates/infra/tests/database.rs`
   with `req_058_access_grants_shape_and_rls_survive_migrations`: on a fresh DB assert the four columns
   and types, the email PK, RLS enabled+forced, the policy name and `boma.is_system()` predicate, then
   that a member-context SELECT returns 0 rows (D-09 deny-by-default). Regenerate SQLx offline
   metadata for every new object.
   *Risk:* `pg_catalog` introspection is version-brittle and no such test exists today. *Mitigation:*
   match by object name and stable prefix only, pin to the embedded PG used by the packs, and derive
   expectations from one shared constant. (CONFIRMED no `req_058` DB-shape test exists.)

## Sequencing and gates

Land 5 first (freeze today's shape), then 1 -> 2 -> 3 -> 4. Each item is one forward-only migration
and one commit mentioning REQ-058. Apply every new file to a fresh DB and to a copy seeded through
0047; assert row counts and the four projected values are unchanged. Run `cargo fmt --check`, clippy
`-D warnings`, a fresh-DB `migrate`, `tools/trace_check.py`, and replay `apps/web/tests/admin.spec.ts`
(its three REQ-058 cases). Keep the PK, the policy text and the four-field projection unchanged.

## Coordination / observed drift (CONFIRMED, out of scope here)

- `usecases.rs:9072` documents "never the reason" in audit, but `:9111-9113` writes `"reason": reason`
  into audit metadata. That is a doc-vs-code leak in the usecase/audit module: flag it to that worker,
  do not fix it in a migration. The COMMENTs in item 1 must not repeat the false claim.
- Admin-triggered writes use `SystemReason::ScheduledJob` (`admin.rs:134,173,215`), not an admin reason.
  A finer reason vocabulary is app scope and a possible behaviour change; leave it out of v0.4.0 DDL.
- Wave-21 `0465` item 5 proposes the tenant RLS helper; keep the two helpers separate as above.

## Out of scope

The `AdminStore` port, the three admin RPCs, the admin UI and its Playwright cases, the `Email` and
`AuditKind` domain enums, `audit_events` retention, and any in-place edit of 0047 belong to other briefs.
