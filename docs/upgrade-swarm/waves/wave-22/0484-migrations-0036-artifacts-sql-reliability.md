# v0.4.0 reliability proposal — `migrations/0036_artifacts.sql` (worker #484, lens: reliability)

## Scope and current design (CONFIRMED — read in this session)

`artifacts` is one row per `(space_id, root_artifact_id, version)`. `hash` and `storage_key` are
free `text` NOT NULL; the header states `storage_key = hash` (content-addressed, bytes out of band).
`size` has only `CHECK (size >= 0)`. `root_artifact_id` is free `text` with no self-reference.
`group_id` is free `text` with no FK. RLS: `FORCE`, one policy gated on
`space_id = boma.current_space_id() OR boma.is_system()` for both USING and WITH CHECK. From
`0002_m1.sql`: `current_space_id()` reads `current_setting('boma.space_id', true)` (NULL when unset)
and `is_system()` is non-empty `current_setting('boma.system_reason', true)`. Goldens confirm the
root row's `root_artifact_id` equals its own id and hashes are `sha256:` + lowercase hex.
All proposals below are **additive** (ADD CONSTRAINT / CREATE TABLE / CREATE VIEW / COMMENT); the one
non-additive fix is explicitly deferred. This complements worker #326's store-level proposal — that
one fixes the Rust call sequences; this one fixes what the schema can and cannot guarantee.

## Failure modes

### 1. Content integrity (CONFIRMED schema; severity: medium–high)

- **Hash format unconstrained.** Any string is a valid `hash`/`storage_key`. A truncated or
  wrongly-encoded hash can be persisted and later mis-resolve or collide in the blob store; D-03
  (sniffed type) is only partly addressed because `mime_type` is stored but never structurally
  validated.
- **Documented invariant is unenforced.** `storage_key = hash` is a comment, not a constraint. A
  buggy or system-path writer can store a key that disagrees with the hash, breaking content
  addressing and making reconciliation by hash unreliable.
- **Unbounded size.** `size >= 0` admits absurd values; downstream `u64` coercion falls back to
  `u64::MAX` (worker #326, L74), so corruption becomes a silently fabricated value.
- **Broken version chain.** `root_artifact_id` need not name a real row, and a non-v1 row can point
  at itself. Uniqueness prevents duplicate `(root, version)` but **cannot** enforce dense versions;
  "1..N dense" (REQ-036) remains an application contract.
- Note: `declared_mime <> mime_type` is intentional (sniffed correction, D-03); do **not** add an
  equality CHECK.

### 2. RLS / tenant integrity (CONFIRMED policy; severity: high)

- **Fail-closed is good (positive control):** unset `boma.space_id` makes the `USING` predicate NULL,
  so a member session sees no rows. Preserve this.
- **Untyped system bypass.** The policy trusts `is_system()` with no reason allow-list: any
  connection able to set `boma.system_reason` reads and writes *every* artifact. `0002` narrows the
  reason via an enum at the source, but this policy does not re-check it — a D-09 escape hatch at
  the DB layer.
- **Orphan group reference.** `group_id` has no FK to `groups(id)` (0053: `id text PRIMARY KEY`),
  so a deleted/nonexistent group id can be attached to an artifact and survive group deletion.
- **Cross-tenant actor coherence.** No constraint ties `user_id` to `space_id`; a system-path write
  can attach a user from space A to an artifact in space B. RLS still isolates reads by `space_id`,
  but the row becomes an inconsistent provenance record.
- **Cascade blindness.** `space_id`/`user_id` are `ON DELETE CASCADE`: deleting a space or user
  removes rows silently with no signal for the out-of-band bytes (see §3).

### 3. Orphan blobs (CONFIRMED schema + sibling store proposal; severity: high)

- Bytes are content-addressed and **shareable across families** (`storage_key` is non-UNIQUE by
  design). There is no registry, refcount, or FK, so the DB cannot answer "which keys are still
  referenced?" without scanning rows.
- Cascades and crashes leave unreferenced blobs; a sweep that deletes a key at zero refs races with
  a concurrent insert referencing the same hash (sibling #326 §2). The schema offers no locking or
  authority for that decision.

## Concrete additive hardening (with risks)

1. `ADD CONSTRAINT artifacts_hash_format CHECK (hash ~ '^sha256:[0-9a-f]{64}$')`.
   *Risk:* historical rows using another algorithm or uppercase hex fail validation. GUESS — confirm
   against production rows; if mixed, add `NOT VALID`, clean up, then `VALIDATE`.
2. `ADD CONSTRAINT artifacts_storage_key_eq_hash CHECK (storage_key = hash)`.
   *Risk:* fails if any writer uses a namespaced/prefixed key. Sibling #326 confirms the insert binds
   `storage_key = hash` today (CONFIRMED by that doc; GUESS for all historical rows).
3. `ADD CONSTRAINT artifacts_size_bounded CHECK (size <= <artifact upload cap>)`.
   *Risk:* hardcodes a config value in DDL; drift when per-procedure caps change (D-07). Keep it a
   generous safety ceiling, documented next to the config.
4. `ADD CONSTRAINT artifacts_root_self_fk FOREIGN KEY (root_artifact_id) REFERENCES artifacts(id)
   ON DELETE CASCADE` plus `CHECK (version = 1 OR root_artifact_id <> id)`.
   *Risk:* self-cascade amplifies locks on family delete; confirm family delete is intended (TRACE
   lists "family delete", so likely aligned). Could leave the FK `NOT VALID` initially.
5. `ADD CONSTRAINT artifacts_system_reason_allowed CHECK (NOT boma.is_system() OR
   current_setting('boma.system_reason', true) IN (<canonical reason list>))`.
   *Risk:* must mirror the Rust enum from D-09 or legit system paths break. It constrains
   **writes only**; SELECT-side bypass needs a policy rewrite (non-additive) — deferred to a
   dedicated migration with an ADR. This is the main residual under "additive only".
6. `ALTER TABLE artifacts ADD CONSTRAINT artifacts_group_fk FOREIGN KEY (group_id) REFERENCES
   groups(id) ON DELETE SET NULL NOT VALID;` then clean orphans and `VALIDATE`.
   *Risk:* existing orphan ids block VALIDATE; `SET NULL` changes delete semantics slightly.
   Tenant coherence of the group's `space_id` needs a composite unique index on `groups(space_id,
   id)` first (additive, separate migration) — GUESS whether worth it.
7. Add `artifact_blob_objects(storage_key text PK, size bigint, created_at)` as the authority for
   key existence, then `ADD CONSTRAINT artifacts_storage_key_fk ... NOT VALID`, backfill from
   `artifacts`, `VALIDATE`. Add `artifact_blob_cleanup_outbox` plus an `AFTER DELETE` trigger that
   enqueues a key when its refcount reaches zero, drained by the reconciler.
   *Risk:* two-phase migration (backfill before VALIDATE, brief ACCESS EXCLUSIVE); the trigger must
   be `SECURITY DEFINER` with fixed `search_path` and must be idempotent; reconciliation must use a
   locked zero-refs query, not a bare count, or it reintroduces #326's delete-live-blob race.
8. `COMMENT ON` every new constraint with its REQ/D-xx rationale. Zero functional risk; makes the
   invariants discoverable and the GUESSes auditable.

## Verification gate (v0.4.0)

- Migration applies cleanly to a seeded DB containing the REQ-036 goldens; a fixture with a bad
  hash, mismatched `storage_key`, oversized `size`, orphan `group_id` and orphan root is rejected.
- Test: cascading a space delete enqueues exactly the keys whose refcount hit zero, and a key shared
  by two families is not enqueued.
- Test: a member session with no `boma.space_id` sees zero rows (fail-closed preserved).
- `NOT VALID`/`VALIDATE` steps exercised against a DB with seeded legacy rows.

*Facts tagged CONFIRMED were verified by reading the files named above in this session; production
data shapes, the canonical reason list, and the existence of a `(space_id, user_id)` unique key are
GUESS and must be checked before implementation. No source files were modified.*
