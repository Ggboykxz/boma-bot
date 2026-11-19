# Worker #485 — `migrations/0037_pages.sql` — architecture (v0.4.0)

- Worker #485 · Lens: architecture (structure, boundaries, coupling, debt) · Target: v0.4.0.
- Status: PROPOSAL, READ-ONLY. No migration, code or doc changed; no tests run. CONFIRMED = direct
  read this session; GUESS = inference or a claim from a sibling brief not re-verified here.
- Module: `migrations/0037_pages.sql` (58 lines), the REQ-037 schema for a page (a block document),
  its immutable `page_versions`, and the link-share token (only its hash is stored).
- Read for context (CONFIRMED): `0036_artifacts.sql`, `0038_tickets.sql` (house style, additive
  extensions); `crates/infra/src/stores/pages.rs` (:338-359 `shared_by_token_hash`, `as_system`
  read; `max(version)+1` allocation); `crates/domain/src/page.rs`; `crates/app/src/usecases.rs:1590`;
  `crates/api/src/router.rs:68`, `public.rs:162`; `crates/infra/src/db.rs:20` (`sqlx::migrate!`);
  `spec/01-requirements.md:305-313` (REQ-037), `:316` (REQ-039), `spec/02-data-model.md:115-128`.

## 1. Current structure (CONFIRMED)

Two tables, two partial-unique indexes, one index, two RLS blocks:
- `pages` (:6-18): PK `id`; FKs `space_id` CASCADE, `bot_id` SET NULL, `created_by` CASCADE; `name`;
  `visibility CHECK IN ('private','link')`; nullable `token_hash bytea`, `current_version_id text`,
  `client_nonce`; `created_at`/`updated_at`. Unique partial `pages_token_hash` (:20) and
  `pages_space_creator_nonce` (:21-23).
- `page_versions` (:25-32): PK `id`; `page_id` CASCADE; `version >= 1` with `UNIQUE(page_id,version)`;
  `source jsonb`; `compiled_at`. Cover index `(page_id, version DESC)` (:34).
- RLS (:36-58): both tables ENABLE + FORCE with tenant-isolation policies; the version policy is an
  `EXISTS` join back to `pages`, so the parent's RLS decision is duplicated into every child query.

Four architectural traits:
1. **The public-read contract is invisible to the schema.** REQ-037's anonymous fetch is implemented
   only as `as_system(SystemReason::SharedPageRead, SELECT v… JOIN p… WHERE visibility='link' AND
   token_hash=$1)` in the store (`pages.rs:343-349`). No view, function, role or grant represents it.
   With `FORCE RLS`, the whole public surface rests on one application-side context switch.
2. **The visibility↔token invariant is not DB-enforced.** The data model says "link visibility
   requires a token hash" (`spec/02:120`), but nothing stops `('link', NULL)` or `('private', hash)`.
3. **`current_version_id` is an unenforced reference.** Plain text (:14), no FK to
   `page_versions.id`. The public JOIN silently misses on a dangling id (uniform 404, so safe at the
   edge, but coupling and drift live in the app).
4. **Version numbering is app-computed**, `max(version)+1` under a transaction, made safe only by
   the `UNIQUE(page_id,version)` index. There is no DB-side counter structure (contrast
   `board_state` in `0038`), and no lifecycle/retention structure for immutable versions.

## 2. Guiding principle for v0.4.0

`0037` is immutable: SQLx verifies a checksum via `sqlx::migrate!` (`db.rs:20`), so editing its bytes
breaks `boma migrate` on every deployed DB. Every change ships as a NEW, additive file (e.g.
`0059_pages_public_read.sql`, `0060_pages_constraints.sql`) that only creates objects, adds
nullable/defaulted columns, or adds constraints via `NOT VALID` → `VALIDATE`. Never DROP / RENAME /
ALTER TYPE / DELETE / backfill rows in place. Keep token hashing, RLS expressions, FK cascade actions
and version semantics byte-compatible; any externally visible change is a DEVIATION for
`docs/PARITY.md` with owner approval.

## 3. Concrete upgrades (5), each with a risk

**U1 — Freeze `0037` and add an additive migration-shape contract test (structure; do first).** Never
edit the file; add a fresh-PG test that runs `boma migrate`, asserts the exact `0037` shape (two
tables, columns, PK/FKs/CHECK, three indexes, both enforced policies) and that a checksum edit is
rejected. *Risk:* a shape assertion drifts the moment U4/U5 add columns. *Mitigation:* assert
semantically from one shared expected-object constant and regenerate `.sqlx` offline metadata in the
same commit as any new migration.

**U2 — Represent the public-read seam as a narrow DB object, additively (boundary).** In a NEW
migration add a read-only `pages_public_current` view (and/or a `SECURITY DEFINER` lookup function
with fixed `search_path`) encapsulating exactly the store's JOIN/filter, so the anonymous contract
becomes greppable and auditable. *Risk:* a second source of truth can drift from the store query, and
a broad GRANT or a sloppy definer can widen exposure past `visibility='link'`. *Mitigation:* grant no
base-table privileges, keep it diagnostic in v0.4.0 with `as_system` still authoritative, and add a
parity test asserting view membership equals `shared_by_token_hash`.

**U3 — Enforce visibility↔token in the DB, additively (invariant).** New file:
`ALTER TABLE pages ADD CONSTRAINT pages_link_requires_token CHECK (visibility <> 'link' OR
token_hash IS NOT NULL) NOT VALID;` then `VALIDATE CONSTRAINT` after an audit query. *Risk:* any
existing `('link', NULL)` row blocks VALIDATE, and the revoke path must flip visibility and clear the
hash in one transaction or the state is momentarily inconsistent. *Mitigation:* audit first, leave it
`NOT VALID` until clean, and keep the paired state change in the store (app layer, out of scope here).

**U4 — Add a FK for `current_version_id`, additively (coupling).** New file: composite
`FOREIGN KEY (current_version_id, id) REFERENCES page_versions (id, page_id) NOT VALID` plus an
additive UNIQUE index on `page_versions (id, page_id)` — enforces that the current version both exists
and belongs to the page. *Risk:* dangling or cross-page ids block VALIDATE, and referencing the child
table changes delete ordering. *Mitigation:* `NOT VALID` + audit, keep the column nullable, and defer
`VALIDATE` to a reviewed step if data is dirty (GUESS: clean on this schema).

**U5 — Close the data-model field seam and add lifecycle structure (completeness).** `spec/02:120`
lists `shareSecretId` (and "slug/token hash") but `0037` has only `token_hash` (CONFIRMED). Add
nullable `share_secret_id text` and `token_rotated_at timestamptz` (no backfill) so the mapped fields
exist without a data change. *Risk:* the spec is ambiguous about whether slug/secret are one column or
two, so a guessed column could be wrong and unused columns are debt. *Mitigation:* record this as a
GUESS under `docs/STATE.md` → "Decisions needed", leave both NULL/unused in v0.4.0, and add no data
rewrite until the owner confirms the mapping.

## 4. Cross-cutting risks / sequencing

- Order **U1 → U2 → U3 → U4 → U5**: freeze/test first, then boundary, invariants, coupling, fields.
  Each ships as a separate additive file; U2-U5 only after their ADRs / decisions are approved.
- Coordinate with the reliability lens on this same file: this brief owns only migration structure;
  store transaction/rollback behavior, rate limiting and error mapping stay there. Freeze token hash
  format, RLS expressions, FK cascade actions and version semantics; regenerate `.sqlx` metadata and
  have CI run `boma migrate` on a fresh DB plus replay the REQ-037/REQ-039 suites.
- No new dependency; work is SQL + migration-level tests. Mark claims CONFIRMED/GUESS per `AGENTS.md`;
  gate with `cargo fmt --check`, clippy `-D warnings`, golden and contract suites.

## 5. Out of scope

The `PageStore` port/adapter, `pages_shared` usecase, public HTML/CSP rendering (REQ-039), cleanup
scheduling, and any in-place edit of `0037` — each belongs to another brief or is forbidden by the
checksum invariant.

## 6. Evidence & suggested tests

- CONFIRMED this session: `0037` (:1-58), `0036`, `0038`, `pages.rs` (:338-359), `usecases.rs:1590-1602`,
  `router.rs:68`, `public.rs:162`, `db.rs:20`, `spec/01:305-316`, `spec/02:115-128`.
- GUESS (re-verify before implementing): `shareSecretId` mapping is ambiguous; schema is clean enough
  to VALIDATE U3/U4; embedded PG as the `migrate` target (from sibling proposals, not re-checked).
- Suggested names: `req_037_migration_0037_is_immutable_and_shape_stable` (U1),
  `req_037_public_read_view_matches_store_query` (U2),
  `req_037_link_visibility_requires_token_hash` (U3),
  `req_037_current_version_belongs_to_page` (U4).

Line budget: <120. Status: proposal only; READ-ONLY worker, no code changed and no tests run.
