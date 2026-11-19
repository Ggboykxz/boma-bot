# 0473 — `migrations/0030_audit_events.sql` — architecture (v0.4.0)

- Worker #473 · Lens: architecture (structure, boundaries, coupling, debt) · Target: v0.4.0.
- Status: PROPOSAL (read-only review; no code or migration changed). CONFIRMED = verified by direct
  reads in this worktree; GUESS = reasoned from code, not executed against a live DB.
- Module: `migrations/0030_audit_events.sql` (22 lines): the M0/M9 append-only audit table for D-05
  (private MCP endpoints), read later by REQ-045 export/retention.
- Read for context (CONFIRMED): `migrations/0001_init.sql`, `0002_m1.sql` (`boma.current_space_id()`
  :27–31, `boma.is_system()` :38–42), `0029_mcp_servers.sql`, `0045_audit_cors.sql`,
  `0049_password_recovery_audit.sql`, `0052_invite_accept_audit.sql`, `0046_account_exports.sql`,
  `crates/domain/src/audit.rs`, `crates/infra/src/stores/audit.rs`, wave-14's `0327-...audit-rs...`,
  `spec/01-requirements.md` REQ-045, `spec/05-known-defects.md` D-05.

## Current structure (CONFIRMED)

One forward-only file, consistent with the sqlx `MIGRATOR` pipeline (no down file, checksum recorded):
- `audit_events`: `id text PK` (caller-generated), `space_id text NOT NULL REFERENCES spaces ON
  DELETE CASCADE`, `actor_user_id text` (nullable, no FK), `kind text NOT NULL CHECK` admitting only
  `'mcp.private_call'`, `target text NOT NULL`, `metadata jsonb NOT NULL DEFAULT '{}'`,
  `created_at timestamptz NOT NULL DEFAULT now()`.
- One index: `audit_events_space_created (space_id, created_at DESC)`.
- RLS: `ENABLE` + `FORCE` + `audit_events_tenant_isolation` using
  `space_id = boma.current_space_id() OR boma.is_system()` (USING and WITH CHECK).
- The header comment declares the table "append-only" and says export/retention (REQ-045) reads,
  never rewrites — but nothing in the DDL enforces that invariant.

Downstream drift (CONFIRMED): `0045` already widens `kind`, makes `space_id` nullable with
`ON DELETE SET NULL`, and rewrites the RLS policy; `0049` and `0052` each DROP and recreate
`audit_events_kind_check`, hand-copying the *entire* literal list to add one value. The domain enum
`AuditKind::as_str()` has 19 variants — a second, independent source of truth. `0030` as shipped is
therefore a historical snapshot, not the live shape; proposals below target the *current* table.

## Guiding principle for v0.4.0

Additive only: never edit `0030` (its checksum is recorded by sqlx), never drop, rename or rewrite a
column or row. Every item is a NEW migration, a new object, or a new test. Keep the wire-visible
contract — kind strings, `security.` prefix filter, `ORDER BY created_at DESC, id DESC`, JSONL export
shape, RLS semantics and error text — byte-stable; any external delta is a DEVIATION for
`docs/PARITY.md` (AGENTS.md).

## Concrete upgrades (5), each with a risk

1. **Enforce append-only in the DB, not just in a comment.** New migration adds a `BEFORE UPDATE OR
   DELETE` trigger on `audit_events` that raises an exception, bypassed only by a system reason
   (`boma.is_system()`) so M9 retention can purge. *Risk:* a naive trigger blocks the retention/export
   path the header says will read (and eventually prune) this table. *Mitigation:* implement the bypass
   explicitly, add a REQ-045 test asserting member UPDATE/DELETE fails while a system-context DELETE
   succeeds, and replay the export goldens. (CONFIRMED the invariant is comment-only today.)

2. **Stop hand-copying the `kind` vocabulary.** Add a reference table `audit_event_kinds (kind text
   PK)` seeded from `AuditKind::as_str()`, and a migration-level test that reads the live
   `audit_events_kind_check` and asserts its literal set equals the reference table. Leave the existing
   CHECK verbatim in v0.4.0. *Risk:* rewriting the CHECK by hand (as 0049/0052 do) can drop a valid
   literal and reject a live write. *Mitigation:* land the verification test first, generate future
   CHECK lists from the reference table, and keep a paired Rust-enum + SQL step documented for new
   kinds — no constraint rewrite in this release.

3. **Close the index gap for the real read pattern.** The store filters `kind LIKE 'security.%'` and
   orders `created_at DESC, id DESC`, but the only index is `(space_id, created_at DESC)`, so the
   tiebreaker and kind filter are not index-served. Additively create
   `audit_events_space_kind_created (space_id, kind, created_at DESC, id DESC)`, a partial index
   `WHERE space_id IS NULL` for the system/export path, and `audit_events_actor_created
   (actor_user_id, created_at DESC)` for "what did this user do". *Risk:* a `LIKE 'prefix%'` predicate
   may not use a default-collation btree as expected, and extra indexes add write amplification.
   *Mitigation:* verify with `EXPLAIN` on a seeded DB, keep the existing index, and gate on a before/
   after benchmark of `append` (GUESS: benefit unmeasured).

4. **Give actor and time real semantics.** Add `actor_user_id` FK to `users(id) ON DELETE SET NULL`
   as `NOT VALID` (non-blocking, no row rewrite) plus a nullable `occurred_at timestamptz DEFAULT
   now()` with `CHECK (occurred_at <= created_at + interval '5 minutes')` for clock skew. This lets a
   security event record *when it happened*, not only when it was ingested. *Risk:* orphan actor ids
   make a validated FK fail, and two time columns invite reader confusion. *Mitigation:* `NOT VALID`
   first, pre-flight `SELECT` for orphans before any VALIDATE, document `created_at` as ingestion time
   and `occurred_at` as event time, and add a test that both are populated.

5. **Pin the schema contract with a REQ-tagged migration test.** Add
   `req_045_audit_events_shape_and_rls_survive_migrations` in `crates/infra/tests/database.rs`: on a
   freshly migrated DB assert column names/types/nullability, the three indexes, `space_id` FK action
   is `SET NULL` (post-`0045`), RLS enabled+forced, and the policy name — so a later migration cannot
   silently regress them. *Risk:* `pg_catalog` column names vary by Postgres version and an exact
   match is brittle. *Mitigation:* assert by object name and prefix only, pin to the embedded PG 17
   used by the packs, and derive expectations from one shared constant.

## Sequencing and gates

- Land 5 first (freeze the shape), then 2 → 3 → 4 → 1: contract, vocabulary, indexes, semantics,
  then enforcement (needs the retention bypass decided). Each item is one forward-only migration and
  one commit mentioning REQ-045/D-05.
- Keep `0030` byte-identical; regenerate SQLx offline metadata for every new object. CI migrates a
  fresh DB, replays REQ-045/REQ-058 goldens and runs `tools/trace_check.py` (REQ id in test names).
- Coordinate with wave-14's audit-store proposal: an `ON CONFLICT (id) DO NOTHING` idempotency fix and
  the `AuditKind` token centralisation are that module's scope, not this migration's. No new dependency.

## Out of scope

The `AuditStore` port, export/JSONL redaction (`usecases.rs`, `domain/src/audit.rs`), RLS policy
rewrites already shipped in `0045`, and any in-place edit of `0030` belong to other briefs.
