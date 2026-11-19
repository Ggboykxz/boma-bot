# v0.4.0 reliability proposal — `crates/infra/src/stores/artifacts.rs` (worker #326, lens: reliability)

## Current design (CONFIRMED — read in this session)

`PostgresArtifactStore` is a thin adapter over `sqlx::PgPool`; every method runs inside
`tenant::with_tenant`, which opens a transaction, sets `boma.space_id`/`boma.system_reason`
(`set_config(..., true)`), and commits or rolls back the whole closure. RLS isolates rows by
`space_id`. This gives atomicity *within one method*, but not across the multi-call flows that
drive blob lifecycle, and not across concurrent transactions under Postgres' default READ COMMITTED.

Schema (CONFIRMED, `migrations/0036_artifacts.sql`): `artifacts_family_version` is UNIQUE on
`(space_id, root_artifact_id, version)`; `artifacts_space_user_nonce` is UNIQUE on
`(space_id, user_id, client_nonce)` where non-null; `storage_key = hash`, so blobs are content-addressed
and *may be shared across families*. `tenant::unavailable` maps every SQL error to `AppError::Unavailable`
(503) and logs at warn; `tenant::is_unique_violation` exists but this store never uses it.

## Failure modes

### 1. Unbounded / unbounded-cost queries (CONFIRMED; severity: medium)

- `list_versions` (L374–410) issues `SQL_LIST_VERSIONS` (L115–122) with `fetch_all` and **no LIMIT**.
  A long-lived family (auto-versioned bots) can return an arbitrary number of rows in one shot —
  latency and memory grow with history, with no cap, cursor or timeout.
- `family_keys` (L456–475) runs `SELECT DISTINCT storage_key ... fetch_all` with no LIMIT, and feeds
  the remove flow; its cost scales with family size.
- `SQL_LIST_SPACE` (L123–132) uses a correlated `max(version)` subquery per candidate row. The
  `artifacts_space_root` index mitigates it, but the work is still proportional to matched families;
  `list_space` at least clamps `limit` to `1..=100` (L421) — this is the pattern the two above lack.
- `storage_key_refs` (L499–515) is a `count(*)` — bounded and cheap (positive control).
- **Silent-empty seam:** `family_keys` and `remove_family` (L477–497) resolve the family with
  `root_artifact_id = (SELECT root_artifact_id FROM artifacts WHERE id = $1)`. If the id is absent the
  subquery is NULL, so `family_keys` returns `Ok(vec![])` and `remove_family` returns `Ok(0)` — both
  indistinguishable from a successful empty operation. `artifact_remove` (usecases.rs L1780–1802)
  propagates that as success.

### 2. Orphan blobs (CONFIRMED; severity: high)

Bytes live out of band, but this store's API drives their lifecycle across separate transactions:

- `artifact_create` (usecases.rs L1663) writes the blob (`artifact_content.put`) **before** `put`
  inserts the row. Any `put` error here — bad root (`NotFound`, L199–210), unique/nonce violation,
  DB outage — leaves an unreferenced blob. The hash-keyed write is idempotent, but nothing reclaims it.
- `artifact_remove` calls `family_keys`, then `remove_family`, then per-key `storage_key_refs` +
  `artifact_content.remove` as four independent operations. A crash or failure between them leaves
  blobs orphaned (rows gone, bytes remain). Worse, `let _ = ...remove(&key).await` (usecases.rs L1798)
  **swallows** blob-deletion errors.
- **Delete-live-blob race:** `storage_key_refs` returning 0 is a check, not a reservation. A concurrent
  `put` referencing the same hash (shared `storage_key`) can land after the count and before the blob
  delete, producing a row that points at a deleted blob. `put`'s insert binds `storage_key = hash`
  (L236), so this is reachable, not theoretical.
- No reconciliation path exists: there is no store method to enumerate referenced keys for a sweep.

### 3. Version drift (CONFIRMED; severity: high)

- `put` computes `coalesce(max(version),0)+1` (L211–217) then inserts (L218–243) under READ COMMITTED.
  Two concurrent uploads to the same root can read the same max and both insert the same version. The
  UNIQUE index rejects one, and `tenant::unavailable` turns that into a 503 instead of a bounded retry
  or a 409 — a spurious failure for a legitimate client. The same happens on a replay whose
  `client_nonce` raced past `find_by_nonce` (usecases.rs L1656–1661): the unique nonce index fires,
  surfacing as 503 rather than an idempotent replay. The 3-query sequence (exists-check, max, insert)
  could be one CTE with a lock.
- `SQL_GET_PINNED` (L101–107) is `(id = $1 OR root_artifact_id = $1) AND version = $2` with
  `fetch_optional` (L326–329). If two branches ever share a version number, the query matches >1 row
  and `fetch_optional` **errors** ("multiple rows") instead of returning one summary.
- `SQL_GET_LATEST` (L108–114) is `ORDER BY version DESC LIMIT 1` with no tie-break, so "latest" is
  nondeterministic across equal-version forks.
- `summary_from` does `u64::try_from(size).unwrap_or(u64::MAX)` (L74). The DB CHECK (`size >= 0`)
  makes this unreachable today, so severity is low, but it silently fabricates a value on corruption.

## Concrete hardening for v0.4.0 (with risks)

1. **Bound every unbounded read.** Add a defensive `LIMIT` to `SQL_LIST_VERSIONS` and `family_keys`
   (e.g. 1000), and expose a cursor/`offset` variant if callers need more. *Risk:* the REQ-036 golden
   asserts "every version, newest first" (ports.rs L2961); a cap is an externally visible DEVIATION —
   record it in `docs/PARITY.md` and get approval before shipping, or keep the cap high enough that
   goldens pass and document it as a safety ceiling only.
2. **Make family resolution explicit.** In `family_keys`/`remove_family`, first `fetch_optional` the
   root id and return `AppError::NotFound` when absent, so "no such family" cannot masquerade as
   success. *Risk:* callers relying on a silent zero (none found in this session — GUESS, check
   `artifact_remove` consumers) would need updating.
3. **Close the version race.** Wrap the insert in a bounded retry on `is_unique_violation` (recompute
   `max(version)` inside a fresh transaction, ≤3 attempts), or take `pg_advisory_xact_lock(hashtext(root))`
   before the max read to serialize per family; then map a nonce collision to an idempotent replay
   response. *Risk:* advisory locks add a dependency on lock hygiene and can serialize hot families;
   retries must stay inside a fresh transaction each attempt (the blob write is hash-idempotent, so
   retries are safe).
4. **Make pinned/latest deterministic.** Add a secondary `ORDER BY created_at DESC, id DESC` to
   `SQL_GET_LATEST`, and change `SQL_GET_PINNED` to `ORDER BY version DESC, created_at DESC, id DESC
   LIMIT 1` so `fetch_optional` cannot error on tied versions. *Risk:* if ties are real, this picks a
   winner rather than surfacing drift — pair with a metric/alert on duplicate `(root, version)`.
5. **Make blob reclamation atomic-ish.** Replace the three-call remove flow with one store method
   that, under row locks inside a single transaction, deletes the family and returns the set of
   `storage_key`s with zero remaining references; then delete blobs with retry + `tracing` on failure
   instead of `let _ =`. Add a periodic reconciliation sweep (referenced keys vs blob tree) to heal
   historical orphans. *Risk:* this changes the `ArtifactStore` port (DEVIATION, needs approval); a
   sweep needs a scheduling surface and must not delete concurrently-written blobs — use the same
   locked "zero refs" query as the authority.
6. **Fail loudly on type coercion.** Replace `unwrap_or(u64::MAX)` (L74) with a checked conversion
   mapped to `AppError::Unavailable` so corrupt sizes are visible. *Risk:* negligible; DB CHECK makes
   it dead code today.

## Verification (v0.4.0 gate)

- Integration test: two concurrent `put`s to one root both succeed with versions 1 and 2 (no 503).
- Test: id-missing `family_keys`/`remove_family` return an error, not `Ok(empty)`/`Ok(0)`.
- Test: a blob shared by two families is NOT reclaimed when one family is removed.
- Test: pinned version with a tied sibling returns exactly one row (no `fetch_optional` error).
- Load test: `list_versions` on a 10k-version family stays under the latency budget with the cap.

*All line references and schema facts above are CONFIRMED by reading the files in this session;
the absence of external consumers of `artifact_remove` and the reachability of cross-family hash
sharing in production are GUESS and should be verified before implementation.*
