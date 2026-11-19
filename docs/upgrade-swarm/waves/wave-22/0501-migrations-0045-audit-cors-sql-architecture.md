# 0501 — `migrations/0045_audit_cors.sql` — architecture (v0.4.0)

- Worker #501 · Lens: architecture (structure, boundaries, coupling, debt) · Target: v0.4.0.
- Status: PROPOSAL (read-only review; no product code, migration or other file changed). CONFIRMED =
  verified by direct reads in this worktree; GUESS = reasoned, not executed against a live DB.
- Module: `migrations/0045_audit_cors.sql` (45 lines) — REQ-045 (+ D-05): widens the append-only
  `audit_events` CHECK, makes `space_id` nullable with `ON DELETE SET NULL`, and rewrites its RLS
  policy so deployment-scoped rows (no space) can be written and exported by system reason.
- Read for context (CONFIRMED): `0030_audit_events.sql`, `0049_password_recovery_audit.sql`,
  `0052_invite_accept_audit.sql`, `0046_account_exports.sql`, `0002_m1.sql` (`boma.is_system()`,
  `boma.current_space_id()`), `crates/infra/src/db.rs` (embedded `sqlx::migrate!`, forward-only),
  `crates/infra/src/stores/audit.rs`, `crates/domain/src/audit.rs`, `crates/api/src/public.rs`,
  `crates/api/tests/http_surface.rs`, `crates/infra/tests/database.rs`, REQ-045 in
  `spec/01-requirements.md`, D-05 in `spec/05-known-defects.md`, wave-21's `0473-...audit-events-...`.

## Current structure (CONFIRMED)

- One forward-only, checksum-recorded file embedded in the binary; no down file. Editing it in place
  breaks applied databases, so every item below is a NEW file, object or test.
- Three concerns, all audit DDL: (a) `DROP` + full hand-copy of `audit_events_kind_check` (15
  literals), (b) `space_id DROP NOT NULL` + FK rebuilt as `ON DELETE SET NULL`, (c) `DROP` +
  recreate of `audit_events_tenant_isolation`.
- **The `cors` in the filename is a misnomer.** Zero SQL files mention CORS (all 59 migrations
  grepped). CORS lives correctly in the HTTP layer: `crates/api/src/public.rs:123-142` emits
  `Access-Control-Allow-Origin: *` for public JSON and `http_surface.rs:340` pins the preflight.
  This migration has no CORS coupling — the name is a governance/debt signal only.
- 0045 is the only migration that drops and recreates an RLS policy (others create policies once
  with their table). Its expression also diverges from the house shape
  `space_id = boma.current_space_id() OR boma.is_system()` used across 20+ policies: both USING and
  WITH CHECK carry a redundant `OR boma.is_system()` that subsumes the other disjuncts.
- 0049 and 0052 repeat the DROP + full hand-copy pattern to add two then one kind. Today the live
  CHECK has 18 literals and `AuditKind::as_str()` has 18 variants — in sync now, but the parallel
  wave-21 0030 proposal records "19 variants", which is precisely the drift this needs a guard for.
- The header calls the table append-only; the DDL does not enforce that (the store exposes no
  update/delete). Index remains only `(space_id, created_at DESC)` from 0030.

## Guiding principle for v0.4.0

Additive only: never edit 0030/0045/0049/0052 (checksums recorded), never drop or rewrite a column
or row, never change the wire-visible contract — kind strings, the `security.%` filter,
`ORDER BY created_at DESC, id DESC`, JSONL export shape, RLS outcomes or error text. Every delta is a
new migration, new object or new test; any externally visible change is a DEVIATION for
`docs/PARITY.md` (AGENTS.md).

## Concrete upgrades (5), each with a risk

1. **Disambiguate the misnamed module without renaming it.** Add `migrations/README.md` (a short
   number → concern index) recording that 0045 is audit-only and that CORS is an HTTP-layer setting
   in `crates/api/src/public.rs`; add the same note to the header of the next audit migration. Do
   NOT rename the file: sqlx keys on version + checksum, so a rename breaks applied databases.
   *Risk:* a future worker still files a CORS change here, or the index rots after new migrations.
   *Mitigation:* make the index a one-line-per-file table, add it to the migration-review checklist,
   and keep the note beside the 0045 line only — no behavioural change (CONFIRMED zero CORS SQL).

2. **Normalise the RLS policy to the house shape in a new migration.** A new `0060_...` file does
   `DROP POLICY` + `CREATE POLICY audit_events_tenant_isolation` with
   `USING (space_id = boma.current_space_id() OR boma.is_system())` and the same WITH CHECK, plus a
   REQ-045 test walking member / system / spaceless write-and-read cases. *Risk:* for `space_id IS
   NULL`, `space_id = current_space_id()` is NULL (not true) — the expression is equivalent only
   because `is_system()` is the other disjunct; a careless edit could hide system rows.
   *Mitigation:* prove equivalence with the matrix test before dropping the old policy, keep the
   policy name and error text, and fall back to leaving the policy verbatim with just the test if
   equivalence is not demonstrated.

3. **Freeze the kind vocabulary that 0045 established as a hand-copy pattern.** Add a reference
   table `audit_event_kinds (kind text PK)` seeded from the 18 `AuditKind::as_str()` values and a
   migration-level assertion that its set equals the live `audit_events_kind_check` literals; leave
   the CHECK untouched in v0.4.0. *Risk:* the two lists already disagree in a sibling proposal's
   notes (18 vs 19), and a future DROP + re-ADD that omits a literal rejects live writes.
   *Mitigation:* land the set-equality test first, generate future lists from the reference table,
   and keep the paired Rust-enum + SQL step documented for every new kind (no CHECK rewrite here).

4. **Pin the `space_id` / account semantics 0045 changed.** Add `COMMENT ON COLUMN` statements in a
   new migration stating that the FK is to `spaces(id)` (`SET NULL` on space deletion), that
   `actor_user_id` is still bare text with no FK (0030), and that "deleting an account" in the header
   is not the same operation as deleting a space; add a REQ-045 test asserting `space_id` is
   nullable, the FK action is `SET NULL`, and a spaceless system row survives space deletion and
   stays invisible to members. Optional follow-up: `actor_user_id` FK `NOT VALID`. *Risk:* orphan
   actor ids make a later `VALIDATE` fail, and the header's wording invites a false privacy claim.
   *Mitigation:* `NOT VALID` first, pre-flight orphan `SELECT` before any VALIDATE, and assert only
   observable behaviour (not the comment text) in the test.

5. **Index the spaceless partition 0045 introduced.** Create a partial index
   `audit_events_system_created ON audit_events (created_at DESC, id DESC) WHERE space_id IS NULL`
   for the system/export read path, keeping `(space_id, created_at DESC)` intact. *Risk:* the store
   mixes `kind LIKE 'security.%'` with the `id DESC` tiebreaker, so the planner may ignore the new
   index while write amplification rises (GUESS: benefit unmeasured). *Mitigation:* `EXPLAIN` on a
   seeded DB before/after, gate on an append benchmark, and drop the candidate from the plan if it
   is not chosen.

## Sequencing and gates

- Land 1 and 4 first (freeze naming and semantics), then 3 → 2 → 5 (vocabulary, policy, index).
  Each item is one forward-only migration or one doc/test commit mentioning REQ-045/D-05.
- Keep 0030/0045/0049/0052 byte-identical; regenerate SQLx offline metadata for every new object.
  CI migrates a fresh DB, replays REQ-045/REQ-058 goldens, and runs `tools/trace_check.py` with the
  REQ id in test names. No new dependency, no spec edit.
- Coordinate with wave-21's 0030 proposal: append-only enforcement, the full index family and the
  broad shape test are that module's scope; items 2–5 here are the 0045-specific deltas.

## Out of scope

CORS configuration itself, the `AuditStore` port and export/JSONL redaction, RLS rewrites already
shipped in 0045, and any in-place edit of an applied migration belong to other briefs.
