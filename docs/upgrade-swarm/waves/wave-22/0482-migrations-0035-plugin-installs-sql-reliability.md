# 0482 — migrations/0035_plugin_installs.sql — reliability (v0.4.0)

Worker #482 · module `migrations/0035_plugin_installs.sql` · lens: reliability · READ-ONLY review of
`rebuild-workspace/.worktrees/before-m5` at v0.3.0-era source. Proposals only; no code changed.
Line refs are against this worktree. Claims tagged CONFIRMED (read this session) or GUESS (inferred);
no tests were run (read-only). Boundaries consulted: `crates/infra/src/stores/plugin_installs.rs`
(L39–41 list, L77–86 upsert, L113–115 skill delete), `crates/app/src/usecases.rs` (L1328–1447
install flow), `apps/web/lib/api/rpc.ts` (L1096–1104 view), `spec/02-data-model.md` L142–143,
peers `0034_connector_installs.sql`, `0028_skills.sql`, `0045_audit_cors.sql`, and wave-21/20
migration reliability docs (NOT VALID → VALIDATE, CONCURRENTLY, BYPASSRLS conventions).

## What it already does right (keep, do not re-propose)

- `UNIQUE (space_id, source)` (L17) gives one install per source per space; `ON DELETE CASCADE` on
  `space_id` (L9) prevents orphan rows when a space is deleted (CONFIRMED).
- `ENABLE` + `FORCE ROW LEVEL SECURITY` and a symmetric `USING`/`WITH CHECK` tenant policy
  (L20–24) fail closed without a space context and block cross-tenant writes (CONFIRMED).
- `parts`/`skipped` are `NOT NULL` with `{}`/`[]` defaults (L13–14), so a read never deserializes
  NULL (CONFIRMED); `created_at`/`updated_at` are server-defaulted (L15–16).

## Failure modes → concrete hardening

### F1. State integrity: a reinstall is three non-atomic writes

- `marketplace_install` deletes the old plugin skills, inserts the new skills one-by-one, then
  records the install row — each via its own `with_tenant` connection, with no surrounding
  transaction (usecases.rs L1344–1412, store L113–115/L77–86; CONFIRMED). A crash/refused skill or
  an MCP-registration error on the way through leaves the old skills gone, a partial new bundle,
  and either no row or the *previous* row still present — the "reinstall replaces skills" contract
  (0035 header, store L3–5) is violated and the omission report goes stale. P1. Document and test a
  single-transaction requirement for the three steps at the app boundary (outside this module's
  write scope). P2. Additively add `install_epoch bigint NOT NULL DEFAULT 1` and an immutable
  `first_installed_at timestamptz NOT NULL DEFAULT now()` so a partial install is detectable and the
  row's identity can be reconciled with the skills it owns. *Risk:* a transaction is an app change,
  not addable in SQL alone; keep defaults so existing rows migrate cleanly; do not auto-repair from
  a trigger (would hide the defect rather than fix it).

### F2. State integrity: `id` drift and `created_at` reset on conflict

- The upsert does not update `id` and *overwrites* `created_at = now()` on conflict (store L77–86),
  so the PK keeps the first install's id while `record_install` returns a fresh `pin-…` id
  (usecases.rs L1427) and the list orders by this rewritten `created_at` (store L39–41). The
  returned id can disagree with the stored row, and install history/timestamp is lost (CONFIRMED).
  P3. Additively add `installed_at timestamptz` (nullable, app-set on each install) and keep
  `created_at` as the immutable first-install time going forward; order lists by `installed_at`
  with a `COALESCE` fallback. P4. Add a test asserting the id returned by the RPC equals the row's
  `id` after a reinstall. *Risk:* changing the returned id or list order is an externally visible
  behavior change → DEVIATION in `docs/PARITY.md` + owner approval (AGENTS.md); default the new
  column NULL in v0.4.0 so behavior is unchanged until explicitly switched.

### F3. Constraints: no shape, length, or format guards on the contract columns

- `parts`, `skipped`, `source`, `name`, `resolved_sha` have no CHECKs (L10–14). The UI contract is
  precise — `parts = {skills:int, mcpServers:int}`, `skipped` an array of `{kind,name,reason}`
  (usecases.rs L1433, rpc.ts L1096–1104) — but a buggy writer can store a scalar/array `parts` or an
  object `skipped`, which the list path returns verbatim (store L47–58; GUESS on downstream
  rendering tolerance). Unbounded text is a storage sink. `0034` shows the repo's CHECK-enum
  precedent (0034 L10/L13). P5. In a NEW forward-only migration (never edit 0035), ADD CHECKs
  `jsonb_typeof(parts)='object'`, `jsonb_typeof(skipped)='array'`, `length(source) <= 2048`,
  `length(name) <= 256`, non-empty `source`/`name`, and `octet_length(id) <= 128`, added
  `NOT VALID` then `VALIDATE` after an audit query. P6. Consider `resolved_sha ~ '^[0-9a-f]{40}$'`
  only after confirming every fetcher SHA is 40-hex (GUESS). *Risk:* dirty rows block VALIDATE —
  gate on the audit and repair additively, never relax the rule; caps must be sized from real
  payloads (GUESS on current sizes) or legitimate installs are refused.

### F4. RLS: FORCE semantics and cross-tenant coherence with `skills`

- The policy trusts the row's `space_id` (L23–24) but the install owns `skills` rows in a *different*
  table keyed by `plugin_source` only (store L113–115; `0028` L13). Nothing in the DB ties a skill's
  `space_id` to its install's `space_id`; a writer bug or a `boma.is_system()` path could leave a
  skill visible under space A while its install report lives in space B (analysis; GUESS that no
  caller does this today). FORCE RLS also subjects validation/maintenance scans to the policy, so a
  `VALIDATE CONSTRAINT` or index build run by the table owner can silently see a filtered set
  (peer wave-21 R1). P7. Add a test that a non-system role cannot `UPDATE ... SET space_id` across
  tenants, and that a same-source skill from another tenant is never rewritten. P8. Run all
  validation/backfill under a BYPASSRLS/`is_system()` role; keep the `is_system()` escape hatch and
  FORCE semantics byte-identical — never widen the policy. *Risk:* a composite FK from `skills`
  to `plugin_installs(space_id, source)` would enforce coherence but needs a backfill proof and a
  UNIQUE key already present; stage separately if pursued.

### F5. Access path: no index serves the list query

- `list` orders by `created_at DESC, id ASC` (store L41); the only index is the UNIQUE
  `(space_id, source)` (L17), which cannot serve that order, so a seq-scan-plus-sort grows with
  install history (CONFIRMED from schema; GUESS on table size). P9. Add `CREATE INDEX CONCURRENTLY
  plugin_installs_space_installed ON plugin_installs (space_id, created_at DESC, id)`. *Risk:*
  CONCURRENTLY cannot run inside the migration transaction — stage as a separate statement and
  verify the framework supports it (wave-21 convention); if F2 lands, index `installed_at` instead
  in the same step to avoid a second build.

## Explicit non-goals / rejected

- No edits to 0035, no drops/renames/type changes, no weakening of RLS or new non-tenant bypass:
  every item is a new forward-only migration (`ADD COLUMN/CONSTRAINT` `NOT VALID`, new indexes,
  tests). Out-of-scope features go to `docs/STATE.md`, not into this migration.
- No automatic cleanup/sweeper and no trigger-driven skill reconciliation in v0.4.0 (behavior
  changes → DEVIATION + ADR + owner approval).

## Rollout (v0.4.0) and cross-cutting risks

- Wave A (safe, additive): P5 audit query + CHECKs `NOT VALID`/`VALIDATE`, P9 CONCURRENTLY index,
  P2 columns with harmless defaults, P4/P7 RLS and id-consistency tests. Wave B (gated): P3
  ordering behavior (DEVIATION/ADR), F4 composite FK after backfill proof. Wave C (owner approval):
  F1 app-layer transaction and any auto-repair.
- R1. All constraint validation and backfills must run under BYPASSRLS/is_system or FORCE RLS can
  hide violating rows. R2. Cross-tenant and double-reinstall paths must be covered by REQ-034-named
  tests before shipping. R3. This table shares the `plugin_source` namespace with `skills` (0028)
  and the no-local-code contract with `plugin.rs` (wave-14 #310); coordinate any shape change with
  those owners so the omission report stays truthful.
