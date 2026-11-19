# 0515 — `migrations/0052_invite_accept_audit.sql` — architecture (v0.4.0)

- Worker #515 · lens: architecture (structure, boundaries, coupling, debt) · target: v0.4.0.
- Status: PROPOSAL (READ-ONLY review; no code, migration or doc outside this file changed). Claims are
  CONFIRMED when read directly in this worktree, GUESS when inferred without a live DB or test run.
- Module: `migrations/0052_invite_accept_audit.sql` (16 lines) — REQ-042: admit the one kind the invite
  flow never wrote, `security.invite_accepted`, by DROP + recreate of `audit_events_kind_check`.
- Read for context (CONFIRMED): `0030_audit_events.sql`, `0045_audit_cors.sql`,
  `0049_password_recovery_audit.sql`, `0050_invitation_inviter.sql`, `0002_m1.sql` (hashed invite
  token), `crates/domain/src/audit.rs` (`AuditKind`, 19 variants, `as_str()` :50–65),
  `crates/infra/src/stores/audit.rs`, `crates/infra/src/stores/invitations.rs`, wave-21's
  `0473-...0030-audit-events-sql-architecture.md` and wave-15's `0349-...invitations-rs-architecture.md`.

## Current structure (CONFIRMED)

One forward-only file in the sqlx `MIGRATOR` chain (no down file; checksum recorded). It is the third
member of a repeated pattern — `0045` → `0049` → `0052` — each of which drops
`audit_events_kind_check` and re-adds it with the **entire** literal list hand-copied plus one new
value. `0052` adds `security.invite_accepted` to a 19-literal set; the Rust enum `AuditKind` carries the
same 19 wire values but in a different textual order (password-recovery vs. sign-in/CSP) — a second,
independent source of truth that happens to agree today. The new kind is space-scoped: an accept joins
a space, so its row should carry `space_id` and `actor_user_id`, unlike the deployment-scoped
`security.sign_in_failed`/`csp_violation` rows made nullable by `0045`. The writer is outside this
module (`invite_accept` in `crates/app/src/usecases.rs`), so the migration only permits the value; it
does not guarantee a row is emitted. As shipped, `0052` is a snapshot, not the live schema contract.

## Guiding principle for v0.4.0

Additive only: never edit `0030`/`0045`/`0049`/`0052` (checksums are recorded), never drop, rename or
rewrite a column or row. Every item below is a NEW migration, object or test. Keep the wire-visible
contract — the 19 kind strings (including the exact `security.invite_accepted` spelling), the
`security.` prefix filter, RLS semantics, `security.`-scoped export shape and error text — byte-stable.
Any external delta is a DEVIATION for `docs/PARITY.md` (AGENTS.md), not a silent change.

## Concrete upgrades (5), each with a risk

1. **Break the hand-copy chain with a canonical vocabulary (new migration + test).** Add a reference
   table `audit_event_kinds (kind text PK, since_migration text, req_id text)` seeded with all 19
   literals, and a migration-level test asserting the live `audit_events_kind_check` set equals the
   table. Leave `0052`'s CHECK verbatim in v0.4.0; future kinds are generated from the table and paired
   with a Rust enum change. *Risk:* regenerating the CHECK by hand can silently drop a valid literal
   and reject a live write (exactly the defect wave-21 flagged). *Mitigation:* land the verification
   test first, then generate; no constraint rewrite in this release. Overlaps wave-21 #0473 item 2 —
   de-duplicate by implementing it once there and cross-referencing here.

2. **Pin the new literal with a REQ-042-named contract test (new test).** Add
   `req_042_invite_accepted_kind_in_check_and_enum` in `crates/infra/tests/database.rs`: on a freshly
   migrated DB query `pg_get_constraintdef('audit_events_kind_check')`, assert it contains
   `security.invite_accepted` exactly once, and assert the set equals `AuditKind::as_str()` collected
   at runtime — proving SQL and Rust stay in lockstep. *Risk:* parsing `pg_get_constraintdef` text is
   Postgres-version sensitive and brittle. *Mitigation:* assert by substring/prefix set-equality, pin
   to the embedded PG version used by the test packs, derive expectations from one shared constant.
   (CONFIRMED the two sources agree today; untested that a future drift is caught.)

3. **Make the REQ-042 audit pair atomic and observable (new integration test, not SQL alone).** `0050`
   records the inviter and `0052` permits the accept-audit kind, but they are separate migrations and
   the accept flow writes membership + audit across two stores (wave-15 flagged the accept as
   non-atomic). Add a REQ-042-tagged test asserting that, on accept, exactly one
   `security.invite_accepted` row is written with non-null `space_id`/`actor_user_id` and the inviter
   from `0050` is still resolvable. *Risk:* the write is an app-layer change outside SQL's reach, and
   the in-memory test store may not mirror DB RLS `WITH CHECK`, masking a member-context write failure.
   *Mitigation:* assert against the real migrated DB, keep the test read-only on schema, and route any
   atomicity fix through the invitation-store owner (wave-15 #0349) rather than this migration.

4. **Guard the hardcoded constraint name against future renames (new migration helper).** `0052`
   `DROP CONSTRAINT audit_events_kind_check` names the constraint literally, coupling it to `0045` and
   `0049`; a future rename upstream would break this migration at apply time. Additively provide a
   guarded helper (a `DO $$ ... IF EXISTS ... $$` block or a documented `CREATE OR REPLACE FUNCTION`
   in the `boma` schema) that future kind-adding migrations call, so the DROP/ADD pair is tolerant of
   a missing/renamed constraint while producing an identical final definition. *Risk:* a `DO` block
   can mask a real drift and silently skip the widen, leaving a valid write rejected; changing the
   statement also invalidates the recorded checksum if applied to `0052` itself. *Mitigation:* never
   edit `0052` — land the helper as a NEW object, keep the final constraint definition byte-identical,
   and have upgrade 2's test assert the post-migration set. (CONFIRMED name is hardcoded; GUESS on any
   upstream rename currently planned.)

5. **Document scope lineage for the new kind (new registry rows / doc, no schema rewrite).** Add to the
   `audit_event_kinds` table (upgrade 1) a `scope` column marking `security.invite_accepted` as
   space-scoped and `security.sign_in_failed`/`csp_violation` as deployment-scoped, so a reader knows
   the accept row must carry `space_id` and the failure rows may not. This makes the D-05/REQ-045 RLS
   exception in `0045` explicit for the REQ-042 addition. *Risk:* a second classification that can
   drift from the RLS policy and the domain type. *Mitigation:* treat the registry as documentation
   only (not enforced by RLS), derive it from the existing policy, and include a test that the scope
   annotation matches `0045`'s policy predicate. (CONFIRMED the two scopes exist; GUESS that any
   writer currently mislabels the accept row.)

## Sequencing and gates

- Land 2 first (freeze/lockstep test), then 1 → 5 → 4 → 3 (vocabulary, lineage, guard, atomicity).
  Each item is one forward-only migration or one test, with a commit mentioning REQ-042/REQ-045.
- Keep `0030`–`0052` byte-identical; regenerate SQLx offline metadata for every new object. CI
  migrates a fresh DB, replays the REQ-042/REQ-045 goldens and runs `tools/trace_check.py`.
- De-duplicate with wave-21 #0473 (vocabulary table, append-only trigger, index and shape work are
  that module's scope) and coordinate upgrade 3 with wave-15 #0349 (invitation store). No new dependency.

## Out of scope

The `AuditStore` port, RLS policy rewrites already shipped in `0045`, the `invite_accept` control flow,
and any in-place edit of `0052` or its predecessors belong to other briefs. No new feature outside the
spec; such ideas go to `docs/STATE.md` as proposals.
