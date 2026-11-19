# v0.4.0 architecture proposal — `migrations/0013_memory.sql` (memory schema)

Worker #439 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no code changed).

Status of claims: "current state" below is CONFIRMED by direct reads of `0013_memory.sql`,
`0024_memory_shared_bot_id_nullable.sql`, `crates/infra/src/stores/memory.rs`,
`crates/infra/src/db.rs`, `spec/01-requirements.md` (REQ-022) and `spec/golden/memory-write.json`.
Forward-looking items are proposals (GUESS until implemented and tested). Additive only: every
item ships as a new `00NN_*.sql`; 0013 itself, existing data and all public interfaces stay intact.

## Current structure (CONFIRMED)

- 0013 creates `memory_documents` (`:8-20`) and append-only `memory_revisions` (`:22-30`). The
  current document is a *projection*; `content` is duplicated in both tables by design.
- Two identity systems coexist: a deterministic text `id = mem:{space}:{scope}:{bot}:{path}`
  (built in `infra/src/stores/memory.rs:14-16`) and a `bigserial` key on revisions. Document
  identity is really the 4-tuple, enforced by `UNIQUE (space_id, scope, bot_id, path)` (`:18`).
- 0013 encoded shared memory as `bot_id = ''` with `bot_id NOT NULL REFERENCES bots(id)`, which
  made shared rows impossible (the FK rejects `''`). `0024` repaired this by making `bot_id`
  nullable, replacing the CHECK and adding a partial unique index for shared rows. The *effective*
  schema therefore contradicts 0013's own header comment — the file is not self-contained.
- The store carries two parallel `INSERT ... ON CONFLICT` statements, one per arbiter (table UNIQUE
  for bot rows, partial index for shared), then appends the revision in a separate statement
  (`memory.rs:46-95`). Consistency between `documents.revision` and the latest revision row is an
  application invariant only; the DB does not enforce it.
- RLS is installed by a `DO $$` loop with dynamic `EXECUTE format(...)` (`:34-49`): ENABLE + FORCE
  + one policy per table keyed on `boma.current_space_id()` / `boma.is_system()`. Policy bodies are
  invisible to static tooling and the helper calls are unqualified.
- `updated_at` / `created_at` are caller-supplied (`NOT NULL`, no default); the golden carries a
  fixed client timestamp (`memory-write.json`), so the write path trusts client time.
- The only explicit index is `memory_revisions_document_idx (document_id, revision)` (`:32`), which
  duplicates the implicit index from `UNIQUE(document_id, revision)` (`:29`). Reads filter by
  `space_id` (via RLS) too, but no index leads with `space_id`.
- Migrations are embedded with `sqlx::migrate!("../../migrations")` and run by `MIGRATOR.run`
  (`db.rs:20,72-74`): each file runs in a transaction, so `CREATE INDEX CONCURRENTLY` is not
  available from a migration.

## Guiding principle for v0.4.0

Additive migrations only: layer a canonical read surface, consistency guards, retention helpers
and security hardening over the existing tables without rewriting 0013/0024, without touching
data and without changing any public interface. Where an addition is wire-visible, record a
DEVIATION in `docs/PARITY.md` and wait for approval before shipping.

## Upgrades (5 concrete items)

1. **`00NN_memory_current_view.sql` — canonical read surface.** Add a
   `boma.memory_documents_current` view exposing `(space_id, scope, owner_bot_id NULL for shared,
   path, content, revision, updated_at)`, so consumers no longer need the empty-string-vs-NULL
   history. Additive and read-only; RLS must still apply to callers.
   *Risk:* a view owned by a privileged role silently bypasses RLS unless `security_invoker = true`
   (PG 15+); the view also tracks base columns and drifts when they change.
   *Mitigation:* set and assert `security_invoker` in a migration test; add `COMMENT ON` naming the
   shared/NULL encoding as the single source of truth.

2. **`00NN_memory_revision_guard.sql` — DB-enforced projection consistency.** Add a `DEFERRABLE
   INITIALLY DEFERRED` constraint trigger on `memory_documents` that, at commit, rejects a row
   whose `revision` has no matching `memory_revisions (document_id, revision)` row. Existing app
   writes stay intact; the DB now catches any future writer that bumps the projection without
   appending a revision.
   *Risk:* deferred checks surface late at commit time; the guard cannot detect a revision row
   inserted without a matching projection bump.
   *Mitigation:* test the happy path plus a deliberately broken write in `infra/tests`, and document
   that it complements (not replaces) the transaction in `tenant::with_tenant`.

3. **`00NN_memory_revision_retention.sql` — bounded append-only growth.** Add
   `boma.memory_revisions_prune(keep_last int)` and an index leading with `(document_id, revision
   DESC)` so operators can cap revision history per document. Spec/golden only require the latest
   projection, so pruning older revisions is behavior-preserving for REQ-022.
   *Risk:* pruning is physically destructive and irreversible; a future history reader would break
   silently; index builds inside `MIGRATOR.run` cannot use `CONCURRENTLY`.
   *Mitigation:* make pruning an explicit, logged operator function (no automatic job), guard
   `keep_last >= 1`, and verify no code path reads `memory_revisions` beyond the current row.

4. **`00NN_memory_rls_search_path.sql` — harden the RLS helpers additively.** Use
   `ALTER FUNCTION ... SET search_path = boma, pg_temp` on the policy helpers, and add a guard so a
   repeated policy-installation loop cannot stack duplicate policies. No schema or client behavior
   change.
   *Risk:* a helper body that currently resolves objects in `public` can break once the search path
   is pinned; `ALTER FUNCTION` is not undone by a later `CREATE OR REPLACE`.
   *Mitigation:* read the current definitions in the `boma` schema first, pin the exact schemas they
   use, and add a regression test that a cross-tenant read still returns zero rows.

5. **`00NN_server_time_defaults.sql` — trustworthy timestamps.** Add `DEFAULT now()` to
   `updated_at` / `created_at` plus a `BEFORE INSERT OR UPDATE` trigger that fills them only when
   the caller omits a value, moving the default off the client while preserving explicit timestamps
   for parity fixtures.
   *Risk:* a trigger that always overwrites would break `memory-write.json`'s fixed
   `2026-01-01T00:01:00Z`; changing who supplies time is wire-visible.
   *Mitigation:* keep caller-provided values authoritative, test both omitted and supplied cases,
   and log any DEVIATION in `docs/PARITY.md`.

## Cross-cutting risks / sequencing

- All five are new files only; none edits 0013/0024 or rewrites data. 1 and 4 are local and lowest
  risk — land first. 2 and 3 touch write/history semantics; gate them on `infra/tests` coverage.
  5 is wire-visible and needs PARITY sign-off.
- Respect the transactional migration runner: no `CONCURRENTLY`, no long locks in one file — keep
  index builds small or split them.
- Keep REQ-022 in every new test name (`tools/trace_check.py` counts declarations, not comments)
  and replay `spec/golden/memory-write.json` after each landing.
- Verification gate per step: `cargo fmt --check`, `clippy -D warnings`, `cargo test`, and migrations
  applied both to a fresh DB and to a DB that has already run `0024`.

## Out of scope for this module

The store adapter (`crates/infra/src/stores/memory.rs`), the `MemoryStore` port and the
`/rpc/memory/write` route are separate briefs; this proposal covers additive schema structure
only. Any change to the response shape or to `spec/golden/memory-write.json` is a DEVIATION, not a
schema-only addition, and is excluded here.
