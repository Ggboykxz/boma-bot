# v0.4.0 architecture proposal — `migrations/0027_plugin_submissions.sql` (intake schema / REQ-034, REQ-058)

Worker #467 · Lens: architecture (structure, boundaries, coupling, debt) · Target: v0.4.0.
Status: PROPOSAL (read-only review; no code or migration changed). CONFIRMED = verified by direct read
of this worktree; GUESS = needs a test/oracle run.
Module: `migrations/0027_plugin_submissions.sql` (20 lines) — the non-tenant intake table for
anonymous plugin submissions and the single review-queue index.
Read for context (CONFIRMED): `0023_rate_limits.sql`, `0028_skills.sql`, `0030_audit_events.sql`,
`0034_connector_installs.sql`, `0035_plugin_installs.sql` (RLS shape),
`crates/infra/src/stores/plugin_submissions.rs`, `crates/infra/src/stores/admin.rs:232-299`,
`crates/api/src/public.rs:21-93`, `crates/api/src/wire.rs:1704-1745`,
`spec/01-requirements.md:279-286,473-479`, `spec/05-known-defects.md:21` (D-06),
`spec/golden/plugin-submission{,-invalid}.json`, and wave-19's `0417/0419/0421` proposals for house style.

## Current structure (CONFIRMED)

- 20 lines, forward-only: one `CREATE TABLE`, one `CREATE INDEX`; no down file, no RLS. Matches the
  sqlx `MIGRATOR` pipeline and the `req_053` idempotence gate.
- Deliberately outside row-level security, like `rate_limits` (0023): intake has no space and review
  is an admin/system concern (REQ-058). This is a correct boundary, but it has consequences below.
- `status text` has a CHECK over six values (`pending, unconfirmed, approved, rejected, applied,
  revoked`), yet only `pending -> decision` is ever exercised (`admin.rs:288-289`). `decide_submission`
  gates on `status = 'pending'` in application SQL only — no DB transition guard, and five states are
  unreachable from any current code path.
- `created_at` has `DEFAULT now()` but the writer binds the application clock
  (`plugin_submissions.rs:40-48`) and the reader formats with `boma.rfc3339` (`admin.rs:245`): two time
  sources in one aggregate; queue order depends on caller clocks.
- No review provenance: `_reviewer` is ignored (`admin.rs:278`); there is no `reviewed_at`, decision
  note or notification state, although REQ-058 requires the submitter to be notified.
- No outcome linkage: `applied`/`revoked` carry no resolved SHA, no timestamp and no FK to
  `plugin_installs` (0035) or `audit_events` (0030). The intake-to-install chain is not queryable.
- `contact` is the only PII: plaintext, never projected by `AdminPluginSubmissionView` (CONFIRMED),
  with no expiry, redaction marker or retention rule. No UNIQUE on `source` or submitter; duplicate
  submissions are representable. D-06's 10/hour/IP lives in `rate_limits` under
  `plugin-submissions:{ip}` (`public.rs:49`), not anchored here.
- Single index `(status, created_at)` matches the queue's `ORDER BY created_at DESC` (`admin.rs:248`);
  the file comment says "reviewed-newest-last", the query is newest-first — minor doc drift.

## Guiding principle

`0027` is immutable (sqlx records its checksum): every change is a NEW, additive migration adding
objects, nullable/defaulted columns or indexes. Never drop, rename, retype or destructively backfill.
Keep wire shapes, the six CHECK values, the index name and the RLS-free stance byte-compatible; any
wire-visible change is a DEVIATION for `docs/PARITY.md`.

## Concrete upgrades (5), each with a risk

1. **Add review-provenance columns.** In a new migration: `ALTER TABLE plugin_submissions ADD COLUMN
   reviewed_at timestamptz`, `reviewed_by text`, `decision_note text`, `notified_at timestamptz` — all
   nullable, no default. Supply the reviewer that `decide_submission` currently discards, so REQ-058's
   "who decided, when, was the submitter told" becomes answerable.
   *Risk:* columns added without a paired writer stay NULL forever; the lints forbid `expect` outside
   tests. *Mitigation:* land the port/store signature change in the same commit; add a not-null-when-
   terminal assertion test; keep legacy rows NULL by design.

2. **Encode the status transition guard in the DB, additively.** Add a small
   `plugin_submission_transitions (from_status, to_status)` lookup table seeded with the legal edges
   (`pending->unconfirmed|approved|rejected`, `approved->applied`, `applied->revoked`) plus a
   BEFORE UPDATE trigger that rejects other edges; keep the app's `WHERE status='pending'` as the
   first line of defence.
   *Risk:* couples the DB to the Rust enum and can reject legitimate rows or make a migration fail on
   a bad backfill (GUESS: none today). *Mitigation:* pre-flight audit `SELECT ... WHERE status NOT IN
   (...)`; derive the seed from one shared constant; ADR-gate the trigger so it can stay disabled until
   proven; never weaken the existing CHECK.

3. **Link intake to its outcome without crossing the RLS boundary.** Add nullable `resolved_sha text`,
   `applied_at timestamptz` and `audit_event_id text`. Do **not** add an FK to `plugin_installs`: that
   table is tenant-scoped with FORCE RLS (0035), and referencing it from this RLS-free table opens a
   cross-tenant read path. Reference `audit_events` (0030) by id only.
   *Risk:* status and outcome columns can drift, giving a 'revoked' row with a stale SHA.
   *Mitigation:* one transaction in the apply use case writes status + outcome + audit row; a
   reconciliation test asserts each terminal row has a consistent outcome; document the boundary.

4. **Give the contact PII a lifecycle, in the schema only.** Add nullable `contact_expires_at
   timestamptz` (or `contact_redacted_at`) and a partial index over still-pending rows past the
   threshold; document that `contact` is never returned by any API. Keep the actual purge in a
   separate scheduled job.
   *Risk:* the retention window is a product decision, and deleting contact too early breaks
   re-notification (REQ-058); a destructive purge would violate additive-only here.
   *Mitigation:* marker + queue view only in this migration; retention value via typed config with a
   safe default and an ADR / "Decisions needed" entry; a contract test asserts no projection includes
   `contact`.

5. **Pin the schema with a REQ-tagged migration test (no schema change).** Extend
   `crates/infra/tests/m1.rs` near `req_034_a_plugin_submission_round_trips_through_postgres` with
   `req_034_plugin_submissions_shape_survives_migrations`: on a fresh DB assert columns, types,
   nullability, the six CHECK values, the index name, the absence of RLS, and the presence of objects
   from items 1-4 so a later migration cannot silently remove them. Regenerate SQLx offline metadata.
   *Risk:* exact CHECK/COMMENT text is fragile to wording edits. *Mitigation:* match an ordered set or
   stable prefix; no new dependency needed.

## Sequencing and gates

- Land 5 first (prove today's shape), then 1 -> 2 -> 3 -> 4: provenance, guard, linkage, PII. Each is
  one forward-only migration and one commit mentioning REQ-034/REQ-058.
- Keep `0027` byte-identical; no drop/rename/destructive backfill, no RLS relaxation. D-06's rate
  limit stays in `rate_limits` — coordinate with that owner rather than duplicating the counter.
- Replay both `plugin-submission` goldens and the trace check; run `cargo fmt --check`, clippy
  `-D warnings`, a fresh-DB `migrate`, the contract suite and the migration tests after each step.

## Out of scope

The public POST/CORS/body-cap/10-per-hour logic (`public.rs`), the `admin.plugins.*` RPC orchestration
(`rpc.rs`), the domain status enum, ports/memory doubles, the apply/revoke use cases, and any in-place
edit of `0027` belong to other briefs in this swarm.
