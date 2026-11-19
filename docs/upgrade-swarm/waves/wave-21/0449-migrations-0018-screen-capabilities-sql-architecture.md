# Worker #449 — `migrations/0018_screen_capabilities.sql` — architecture (v0.4.0)

- Worker #449 · Lens: architecture (structure, boundaries, coupling, debt) · Target: v0.4.0.
- Status: PROPOSAL, READ-ONLY. No product code or migration changed, no tests run. CONFIRMED = direct
  read this session; GUESS = inference or claim from a sibling proposal not re-verified here.
- Module: `migrations/0018_screen_capabilities.sql` (33 lines), the REQ-030 schema for the sealed
  screen capability: a bearer token whose hash is stored, resolving to (space, computer, generation).
- Read for context (CONFIRMED): `0017_computers.sql` (computer.screen_generation, RLS loop),
  `0019_computer_takeover.sql` and `0020_team_computers.sql` (additive-extension house style),
  `spec/01-requirements.md:248-253` (REQ-030), `spec/02-data-model.md:113`. Read for house style and
  cross-layer seams: wave-17 #391 (screen store), wave-19 #421/#415 (migration proposals).

## 1. Current structure (CONFIRMED)

- One table `screen_capabilities`: `token_hash bytea PRIMARY KEY`; `space_id` and `computer_id` text
  FKs with `ON DELETE CASCADE`; `screen_generation integer NOT NULL CHECK (>= 0)`; `created_at
  timestamptz NOT NULL` with no DB default. One index on `computer_id` (:16).
- One RLS block (:18-33): a `DO $$ … FOREACH tenant_table IN ARRAY['screen_capabilities'] … EXECUTE
  format(...)` enables + forces RLS and creates `screen_capabilities_tenant_isolation` with
  `USING = WITH CHECK = (space_id = boma.current_space_id() OR boma.is_system())`.
- Four architectural traits:
  1. **Revocation invariant is split across layers.** The DB stores the generation, but the
     "current" comparison against `computers.screen_generation` lives only in the Rust resolver
     (#391, GUESS here). No table, view or index represents validity, so the DB cannot answer it.
  2. **Bot provenance is implicit.** REQ-030 says the capability names bot, computer and generation,
     yet there is no `bot_id` column; "which bot" requires a join through `computers`.
  3. **RLS is hidden in dynamic SQL.** `format()` + `FOREACH` make the effective policy un-greppable
     and invisible to SQLx offline metadata; with a single table the loop is pure boilerplate.
  4. **No retention structure.** Invalidation is logical (generation bump); rows persist until an FK
     cascade removes the computer/space. No `expires_at`, no time-based index (GUESS, per #391).

## 2. Guiding principle for v0.4.0

`0018` is immutable: sqlx verifies a checksum, so editing its bytes breaks `boma migrate` on every
deployed database (CONFIRMED via #415 citing `STATE.md`). Every change ships as a NEW, additive file
(e.g. `0060_screen_capabilities_current.sql`, `0061_screen_capabilities_retention.sql`) that only
adds objects, nullable/defaulted columns or indexes — never DROP / RENAME / ALTER TYPE / DELETE or a
row rewrite. Keep the token hash format, RLS expression and generation semantics byte-compatible;
anything externally visible is a DEVIATION for `docs/PARITY.md` with owner approval.

## 3. Concrete upgrades (5), each with a risk

**U1 — Freeze `0018` and add an additive migration contract test (structure, do first).** Never edit
the file; add a fresh-PG test that runs `boma migrate`, asserts the exact `0018` shape (five columns,
PK, FK actions, CHECK, one index, enforced policy) and that a hand-edited checksum is rejected.
*Risk:* a shape assertion must be updated in the SAME commit as any new column from U4/U5.
*Mitigation:* assert shape semantically, keep the assertion in one shared constant, and regenerate
SQLx offline metadata whenever a new migration lands.

**U2 — Make RLS governance introspectable without removing the loop (governance).** In a NEW migration
add a `COMMENT` recording the effective policy, plus a `DO $$` assertion that raises unless
`pg_policies` shows `screen_capabilities_tenant_isolation` with `using = with check` and
`pg_class.relrowsecurity/relforcerowsecurity = true`; mandate static per-table `ENABLE`/`FORCE`/
`CREATE POLICY` DDL for all future tenant tables. *Risk:* `pg_policies` columns vary by Postgres
version and a test mirroring the loop can drift from it. *Mitigation:* pin to the embedded PG 17,
assert only policy name + enforced flags from one expected-policy constant, and leave `0018` and its
loop untouched.

**U3 — Represent the revocation invariant in the DB, additively (coupling).** Add a covering index
`screen_capabilities_computer_gen_idx ON screen_capabilities (computer_id, screen_generation)` and an
additive VIEW `screen_capabilities_current` joining `computers` on id and filtering
`c.screen_generation = cap.screen_generation` (and current status). This makes "valid only while the
computer is current" queryable/auditable in SQL without changing the resolver. *Risk:* the view can
drift from the Rust resolver into a second source of truth. *Mitigation:* document both from one rule,
add a parity test comparing view membership to `resolve_screen_capability`, and treat the view as a
read-only diagnostic — never authoritative.

**U4 — Add nullable `bot_id` for direct token provenance (completeness).** New migration:
`ADD COLUMN bot_id text REFERENCES bots (id) ON DELETE CASCADE` (nullable), so "which bot issued
this" needs no join. *Risk:* denormalization can drift from `computers.bot_id`, and a nullable column
cannot backfill old rows without a data change. *Mitigation:* keep it nullable, document `computers`
as authoritative, and defer any backfill to a separately reviewed step (GUESS: skip in v0.4.0).

**U5 — Add retention structure without deleting anything (lifecycle).** New migration: nullable
`expires_at timestamptz` plus a partial index on `created_at` for cleanup scans, and a header defining
ownership (a maintenance job, not the migration). *Risk:* TTL semantics duplicate the resolver's
validity rule and need an ADR; an aggressive cutoff could drop rows useful for incident review.
*Mitigation:* leave `expires_at` NULL/unused in v0.4.0 (structure only), state that purge belongs to a
scheduled job; if adopted later, use a conservative window (GUESS: 30 days) and test the exact cutoff.

## 4. Cross-cutting risks / sequencing

- Order **U1 → U2 → U3 → U4 → U5**: freeze/test, then RLS governance, then invariant structure,
  provenance, retention. Each lands as a separate additive file; U3-U5 only if their ADRs are approved.
- Coordinate with #391 (store) and #450 (reliability lens on this same file): this proposal owns only
  migration structure; resolver logic, purge scheduling and typed-time changes stay in those briefs.
- Freeze token hash format, RLS expression, FK cascade behavior and generation semantics. Regenerate
  `.sqlx` metadata; CI must run `boma migrate` on a fresh DB and replay the REQ-030 tests.
- No new dependency; work is SQL + migration-level tests. Mark every claim CONFIRMED/GUESS per
  `AGENTS.md`; gate each step with `cargo fmt --check`, clippy `-D warnings`, golden and contract suites.

## 5. Out of scope

The `ScreenCapabilityStore` port/adapter and resolver in `boma-app`/`boma-infra` (#391), error handling
and cleanup scheduling (#450), and any in-place edit of `0018` are excluded — each belongs to another
brief or is forbidden by the checksum invariant.

## 6. Evidence & suggested tests

- CONFIRMED this session: `0018` (:1-33), `0017`, `0019`, `0020`, `spec/01:248-253`, `spec/02:113`.
- GUESS (re-verify before implementing): resolver-only generation check and absent purge path (from
  #391); no FK/audit use of a bot column today; embedded PG 17 as the migrate target.
- Suggested names: `req_030_migration_0018_is_immutable_and_shape_stable` (U1),
  `req_030_screen_capabilities_rls_policy_is_enforced_and_introspectable` (U2),
  `req_030_current_view_matches_resolver` (U3).

Line budget: <120. Status: proposal only; READ-ONLY worker, no code changed and no tests run.
