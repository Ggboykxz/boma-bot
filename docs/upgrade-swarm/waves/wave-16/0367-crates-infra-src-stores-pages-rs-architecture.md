# 0367 — `crates/infra/src/stores/pages.rs` — architecture (v0.4.0)

- Worker #367 · Lens: architecture · Target: v0.4.0 · Read-only review; no code was changed.
- Module: `crates/infra/src/stores/pages.rs` (360 lines) at `before-m5` (v0.3.0-rc5): the Postgres
  adapter for `PageStore`, re-exported from `crates/infra/src/stores/mod.rs:32`.
- REQ-037 (`spec/01-requirements.md:305–309`): a shared page is readable signed-out by token only,
  with `noindex`/`no-referrer`, and a revoked or unknown token yields a uniform 404.
- Read for context (CONFIRMED by direct reads): `crates/app/src/ports.rs:3044–3174`
  (`PageSummary`, `PageVersion`, `NewPageVersion`, `PageStore`), `crates/app/src/usecases.rs:1480–1602`
  (`pages_save`, `pages_share`, `pages_revoke_share`, `pages_shared`), `crates/infra/src/tenant.rs`
  (`with_tenant` / `as_system` / `unavailable` / `is_unique_violation`), `migrations/0037_pages.sql`,
  `crates/testkit/src/memory.rs:6690–6915` (`MemoryPageStore`), `spec/golden/shared-page*.json`,
  `spec/05-known-defects.md`, and the wave-15/0333 proposal for house style.

## Current structure (CONFIRMED)

One struct, `PostgresPageStore { pool }`, with a `const fn new` (:15–25); six `PageStore` methods
(:81–359). Every method opens its own transaction through `tenant::with_tenant` (or `as_system` for
the public read), so RLS (`boma.space_id`) is set in one place. Two private mappers exist:
`map_page_row` takes the `PageSummaryRow` 10-tuple alias (:28–66), and `version_from` takes a
5-tuple (:68–78). All reads convert timestamps with `boma.rfc3339()` into `String` fields, matching
the rest of the infra layer.

Four structural facts drive this proposal:

- The 10-column page projection is **written out three times** (:94–107, :208–219, :241–252) even
  though `PageSummaryRow` is already declared — the alias is used only by the mapper signature, not
  by the queries. A column addition must be made in four places. `version_from` also carries a dead
  `#[allow(clippy::too_many_arguments)]` (:68): it takes one tuple, not many arguments.
- `pages_save` checks `find_by_nonce` in one transaction (usecases.rs:1498–1502) and writes in another
  (`put_version`); the partial unique index `pages_space_creator_nonce` (migration :21–23) is the
  real backstop, but `put_version` maps every driver error to `Unavailable` (:119, :147, :177,
  :199, :207, :223). A concurrent replay therefore surfaces as an outage instead of the existing
  page. The same is true of a `page_versions` UNIQUE(page_id, version) collision.
- Version numbering is `SELECT coalesce(max(version),0)+1` with no row lock (:152–158), under
  READ COMMITTED. Two concurrent `put_version` calls on one page can compute the same `next`; one
  then fails the UNIQUE constraint and is reported as `Unavailable`. The write also uses two clock
  sources: client `created_at` for the INSERT/UPDATE (:174, :183, :196) but `now()` for the final
  `updated_at` (:201).
- "Current version" has **two definitions**: `get` resolves it with `max(version)` (:269–276), while
  `shared_by_token_hash` follows `pages.current_version_id` (:346–349). `migrations/0037_pages.sql`
  declares no FK from `current_version_id` to `page_versions.id`, so a stale/dangling pointer makes
  the anonymous reader see nothing while a member still sees the latest version. `share` and
  `revoke_share` (:295–336) never inspect `TenantContext`, so a `System` context runs with
  `boma.is_system()` and can flip any page by id — unlike `MemoryPageStore`, which rejects it
  (memory.rs:6857, :6875). (GUESS: no system caller was found among the `pages_*` usecases; reasoned
  from code, not executed.)

## Guiding principle for v0.4.0

Keep the port signatures, the `Unavailable`/`NotFound` variants, the uniform-404 path and every
wire-visible field byte-compatible: `spec/golden/shared-page.json`, `shared-page-invalid.json` and
the two REQ-037 UI goldens pin this behavior, and any externally visible delta is a DEVIATION for
`docs/PARITY.md` (AGENTS.md). Improve structure by single-sourcing projections and "current", and by
making the store honest about replays and concurrency — never by weakening the share gate or
distinguishing unknown from revoked tokens.

## Concrete upgrades (5), each with a risk

1. **Use one page projection everywhere; drop the dead lint allow.** Apply `PageSummaryRow` as the
   `query_as::<_, …>` type in all three SELECTs (:94–107, :208–219, :241–252), or move it to a typed
   `sqlx::FromRow` DTO in `stores/rows.rs` with one `impl From<…> for PageSummary`. Remove the
   `too_many_arguments` allow on `version_from` (:68). *Risk:* `query_as` maps by position, so an
   ordering slip can silently swap two `String` fields; a `FromRow` DTO relies on exact aliases.
   *Mitigation:* keep the alias as the only source of column order (or alias all ten columns
   explicitly), add one row-mapping unit test, and replay both REQ-037 goldens before/after.

2. **Make `put_version` idempotent at the store boundary.** When the page INSERT hits
   `pages_space_creator_nonce`, re-read the existing page by `(space_id, created_by, client_nonce)`
   and return it instead of `Unavailable`; keep `find_by_nonce` as the fast path for the usecase.
   *Risk:* several unique indexes exist (`pages_pkey`, `pages_token_hash`, `page_versions_pkey`), and
   a blanket unique-violation match could return an unrelated page on a genuine id collision.
   *Mitigation:* match the violated constraint name via `tenant::is_unique_violation` plus the
   `DatabaseError::constraint()` API, re-read only on the nonce index, and add a fault-injection test
   that races two writes with the same nonce and asserts exactly one page row.

3. **Remove the version-allocation race and the mixed clock.** Lock the parent `pages` row with
   `SELECT … FOR UPDATE` before computing `max(version)+1` (:152–158), or use
   `INSERT … ON CONFLICT (page_id, version) DO NOTHING RETURNING` with a bounded retry; take
   `updated_at` from the same `created_at` value used for the other writes (:201). *Risk:* an extra
   row lock adds contention and a deadlock surface; a retry loop can hide a real fault, and version
   numbers are wire-visible. *Mitigation:* use a bounded retry (e.g. 3), preserve contiguous
   numbering and the `version >= 1` CHECK, and add an integration test with concurrent writers that
   asserts distinct versions and one current pointer.

4. **Single-source "current version".** Make `get` resolve the unpinned version from
   `pages.current_version_id` (falling back to `max(version)` for legacy rows) so members and the
   anonymous reader resolve the same document; propose a FK
   `pages(current_version_id) REFERENCES page_versions(id)` in a follow-up migration.
   *Risk:* for any row where the pointer is stale or NULL, output changes — an externally visible
   delta; the migration is another worker's scope and backfilling can be expensive.
   *Mitigation:* land the read change with the fallback first and a seeded-row test comparing both
   paths; route the FK/backfill through an ADR and `docs/PARITY.md` before touching the schema.

5. **Align `System` context handling across the two adapters.** Decide the contract explicitly:
   either guard `share`/`revoke_share` (:295–336) with the same member-only check the other methods
   use (and that `MemoryPageStore` already has), or document a `revoke_share_in_space` primitive for
   a future moderation path. *Risk:* the guard removes a latent cross-tenant capability; if a system
   caller exists outside the `pages_*` usecases, it breaks. *Mitigation:* grep all `pages` store call
   sites before landing, keep RLS as the backstop, and add a test asserting a `System` context
   cannot change another space's `visibility`.

## Cross-cutting risks / sequencing

- Sequence 1 → 4 → 2 → 3 → 5: projection and mapping safety first, then the "current" definition,
  then replay/idempotency, then concurrency, then the context contract.
- The `PageStore` port, `pages_save`/`pages_shared` orchestration, `MemoryPageStore`, migration 0037
  and the RLS policies are other workers' scope: coordinate items 2–5, do not duplicate edits.
  No new dependency is needed for items 1–5.
- Gate each step with `cargo fmt --check`, `clippy -D warnings`, the REQ-037 integration/golden and UI
  golden suites, and `tools/trace_check.py` (REQ id in test names). `pages.rs` has no tests today;
  add focused ones beside the new seams.

## Out of scope for this module

Token generation, hashing and the uniform-404/noindex/no-referrer response (usecases + API layer),
the `boma_domain::page` document model, `migrations/0037_pages.sql` itself, and the shared-page rate
limit (REQ-053 / defect D-01) are left to their owners. This proposal only restructures the adapter.
