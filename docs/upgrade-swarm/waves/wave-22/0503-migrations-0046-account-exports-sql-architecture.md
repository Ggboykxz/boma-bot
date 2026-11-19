# 0503 — `migrations/0046_account_exports.sql` — architecture (v0.4.0)

- Worker #503 · Lens: architecture (structure, boundaries, coupling, debt) · Target: v0.4.0.
- Status: PROPOSAL, READ-ONLY; no migration or product code changed, no tests run. CONFIRMED =
  verified by direct reads in this worktree; GUESS = inference or a claim from a sibling brief.
- Module: `migrations/0046_account_exports.sql` (21 lines): the REQ-044 account-export job ledger.
- Read for context (CONFIRMED): this file; `0030_audit_events.sql`, `0036_artifacts.sql`,
  `0045_audit_cors.sql` (migration house style); `crates/infra/src/stores/account_export.rs`,
  `crates/app/src/ports.rs:700-754`, `crates/app/src/account_export.rs:260-345`;
  `spec/01-requirements.md:356-361`, `spec/04-nonfunctional.md:135-139`,
  `spec/02-data-model.md:170-181`, `spec/golden/export-start.json`; wave-14 #317 (store lens) and
  wave-21 #473 (migration lens). GUESS items are attributed there, not re-verified by execution.

## 1. Current structure (CONFIRMED)

- One table `account_exports` (:5-13): `id text PK` (app-generated `exp_` id), `user_id text NOT NULL
  REFERENCES users ON DELETE CASCADE`, `status text NOT NULL DEFAULT 'pending' CHECK IN
  ('pending','ready','failed')`, `payload_bytes bigint NOT NULL DEFAULT 0`, `skipped text[] NOT NULL
  DEFAULT '{}'`, `created_at`/`updated_at timestamptz NOT NULL DEFAULT now()`.
- One index `account_exports_user (user_id, created_at DESC)` (:15) — a listing index, not a dedupe key.
- Static RLS block (:17-21): `ENABLE` + `FORCE` + `account_exports_system_only`, USING and WITH CHECK
  both `boma.is_system()`. Unlike `0018`, no dynamic `format()` loop (good, greppable).
- Header (:1-4) states the design intent: no `space_id` — an export crosses every space, so only a
  system reason may read, and the application checks ownership explicitly.

Architectural traits worth naming:
1. **Ledger, not archive.** The table records only status, byte count and skipped paths. The zip bytes
   live in out-of-band object storage; the DB holds no `storage_key`, no manifest `version`, no
   `download_url` — nothing that can locate or validate the artifact it describes. NFR requires a
   versioned manifest (`04-nonfunctional.md:138`, `02-data-model.md:172-173`) that is invisible here.
2. **Ownership is split across layers.** RLS admits only system; `find` filters `id + user_id` in SQL,
   but `mark_ready` updates by `id` alone and relies on an earlier app-side check (#317, GUESS). The DB
   cannot express "owner may read own row" and enforces no transition guard.
3. **Lifecycle has no shape.** `status` admits `failed`, but no method writes it and there is no
   `failed_at`/error column; `ready` has no completion timestamp; `payload_bytes` is not coupled to
   status. A terminal row carries no reason or time.
4. **No idempotency at the boundary.** Repeated `create` calls mint distinct rows; no key prevents
   duplicate exports for the same request. No retention/expiry structure, so GDPR deletion cascades
   the ledger row via `user_id` but leaves the archive bytes to another layer (REQ-044, :357-360).

## 2. Guiding principle for v0.4.0

Additive only. Never edit `0046`: sqlx verifies its checksum, so an in-place edit breaks `boma migrate`
on every deployed database (per #473 / `STATE.md`, GUESS). Every change is a NEW forward-only file
(e.g. `0060_account_exports_archive_ref.sql`) adding columns (nullable/defaulted), objects or indexes
— never `DROP`/`RENAME`/`ALTER TYPE`/`DELETE`/row rewrite. Keep the wire contract byte-stable:
`exportId`, `status` strings, index and policy names, RLS semantics. Any external delta is a DEVIATION
for `docs/PARITY.md` with owner approval (AGENTS.md).

## 3. Concrete upgrades (5), each with a risk

**U1 — Freeze `0046` and assert its shape (structure; do first).** Never edit the file; add a REQ-044
tagged test on a freshly migrated DB asserting the exact columns/types/defaults, `user_id` FK action,
one index, RLS enabled+forced and the `account_exports_system_only` expression. *Risk:* `pg_catalog`
introspection drifts across Postgres versions and an exact match breaks. *Mitigation:* assert by object
name and pin to the embedded PG 17; keep expectations in one shared constant (pattern from #473 U5).

**U2 — Give the archive a DDB-side reference and format version (coupling; highest value).** New
migration: `ADD COLUMN storage_key text`, `manifest_version integer`, `completed_at timestamptz`
(nullable, no defaults), so the row can locate and validate the versioned zip NFR requires. *Risk:*
nullable columns stay NULL on old rows and can drift from the object store, creating a second source
of truth. *Mitigation:* document object storage as authoritative, leave unused in v0.4.0, and require
the store writer (#317) to populate them in the same commit that adopts the columns.

**U3 — Make failure and time explicit without changing the vocabulary (lifecycle).** New migration:
nullable `failed_at timestamptz` and `failure_reason text`, plus a reference table
`account_export_status (status text PRIMARY KEY)` seeded from the three existing literals and a test
that the live CHECK set equals it. *Risk:* the app never writes `failed` today, so columns are dead
until a writer changes; a future hand-copied CHECK (cf. `0045` audit kinds) can drop a literal.
*Mitigation:* land the verification test first, leave the CHECK verbatim, and pair any new status with
a Rust enum + SQL step (coordination with #317).

**U4 — Add a dedupe key without touching old rows (idempotency; barrier).** New migration: nullable
`request_key text` plus `CREATE UNIQUE INDEX account_exports_user_request ON account_exports
(user_id, request_key) WHERE request_key IS NOT NULL`. Old rows (NULL) are unaffected; a repeated
`create` can be detected by the app. *Risk:* NULL means no protection until writers set it, and
deduplicating a start changes client-visible responses (DEVIATION). *Mitigation:* ship the column
unused in v0.4.0, add a concurrency test for two parallel start requests, and record any response
change in `docs/PARITY.md` before enabling it.

**U5 — Add retention structure, not a purge (lifecycle; last).** New migration: nullable
`expires_at timestamptz` and a partial index on `created_at WHERE expires_at IS NOT NULL` for a future
cleanup scan; a header states that purge belongs to a scheduled job and must reach object storage too.
*Risk:* a TTL duplicates the lifecycle rule and an aggressive cutoff destroys evidence; the FK cascade
only removes the row, not the archive. *Mitigation:* leave `expires_at` NULL in v0.4.0, require an ADR
and a conservative window (GUESS: 30 days) before any purge, and test the exact cutoff.

## 4. Sequencing and cross-cutting risks

- Order **U1 → U2 → U3 → U4 → U5**: freeze, reference, lifecycle, idempotency, retention. Each lands as
  one additive migration and one commit mentioning REQ-044; U2-U5 are dormant structure until a writer
  adopts them. No gate weakened.
- Coordinate with #317 (infra store: `mark_ready` ownership, typed status, `bundle` decomposition) and
  #504 (reliability lens on this same file): this brief owns only migration structure; store logic,
  error mapping and runtime limits stay in those briefs.
- Regenerate SQLx offline metadata for every new object. CI migrates a fresh DB, replays the
  `export-start` golden and REQ-044 tests, and runs `tools/trace_check.py`. No new dependency.

## 5. Out of scope

Any in-place edit of `0046`; a view/`SECURITY DEFINER` function exposing owner-readable rows (would
duplicate the app's ownership check, #317); the RPC start path, zip/artifact I/O and actual archive
purge; the `status` enum in Rust. Each belongs to another brief or to a later, ADR-gated release.

## 6. Evidence and suggested tests

- CONFIRMED this session: `0046` (:1-21), `0030`, `0036`, `0045`, the infra store, `ports.rs:700-754`,
  `account_export.rs:260-345`, the three spec sections and the golden named above.
- GUESS (re-verify before implementing): sqlx checksum enforcement; PG 17 as migrate target; no code
  writes `failed` today (from #317); archive bytes stored out of band with no version persisted.
- Suggested names: `req_044_migration_0046_shape_and_rls_stable` (U1),
  `req_044_export_archive_reference_columns_additive` (U2),
  `req_044_export_status_vocabulary_matches_check` (U3).

Line budget: <120. Status: proposal only; READ-ONLY worker, no code changed and no tests run.
