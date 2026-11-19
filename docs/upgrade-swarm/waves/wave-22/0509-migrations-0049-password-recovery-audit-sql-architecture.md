# Worker #509 — `migrations/0049_password_recovery_audit.sql` — LENS: architecture — v0.4.0 proposal

- **Status:** PROPOSAL (read-only review; no source changed). CONFIRMED = verified by direct reads in
  this worktree; GUESS = reasoned or needs a DB run.
- **Scope (16 lines, CONFIRMED):** REQ-040 widens the append-only `audit_events.kind` CHECK by exactly
  two literals — `security.password_recovery_issued`, `security.password_recovery_completed` — via the
  now-standard `DROP CONSTRAINT audit_events_kind_check` → `ADD CONSTRAINT ... CHECK (kind IN (...))`
  pair, hand-copying the full 17-literal list from `0045`.
- **Additive only:** nothing below edits `0049`, `0045`, `0030` or any row. Every item is a NEW
  migration, object, column or test. Kind strings, `security.` prefix filter and RLS stay byte-stable;
  an external delta is a DEVIATION for `docs/PARITY.md` (AGENTS.md).
- **Read for context (CONFIRMED):** `0030_audit_events.sql`, `0045_audit_cors.sql`,
  `0052_invite_accept_audit.sql`, `0048_password_resets.sql`; `crates/domain/src/audit.rs:8–71`
  (`AuditKind::as_str`), `crates/app/src/accounts.rs:212–273` (issuer), `:275+` (consumer),
  `crates/testkit/src/build.rs:7999–8082` (REQ-040 goldens); sibling briefs 0473 (0030 architecture),
  0327/0328 (audit store) and `spec/01-requirements.md` REQ-040/REQ-045.

## Current position in the architecture (CONFIRMED)

`0049` is the middle link of a repeated chain: `0045` (15 literals) → `0049` (17) → `0052` (18, adds
`security.invite_accepted`). Each link drops and re-validates the same *named* CHECK and hand-copies the
entire vocabulary to append one value. Three independent sources of truth now exist: the SQL CHECK,
the 18-variant `AuditKind` enum (`audit.rs:8–45`), and raw literals in the stores/goldens. The enum is a
global superset, so between `0049` and `0052` it already admits `InviteAccepted` while the live CHECK
does not — enum and DB are **not** in lockstep at intermediate migration states. `0049` as shipped is a
historical snapshot; proposals target the current, post-`0052` table.

The two kinds are account-scoped but written with `space_id: None`, `actor_user_id: Some(user.id)`,
`target: "password_reset"`, `metadata: {}` (`accounts.rs:262–270`). Per `0045` that makes them
deployment-scoped: invisible to members, readable only by operators/system/export. The issuer appends
the token (`0048`) and the audit row through **two separate stores**, with the audit `let _ = ...await`
best-effort — so a token can exist with no matching `issued` event, and nothing in the DB correlates
`issued` to `completed` or to a `password_reset_tokens` row.

## Concrete upgrades (5), each with a risk

1. **Centralise the vocabulary instead of hand-copying it (highest debt).** Add a reference table
   `audit_event_kinds (kind text PRIMARY KEY)` seeded from `AuditKind::as_str()`, plus a CI test that
   reads the live `audit_events_kind_check` and asserts its literal set equals the table. Keep the
   CHECK verbatim in v0.4.0; generate future lists from the table. *Risk:* a hand-copied list (as in
   `0049`/`0052`) can silently drop a valid literal and reject a live write. *Mitigation:* land the
   verification test first, then a documented paired enum + SQL step for every new kind. Coordinate
   with 0473 item 2 — one shared owner, no duplicate table.

2. **Make the CHECK swap low-impact and superset-safe.** Add a migration-level assertion that the new
   literal set is a strict superset of the old (lost literal = fail), and keep the DROP/ADD inside the
   single transactional migrator. Additively explore `NOT VALID` + `VALIDATE` for future widening.
   *Risk:* `DROP CONSTRAINT` + re-add takes `ACCESS EXCLUSIVE` and rescans every row; on a live DB
   with a large `audit_events` this blocks reads/writes and can trip `with_tenant` acquire timeouts.
   *Mitigation:* run in a low-traffic window, assert superset in a test before shipping, keep the
   constraint name as the stable anchor, and `EXPLAIN`/benchmark the validation cost (GUESS: unmeasured).

3. **Give the two recovery kinds DB-enforced lifecycle linkage.** Additively add a nullable
   `password_reset_token_id text REFERENCES password_reset_tokens(id) ON DELETE SET NULL` on
   `audit_events` (plus a metadata-key convention) so `issued`/`completed` can be correlated to the
   token and to each other. Existing and non-recovery rows stay NULL. *Risk:* consuming/deleting a
   token `SET NULL`s the link, and two audit writes still are not atomic with the token write, so
   linkage is best-effort unless the app is changed. *Mitigation:* keep nullable, document the
   convention, add a REQ-040 test that a fresh issued/completed pair shares one token id; app-side
   outbox/transaction belongs to the audit-store brief (0327), not this migration.

4. **Pin and document the RLS scope decision for recovery events.** The code already chooses
   `space_id: None` → system/export-only visibility; make that canonical: a migration comment plus a
   REQ-040-named test asserting an `issued`/`completed` row with NULL `space_id` is invisible to a
   member and visible in system context/export. *Risk:* getting scope wrong either leaks sensitive
   account-security events into a space's member-readable audit, or hides them from a reviewer who
   needs them. *Mitigation:* record the choice in `docs/STATE.md` as GUESS with the simplest safe
   default (deployment-scoped, system-only), matching `0045`'s NULL-space rule; revisit only via ADR.

5. **Freeze the contract with a REQ-040 migration test.** Add
   `req_040_password_recovery_kinds_survive_widenings`: on a freshly migrated DB assert both literals
   are accepted, a bogus `security.password_recovery_failed` is rejected (SQLSTATE `23514`), both are
   present by name in the live CHECK set, and the enum's `as_str()` for the two variants matches.
   *Risk:* `pg_catalog` introspection is Postgres-version-brittle; a stricter future CHECK could reject
   a still-valid writer. *Mitigation:* assert by object/literal name only, pin the embedded PG used by
   the packs, derive expectations from one shared constant, and keep the test additive.

## Sequencing and gates

- Land 5 first (freeze shape), then 1 → 2 → 4 → 3: contract, vocabulary, lock-safety, scope, linkage.
  Each item is one forward-only migration and one commit mentioning REQ-040/REQ-045.
- Keep `0030`/`0045`/`0049` byte-identical (sqlx checksums); regenerate SQLx offline metadata for every
  new object. CI migrates a fresh DB, replays the REQ-040 goldens (`testkit:7999–8082`), and runs
  `tools/trace_check.py` (REQ id in test names). No new dependency.

## Out of scope

The `AuditStore` port, best-effort append and outbox/atomicity (0327), export/JSONL redaction, the
`AuditKind` enum edit itself, and the RLS policy rewrite already shipped in `0045` belong to other
briefs. No new audit kind is introduced in v0.4.0 without REQ/ADR sign-off.
