# 0483 — `migrations/0036_artifacts.sql` — architecture (v0.4.0)

- Worker #483 · Lens: architecture · Target: v0.4.0 · Read-only review; no code was changed.
- Module: `migrations/0036_artifacts.sql` (40 lines) at `before-m5` (v0.3.0-rc5).
- Read for context (CONFIRMED by direct reads): `crates/infra/src/stores/artifacts.rs`,
  `migrations/0053_groups.sql`, `migrations/0037_pages.sql`, `spec/01-requirements.md` REQ-036,
  `spec/02-data-model.md:115–126`, `spec/03-api-contract.md:99–103`, `spec/04-nonfunctional.md`,
  `AGENTS.md`, and wave-21 `0469-migrations-0028-skills-sql-architecture.md` (house format).

## Current structure (CONFIRMED)

One table `artifacts`: `id text PRIMARY KEY`; `space_id NOT NULL → spaces ON DELETE CASCADE`;
`bot_id → bots ON DELETE SET NULL`; `group_id text` **with no FK at all**; `user_id NOT NULL → users
ON DELETE CASCADE`; `run_id → runs ON DELETE SET NULL`; free text `name`, `description DEFAULT ''`,
`declared_mime`, `mime_type`; `size bigint CHECK (>= 0)`; `hash`, `storage_key`, `root_artifact_id`;
`version integer CHECK (>= 1)`; `standalone boolean DEFAULT true`; nullable `client_nonce`;
`created_at DEFAULT now()`. Four indexes: unique `(space_id, root_artifact_id, version)`, partial
unique `(space_id, user_id, client_nonce) WHERE client_nonce IS NOT NULL`, `(space_id,
root_artifact_id, version DESC)`, `(storage_key)`. RLS enabled **and forced**, one policy whose
USING and WITH CHECK are both `space_id = boma.current_space_id() OR boma.is_system()`.

The header states invariants the DDL does not encode: "storage_key = hash", and "versions form a
chain rooted at the first artifact". The store confirms the first (both bind `artifact.hash`),
but the version chain is trusted to `put`: it reads `max(version)+1` then inserts (lines 211–243),
relying on the unique index as the only guard; concurrent distinct nonces in one family surface as
`AppError::Unavailable` via `tenant::unavailable`. `find_by_nonce` filters only `(user_id,
client_nonce)` — no `space_id` (line 87). `list_space` filters `bot_id`/`group_id` with a correlated
`max(version)` subquery (lines 123–132). No index exists on `bot_id`, `group_id`, or `run_id`.

## Guiding principle for v0.4.0

Additive only: every change below is `CREATE INDEX`, `ADD CONSTRAINT ... NOT VALID` + `VALIDATE`, or
a new table/function/trigger with its own RLS policy. No dropped or renamed columns, no type change,
no replaced policy, no ORDER BY or wire-shape change. Preserve column names, the unique identity
`(space_id, root_artifact_id, version)`, the partial nonce index, and the exact RLS predicate —
any externally visible change is a DEVIATION requiring `docs/PARITY.md` (AGENTS.md). Claims about
data volume or plan shape are GUESS (no EXPLAIN or stats were run in this read-only review).

## Concrete upgrades (5), each with a risk

1. **Encode `storage_key = hash` and the hash format.** Add
   `CHECK (storage_key = hash) NOT VALID` and `CHECK (hash ~ '^[0-9a-f]{64}$') NOT VALID`, then
   `VALIDATE` after an audit `SELECT`. This turns the header's content-addressing promise into a
   database guarantee and catches adapter drift (the store binds `hash` twice, lines 235–236).
   *Risk:* legacy rows or a future non-SHA-256 backend use a different key (e.g. `s3://...`), and
   validation then fails on real data. *Mitigation:* audit distinct `hash`/`storage_key` pairs and
   key lengths first; if any differ, keep the constraint `NOT VALID` (guards new writes only) and
   record the conflict in `docs/STATE.md` rather than rewriting history.

2. **Encode the version-chain root invariant.** Add
   `CHECK ((version = 1) = (root_artifact_id = id)) NOT VALID`, then validate. This makes
   "rooted at the first artifact of the family" (header, `spec/02-data-model.md:119`) enforceable
   rather than convention. *Risk:* a family whose version-1 row was deleted by a cascade (there is
   no intra-family FK) leaves later rows with an orphan root, blocking validation and breaking
   `list_versions`/`remove_family` subqueries. *Mitigation:* audit
   `SELECT ... WHERE root_artifact_id = id AND version <> 1 OR ...`; repair or document before
   validating. Do **not** add an `ON DELETE CASCADE` self-FK — that silently changes family-delete
   semantics and is a DEVIATION.

3. **Close the `group_id` referential gap.** `group_id` is the only provenance column without an
   FK (CONFIRMED line 12); `groups.id` exists (`0053_groups.sql:6`). Add
   `artifacts_group_id_fkey FOREIGN KEY (group_id) REFERENCES groups(id) ON DELETE SET NULL NOT
   VALID` + validate. *Risk:* stale group ids block validation, and `SET NULL` changes today's
   behavior (deleting a group currently leaves a dangling `group_id` that still filters
   `list_space`). *Mitigation:* audit orphans, backfill/null them, then validate; call the SET NULL
   change out in `docs/PARITY.md` as a corrective DEVIATION if any caller observes it. Ordering is
   safe: an additive v0.4.0 migration runs after `0053`.

4. **Index the query shapes actually used.** Add `(space_id, bot_id, version DESC)`,
   `(space_id, group_id, version DESC)`, and `(space_id, run_id)` for `list_space` filters and
   run-scoped provenance; add `(user_id, client_nonce)` to match `find_by_nonce`'s real predicate
   (the current partial index leads with `space_id`, which that query omits and only RLS supplies).
   *Risk:* write amplification on an insert-heavy table, and a wasted index if the planner already
   uses the RLS predicate plus the existing partial index (GUESS). *Mitigation:* run `EXPLAIN
   (ANALYZE, BUFFERS)` on the four store queries first; ship only indexes that change a plan, use
   plain `CREATE INDEX` (concurrent build cannot run inside a migration transaction), and keep each
   one reversible via `DROP INDEX`.

5. **Reclaim out-of-band bytes on deletion.** Bytes live outside Postgres, so the `user_id`
   `ON DELETE CASCADE` and `space_id` cascade paths drop metadata without unlinking the object,
   and `spec/04-nonfunctional.md` requires reclaimable artifacts. Add an additive outbox
   `artifact_deletions (id, space_id, storage_key, deleted_at)` with RLS copied byte-for-byte from
   `artifacts_tenant_isolation`, populated by a `boma` (schema-qualified) `AFTER DELETE` trigger and
   drained by the existing file-store worker. *Risk:* a trigger on DELETE fires for family deletes,
   cascades, and RLS-visible deletes alike, so it can emit duplicate keys or fire during tenant
   teardown; and a consumer that never drains the outbox leaks rows. *Mitigation:* dedupe by
   `(storage_key, deleted_at)` and verify with `storage_key_refs` before unlinking (the store
   already exposes this), make the trigger conditional on `OLD.storage_key IS NOT NULL`, and land
   the worker change in the same release or keep the outbox inert until it does.

## Cross-cutting risks / sequencing

- Sequence 1 → 2 → 3 → 4 → 5: constraints need a data audit before `VALIDATE`; indexes come after
  EXPLAIN; the trigger/outbox last. All steps are individually revertable and leave the public
  surface untouched.
- Freeze while doing this: column names, the `(space_id, root_artifact_id, version)` unique index,
  the partial nonce index, the RLS predicate, and all `artifacts.rs` query strings/ORDER BYs.
- RLS grants `boma.is_system()` reads while `put`/`find_by_nonce` reject system context (lines 143,
  186); preserve that asymmetry exactly.
- `put`'s `max(version)+1` read-then-write (lines 211–243) is the real concurrency hotspot; the
  unique index prevents duplicates but reports collisions as `Unavailable`. Additive DB-side option:
  a `boma.artifact_family_lock(root text)` function taking a transaction-scoped advisory lock,
  called before `max(version)`, with the unique index still authoritative. Cross-module change —
  coordinate with the store worker; do not solve it by overriding the app-supplied version in a
  trigger (that changes behavior).
- `standalone` is always inserted `true` (line 222) and never updated; no additive change here,
  but note it as dead state for a future contract decision, not a v0.4.0 fix.
- Traceability: no new REQ id (context is REQ-036); `tools/trace_check.py` is unaffected. Gate with
  `cargo fmt --check`, `clippy -D warnings`, REQ-036 version/delete round-trips, goldens
  `artifact-create.json` + `artifact-versions.json`, and the account-export artifact layout test.
  No new dependency.

## Out of scope for this module

Rewriting/backfilling existing rows, a self-referencing family FK, case/normalization changes,
dropping or renaming columns or the RLS policy, the object-store adapter (`artifact_files.rs`),
the RPC/DTO shape, and the export layout (`account_export.rs`). Those belong to other workers; this
proposal only strengthens the `artifacts` table structure additively.
