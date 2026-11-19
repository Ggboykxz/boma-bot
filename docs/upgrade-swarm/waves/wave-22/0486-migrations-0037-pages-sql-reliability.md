# Worker 0486 — `migrations/0037_pages.sql` — reliability (v0.4.0 proposal)

READ-ONLY review of `before-m5` (v0.3.0-rc5-era source). Auditor proposal only: no code, schema,
spec or golden changed. Claims tagged CONFIRMED (read this session) or GUESS (inferred). Every
suggestion is **additive** — a new migration (`0060_pages_hardening.sql`): `ADD CONSTRAINT ... NOT
VALID` + `VALIDATE`, new indexes/FKs, `CREATE TRIGGER`, `COMMENT ON` — never an edit to 0037, no
drops/renames/type changes, no RLS weakening. Visible behavior changes are ADR/DEVIATION candidates.

## What is already right (keep, do not re-propose)

- Fresh `CREATE TABLE` (no `ALTER` on a live table), so the 0019 DDL-lock class does not apply (CONFIRMED).
- `UNIQUE` partial indexes on `token_hash` and `(space_id, created_by, client_nonce)`, plus
  `UNIQUE(page_id,version)`, `version>=1` and `visibility` CHECKs (0037:12,20-23,28,31) (CONFIRMED).
- Both tables use `FORCE ROW LEVEL SECURITY` with `USING`/`WITH CHECK` tenant policies; the
  `page_versions` policy uses `EXISTS` on `pages` so a cross-tenant insert fails closed
  (0037:36-58); `page_versions.page_id` is `ON DELETE CASCADE` (0037:27) (CONFIRMED).
- Source validation is strict at write time (`domain/src/page.rs`: `deny_unknown_fields`, block
  caps, 256 KiB `PAGE_SOURCE_MAX_BYTES`) (CONFIRMED).

## Failure modes

### Versioning

**F1 — `next = max(version)+1` is a TOCTOU (CONFIRMED code path).** `put_version` reads the max
then inserts (`infra/src/stores/pages.rs:152-158,188-199`) with no `SELECT ... FOR UPDATE` on the
page row. Two concurrent writers compute the same `next`; `UNIQUE(page_id,version)` rejects one,
and the store maps SQL errors to `tenant::unavailable` — the loser sees a misleading 503, not a
retry/409. GUESS: triggered by a bot re-publishing while a human edits.

**F2 — Two competing definitions of "current".** Member `get` resolves `max(version)` (pages.rs:
267-276); the public read joins `p.current_version_id = v.id` (pages.rs:345-350). If the pointer
is stale or corrupted (import/manual repair, or any path that writes a pointer naming a version
that is not the newest) the member view and the shared view show different documents. GUESS on
transactional wiring of `tenant::with_tenant`; the divergence is still possible from skew.

**F3 — The public join omits `v.page_id = p.id`.** `shared_by_token_hash` joins solely on
`v.id = p.current_version_id` under `as_system(SystemReason::SharedPageRead)` (pages.rs:343-355).
A corrupted pointer to another page's version — even another tenant's — produces that version to
an anonymous caller, bypassing RLS by design. Associated: there is no FK from
`pages.current_version_id` to `page_versions(id)` and no immutability guard on `page_versions`, so
a version's `source` can be edited in place after it has been shared (CONFIRMED absence).

**F4 — App-supplied `compiled_at`.** `put_version` binds `created_at` over `DEFAULT now()`
(pages.rs:196) with no DB floor, so a skewed clock yields a nonsensical "compiled" time (CONFIRMED).

### RLS / access path

**F5 — The anonymous path is a deliberate RLS bypass and a single narrow trust surface.** All
cookieless reads run as system (usecases.rs:1590-1601 → pages.rs:343): correct today, but any bug in
that one query is a system-wide read with no second layer (CONFIRMED shape, GUESS on exploitability).

**F6 — `find_by_nonce` omits `space_id`.** It filters `created_by` + `client_nonce` only
(pages.rs:113), relying on RLS; a future system-ish helper or policy change turns it into a
cross-space lookup (GUESS; no current caller does this).

### Constraints

**F7 — Write-time validation is not a DB guarantee.** `source jsonb`/`name` have no type/size CHECK
(0037:11,29): an import/bypass row can hold a scalar or oversized source, and the public path
re-serves raw JSON without revalidating (pages.rs:345-355) — malformed or unreviewed content reaches
the viewer (REQ-037/REQ-039 trust link).

**F8 — Visibility/hash pair is unenforced.** Nothing stops `visibility='link'` with NULL hash or
`'private'` with a leftover hash; `share`/`revoke` write both, but partial writes/import skew leave
incoherent state (CONFIRMED code, GUESS on occurrence).

**F9 — Deleting the creator cascades away shared pages.** `created_by ... ON DELETE CASCADE`
(0037:10) removes page, versions and any live link on account deletion (GUESS on policy; note only).

## Concrete hardening (additive, with risks)

1. **Enforce pointer integrity (F2,F3).** Add `page_versions_id_page_uniq UNIQUE (id, page_id)` and
   `pages_current_version_fk FOREIGN KEY (current_version_id, id) REFERENCES page_versions(id,
   page_id) ON DELETE SET NULL`, both `NOT VALID` then VALIDATE. *Risk:* circular FK and a new
   unique index on a growing table — build `CONCURRENTLY`; skew blocks VALIDATE, so ship a read-only
   audit query first and repair, never relax.

2. **Make versions immutable (F3,F4).** Additive `BEFORE UPDATE OR DELETE ON page_versions` trigger
   raising an exception unless a maintenance/system role is active, plus `COMMENT`. *Risk:* blocks
   import repair — enumerate those paths and allow a narrow bypass; guard with
   `WHEN (OLD.source IS DISTINCT FROM NEW.source ...)` for negligible cost.

3. **Pin the pair and the shape (F7,F8).** `pages_share_pair_check CHECK ((visibility='link') =
   (token_hash IS NOT NULL))`, `token_hash_len CHECK (octet_length(token_hash) = 32)` (when
   non-null), `source_object CHECK (jsonb_typeof(source)='object' AND pg_column_size(source) <=
   262144 + margin)`, `name_len CHECK (length(name) <= 120)` — all `NOT VALID` then VALIDATE.
   *Risk:* caps must match `PAGE_SOURCE_MAX_BYTES`/`PAGE_NAME_MAX`; existing violating rows block
   VALIDATE (expected zero). Do not let the CHECK substitute for read-side validation (item 5).

4. **Harden the public read (F3,F5).** Add `AND v.page_id = p.id` and tighten to
   `p.visibility='link' AND p.token_hash=$1 AND p.current_version_id=v.id`. Prefer an additive
   `SECURITY DEFINER` function `boma.shared_page(hash)` with pinned `search_path`, granted to the
   anonymous role, over a broad system read; `COMMENT` the trust boundary. *Risk:* SECURITY DEFINER
   is a new attack surface — pin search_path, use a dedicated owner, re-run uniform-404 goldens.

5. **Revalidate at read (F7).** Call `validate_page_source` on JSON from `shared_by_token_hash`,
   degrading to uniform 404 on failure. *Risk:* a previously valid stored document could go dark —
   ADR/DEVIATION candidate; gate in v0.4.0 if legacy rows exist.

6. **Close the version race (F1).** In `put_version`, take `SELECT id FROM pages WHERE id=$1 FOR
   UPDATE` before the max read, or retry on a unique violation and map exhaustion to a legible
   conflict, not `unavailable`. *Risk:* lock contention; a constraint alone prevents corruption but
   not the 503, so this code fix is the real remedy.

7. **Add `space_id` to `find_by_nonce` (F6)** as defense in depth; harmless under RLS. Record F9 in
   `docs/STATE.md` — changing deletion behavior is a DEVIATION.

8. **Tests first, REQ-037 in every name.** `req_037_concurrent_version_puts_do_not_503`,
   `req_037_current_pointer_names_a_same_page_version`, `req_037_versions_are_immutable`,
   `req_037_share_pair_and_source_shape_checks`, `req_037_shared_read_rejects_cross_page_pointer`,
   `req_037_migration_is_idempotent`, plus replay of `shared-page.json`,
   `shared-page-invalid.json` and `ui/REQ-037-shared-page*.json` for byte-identical uniform 404.

## Recommended order for v0.4.0

Ship 3, 4 (predicate fix), 7 and 8 first — additive, low risk, no behavior change. Then 1 and 2
behind backfill audits. Treat 5 (read-side revalidation), 6 (conflict semantics), F9 and any
SECURITY DEFINER redesign in 4 as ADR-gated DEVIATION candidates needing owner approval and
`docs/PARITY.md` entries. Keep RLS strict; never add a non-tenant bypass to serve a public page.
