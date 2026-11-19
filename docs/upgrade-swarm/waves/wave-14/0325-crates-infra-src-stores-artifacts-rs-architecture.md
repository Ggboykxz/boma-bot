# Worker #325 — Architecture proposal for `crates/infra/src/stores/artifacts.rs` (v0.4.0)

Lens: architecture. Module: Postgres adapter for the REQ-036 artifact metadata store (bytes live out
of band via `ArtifactContentStore`; `storage_key = hash`). Read-only worker: no code was changed and
no tests were run. All structural observations below are CONFIRMED by reading the module,
`crates/app/src/ports.rs` (2837–3040), `crates/infra/src/tenant.rs`,
`crates/infra/src/stores/rows.rs`, `migrations/0036_artifacts.sql`,
`crates/app/src/usecases.rs` (1775–1802) and the in-memory adapter in
`crates/testkit/src/memory.rs` (6470–6559) in this session.

## Current structure (CONFIRMED)

- A single `PostgresArtifactStore { pool }` implements the nine `ArtifactStore` operations; every
  call opens its own transaction through `tenant::with_tenant`, which sets the transaction-scoped
  `boma.space_id`/`boma.system_reason` and relies on RLS for isolation (ADR-002).
- Seven SQL `const`s repeat the *same* 15-column projection; the 15-element tuple type is declared
  once as `ArtifactSummaryRow` (line 28) but then re-typed inline in **eight** `query_as` calls
  (lines 148–167, 244–260, 281–325, 344–363, 382–401, 424–443). This contradicts the stated rule
  in `stores/rows.rs`: "the shape of the row is written down once".
- `put` does three round trips: root-existence check, `SELECT max(version)+1`, then `INSERT`, then a
  re-read via `SQL_GET_BY_ID` — four statements, with version allocation only as atomic as the
  surrounding transaction. The unique index `artifacts_family_version(space_id, root_artifact_id,
  version)` is the real guard against concurrent duplicates.
- `list_space` computes "latest per family" with a correlated `SELECT max(version)` subquery per
  row; `list_versions`, `family_keys` and `remove_family` resolve the family root with a repeated
  scalar subquery on `id`.
- Storage key is not taken from a content-store result: line 236 binds `artifact.hash` a second time
  as `storage_key`. `size` is cast with `try_from(...).unwrap_or(u64::MAX)` on read and rejected with
  `AppError::Unavailable` on write (the DB `CHECK (size >= 0)` makes the clamp unreachable).
- Context handling is inconsistent: `find_by_nonce` returns `Ok(None)` and `put` returns
  `Err(NotFound)` for `TenantContext::System`, while the other methods run with the system RLS
  bypass. `family_keys`/`remove_family` use an `id`-only subquery, so a system-context delete is not
  explicitly space-scoped (RLS is the only guard). The memory adapter returns empty/0 for the same
  system calls — the two adapters are not behaviourally conformant.
- No `tracing::instrument`, no store-level tests, and no REQ-036-named test in this file.

## Upgrades for v0.4.0 (with risks)

### U1 — One row type, one mapping (highest value, lowest risk)
Add `ArtifactRow` as `#[derive(Debug, sqlx::FromRow)]` in `stores/rows.rs` with the 15 typed fields
(`created_at: String` after `boma.rfc3339`), implement `From<ArtifactRow> for ArtifactSummary`
(move `summary_from`'s `u64::try_from` there), and replace all eight inline tuples with
`query_as::<_, ArtifactRow>`. Delete `ArtifactSummaryRow`/`summary_from` here.
*Risk:* a `FromRow` field name must match the projected column alias exactly; keep the `boma.rfc3339`
alias as `created_at`. Compile-time checking against committed offline metadata catches drift at
build time. Net effect: ~120–150 fewer lines and a single place where the projection can break.

### U2 — Atomic, idempotent `put`
Compute the version and insert in one statement (a CTE over `INSERT ... SELECT ... ON CONFLICT
(space_id, root_artifact_id, version) DO NOTHING RETURNING *`, or a bounded retry on
`tenant::is_unique_violation`), keep the existing root-belongs-to-space check, and map a nonce
conflict (`artifacts_space_user_nonce`) to a typed conflict rather than `AppError::Unavailable`.
Fold the post-insert `SQL_GET_BY_ID` re-read into `RETURNING` (same projection via U1).
*Risk:* error-shape changes are externally visible → record any change in `docs/PARITY.md` before
shipping; retry must not mint a gap in the dense version chain (golden expects versions 1..3);
needs a concurrent-upload test to prove the old `Unavailable` race is gone.

### U3 — Query shape and family-root resolution
Rewrite `list_space` as `DISTINCT ON (root_artifact_id) ... ORDER BY root_artifact_id, version DESC,
created_at DESC` (or a `LATERAL` join), then order by `created_at DESC, id DESC` for determinism.
Introduce one private helper/CTE that resolves `(space_id, root_artifact_id)` from an `artifact_id`
and reuse it in `get`, `get_exact`, `list_versions`, `family_keys`, `remove_family`.
*Risk:* `DISTINCT ON`/tie-break ordering must match the golden's newest-first order exactly; the
helper must keep the RLS predicate and not widen scope for `TenantContext::System`; benchmark before
and after with an `EXPLAIN` on a seeded family table (claim stays GUESS until measured).

### U4 — Converge the two adapters on tenant-context semantics
Decide and document one rule for `TenantContext::System` (e.g. `find_by_nonce`/`put` stay rejected,
reads stay allowed, mutating family ops require an explicit space), implement it identically in the
Postgres and memory adapters, and add a conformance test that replays the same scenario against
both. Explicitly carry `space_id` in `remove_family` rather than relying only on the RLS bypass.
*Risk:* system-context maintenance paths may currently depend on the silent empty/0 answers; changing
them needs a scan of callers first. ADR-002's static `boma.space_id` grep and its single-setting
invariant must remain intact.

### U5 — Content provenance, observability, traceability
Take the out-of-band write result (`ContentWrite { hash, size }`) as the source for `hash`,
`storage_key` and `size` instead of binding `artifact.hash` twice; validate the `sha256:` prefix at
the boundary. Add `#[tracing::instrument(skip_all)]` spans with `ops` counters but **never** log
`client_nonce`, `user_id` or blob keys (REQ-054). Add REQ-036-named integration tests for nonce
replay, concurrent version allocation, delete visibility for every space member, and the D-03
"HTML declared as PNG" sniffed-type case.
*Risk:* changing how `storage_key` is derived touches the port signature and the memory adapter — a
DEVIATION if it changes the public API, so keep it internal unless approved. Spans add log volume;
keep fields redacted.

## Sequencing / non-goals

U1 → U2 → U3 → U4 → U5; U1 and U2 are prerequisites for safe query edits. No new features outside
REQ-036/D-03, no schema change (migration 0036 already carries the needed unique indexes), and no
change to the `ArtifactStore` port surface without a recorded DEVIATION. All forward-looking items
are proposals (GUESS until implemented and measured); only the "Current structure" section is
CONFIRMED in this session.
