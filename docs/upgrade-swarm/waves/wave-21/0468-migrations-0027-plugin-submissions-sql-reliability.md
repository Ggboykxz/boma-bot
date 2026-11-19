# Wave 21 / Worker 0468 — `migrations/0027_plugin_submissions.sql` (LENS: reliability)

Proposal for v0.4.0. Additive only: every change below *adds* columns, constraints, indexes,
policies or comments; nothing already applied is rewritten, dropped or destructively back-filled.

## Current structure (CONFIRMED — read this session)

`0027_plugin_submissions.sql` is a 20-line forward-only `CREATE TABLE` plus one index. It stores an
anonymous plugin submission: `id text PRIMARY KEY`, `source text NOT NULL`, nullable `name`,
`contact`, `submitter_user_id`, `status text NOT NULL DEFAULT 'pending'` with a CHECK over six
values (`pending, unconfirmed, approved, rejected, applied, revoked`), and `created_at
timestamptz NOT NULL DEFAULT now()`. Index: `plugin_submissions_status_created (status, created_at)`.
Header deliberately places the table outside RLS "like `rate_limits`". There is no `updated_at`,
no reviewer/audit column, no FK, no length/format constraint, no uniqueness, no retention path.

Related surface read this session (CONFIRMED):
- `spec/01-requirements.md` REQ-034: source is validated before any fetch; malformed source is
  rejected with a reason; operator approves/rejects before install. `TRACE.md` marks this
  requirement as documentation-only (Cd).
- `spec/golden/plugin-submission.json`: cookieless, CORS-open POST, 10/hour/IP; reply is
  `{ ok, id }` and carries nothing about the submitter. `plugin-submission-invalid.json`:
  a ref that cannot be parsed must not be resolved or cloned.
- `spec/05-known-defects.md` D-06: unauthenticated cross-origin POST to plugin submissions
  (low) — "require an account or a challenge, and use the shared counter from D-01".
- `0023_rate_limits.sql`: shared counter `rate_limits` has *lazy cleanup*; this table has none.
- `0022_pending_tool_calls_status_check.sql`: precedent for adding a named status CHECK.
- `0035_plugin_installs.sql`: the install-side table is `FORCE ROW LEVEL SECURITY` with tenant
  policy, has `updated_at` and `UNIQUE (space_id, source)` — the submission table has none of these.
- RLS convention (CONFIRMED across `0047_access_grants.sql`, `0048_password_resets.sql`):
  non-tenant admin tables use `USING (boma.is_system())` with `ENABLE` + `FORCE`.

## Reliability failure modes

**Constraints.** No bound on `source`, `name` or `contact` text; an anonymous, CORS-open endpoint
feeding an unbounded column is a storage-exhaustion and queue-flood vector (D-06, no DB backstop).
No uniqueness on `source` or on a submitter key, so retries/duplicate POSTs allocate new ids and
inflate the review queue. `id` has no format constraint (golden shows `sub_…`), so a bad generator
can insert untracked keys. `submitter_user_id` has no FK and no relation to `contact`.

**RLS.** The header deliberately exempts the table from RLS. The only guarantee protecting `contact`
(PII) is application discipline — "never returned by any API" is not enforced by the database.
A single `SELECT *` regression in any admin path reads every submitter's address; unlike the
system-only tables above, there is no DB-level boundary. Because `FORCE ROW LEVEL SECURITY` is
absent, the table owner bypasses even a future policy unless forced.

**Status values.** The six-value CHECK constrains the vocabulary but not the *transition graph*:
`rejected → approved`, `applied → pending`, or any other jump is accepted, so a buggy admin action
silently corrupts state. `unconfirmed` has no exit deadline — a submission can sit there forever
with no detectable stuck state. With no `updated_at`, `reviewed_at` or `reviewer_user_id`, queue
age and who changed a status are unobservable, and no idempotency gate stops a double review.

## Concrete hardening (additive only, with risks)

1. **Bounded text CHECKs.** Add named `CHECK`s: `length(source) BETWEEN 1 AND 2048`,
   `length(contact) <= 320` (and `name <= 200`), `id LIKE 'sub\_%'`. Add `NOT VALID`, audit for
   offenders, then `VALIDATE CONSTRAINT`.
   - Risk: medium — a legacy long/oddly-prefixed row blocks validation. Mitigation: keep bounds
     generous, audit first, and record any offender as DEVIATION rather than deleting or truncating.

2. **Retention support via index + `expires_at`.** Add `expires_at timestamptz` (nullable) and a
   standalone `CREATE INDEX plugin_submissions_expires ON plugin_submissions (expires_at)`. The
   existing `(status, created_at)` index cannot serve a `created_at < cutoff` cleanup scan
   (leftmost key is `status`), so a scan-only index is the additive way to make retention cheap; a
   scheduled DELETE is a separate proposal, not part of this migration.
   - Risk: low — extra index write cost on an insert-light table. GUESS: the actual retention window
     is unspecified; leave the column nullable until REQ-034/ops defines the SLA.

3. **Audit/timestamp columns.** Add `updated_at timestamptz NOT NULL DEFAULT now()`,
   `reviewed_at timestamptz`, `reviewer_user_id text`, `review_note text`, mirroring `0035`'s
   `updated_at`. Makes queue age and review ownership observable without a schema rewrite.
   - Risk: low — defaulted column is cheap; no trigger implies `updated_at` is only as good as the
     writer. Mitigation: document in a `COMMENT ON COLUMN`, and add a trigger only in a follow-up.

4. **Documented status state machine.** Add `COMMENT ON COLUMN plugin_submissions.status`
   enumerating legal transitions (`pending → unconfirmed/approved/rejected`,
   `unconfirmed → approved/rejected`, `approved → applied`, `applied → revoked`) and the meaning of
   each value. Behaviour-free, closes the Cd ambiguity in `TRACE.md`.
   - Risk: low — risk is a wrong graph. Mark GUESS until confirmed against the review/adapters;
     do not add an enforcing trigger yet, since it could block a legitimate legacy transition.

5. **Optional transition constraint/trigger (deferred).** If item 4 is confirmed, a follow-up
   migration may add a `BEFORE UPDATE` trigger rejecting illegal transitions, introduced beside a
   backfill audit.
   - Risk: high — triggers change behaviour and can strand rows mid-review. Gate on confirmation;
     keep out of v0.4.0 unless REQ-034 review flow is pinned down.

6. **Restrictive RLS with an insert carve-out.** `ENABLE` + `FORCE ROW LEVEL SECURITY`, then add
   policies: `SELECT/UPDATE/DELETE` only `USING (boma.is_system())`, and an `INSERT` policy
   `WITH CHECK (true)` granted to the anonymous/web role so cookieless submission keeps working.
   This gives PII a DB-level backstop matching `0047`/`0048` while preserving the golden POST.
   - Risk: high — getting the role grant or policy order wrong breaks the anonymous submit path,
     and `FORCE` applies the policy to the owner too. Mitigation: apply inside a transaction with
     the golden `plugin-submission.json` POST as a regression gate; roll back if it fails. If the
     web role cannot be isolated cleanly, prefer a `SELECT`-only restrictive policy over enabling
     full RLS.

7. **Idempotency key.** Add `idempotency_key text` and a partial `UNIQUE (idempotency_key)` where
   non-null, so a retried POST does not create a second queue row (a DB-level complement to D-06).
   - Risk: medium — a `UNIQUE` constraint cannot be added `NOT VALID` on older PostgreSQL and
     fails if duplicates already exist. Mitigation: pre-audit; backfill keys only for new rows,
     leave legacy rows null, and keep the constraint partial.

## Additive-only guarantee

Items 1/4 are comments/constraints; 2/3/7 add columns or indexes; 6 adds policies. No `DROP`, no
`ALTER COLUMN … TYPE`, no destructive `UPDATE`. New constraints go in separate numbered files
(e.g. `0060_plugin_submissions_contract.sql`, `0061_plugin_submissions_rls.sql`) rather than editing
`0027_plugin_submissions.sql`, preserving the applied-migration hash chain.

## Verification before merge

- Apply all migrations to a fresh DB and to a seeded copy of the current schema; assert existing
  rows are unchanged after each new migration.
- Run the audit queries from items 1/7; record offender/duplicate counts in `docs/STATE.md`.
- Replay the golden `plugin-submission.json` (200 + `{ ok, id }`) and
  `plugin-submission-invalid.json` after item 6 — an anonymous POST must still succeed.
- Mark each claim here CONFIRMED or GUESS per workspace rules (items 2/4 are GUESS as noted).

Status: proposal only — no file outside this report was modified.
