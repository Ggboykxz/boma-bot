# Worker #415 — `migrations/0001_init.sql` — architecture (v0.4.0)

- Module: `migrations/0001_init.sql` (18 lines, M0 bootstrap; REQ-043 / ADR-001 / ADR-002);
  lens: architecture; mode: READ-ONLY proposal, no code changed, no tests run.
- CONFIRMED by file read this session: `0001_init.sql:1-18`; `0002_m1.sql:19-21,27-60,62-280`
  (helpers, global/tenant tables, policies); `0046_account_exports.sql:17-21`,
  `0059_acp_agents.sql:15-21` (RLS conventions); `ADR-001:36-49`, `ADR-002:36-56`;
  `STATE.md:1494-1495` (`_sqlx_migrations` ledger). `grep` found `app_meta` only in this file
  (plus that STATE line) — no later migration or code references it. Upgrades and risks are GUESS.
- No wire/API/data-format change is proposed. Every upgrade is a NEW forward-only migration file;
  anything externally visible would be a DEVIATION needing `docs/PARITY.md` + owner approval.

## 1. Current structure (CONFIRMED)

- 0001 is a deliberate no-domain bootstrap: one table `app_meta (key text PK, value jsonb NOT NULL,
  updated_at timestamptz NOT NULL DEFAULT now())`, one seed row `('schema','{"milestone":"M0"}')`
  via `ON CONFLICT DO NOTHING`, one COMMENT. Nothing else.
- No RLS, no FK, no reference to the `boma` helper schema — that schema is created only in
  0002:21. Zero dependencies is the invariant: 0001 must apply on a completely empty database so
  `boma migrate` can be observed end to end (comments L1-5).
- `app_meta` is global metadata, not tenant data; the COMMENT (L17-18) says "never tenant data,
  never readable by a client" — a convention today, not a grant or a policy.
- The real schema starts in 0002: helpers `boma.current_space_id()` / `is_system()` / `rfc3339()`
  read `current_setting('boma.space_id'|'boma.system_reason')`; four global tables (`users`,
  `organizations`, `organization_members`, `sessions`); eight tenant tables; `ENABLE` + `FORCE`
  RLS with `USING/WITH CHECK (... = current_space_id() OR is_system())`; `spaces` is its own tenant
  so its policy compares `id` instead of `space_id`.
- Version authority is sqlx's `_sqlx_migrations`, created by `boma migrate` next to `app_meta`
  (STATE.md:1494-1495). The `schema` row is informational and is never updated by later migrations.

## 2. Architecture findings (CONFIRMED unless marked)

1. **Checksum immutability forbids editing 0001.** sqlx verifies a hash of each applied migration;
   changing 0001's bytes breaks `boma migrate` on every deployed database. The additive rule and the
   tooling agree: "upgrade 0001" can only mean new files after it.
2. **A stale second source of truth.** The seed says `M0` forever; grep found no writer. GUESS: a
   future tool reading `app_meta` for "current milestone" gets a wrong answer.
3. **The confidentiality claim is unenforced.** No RLS and no GRANT restrict `app_meta`; the
   guarantee rests on client sessions never selecting it — unlike 0046/0059, where system-only or
   tenant policies are explicit. It holds only because the payload is a harmless literal.
4. **Drift tolerance.** `CREATE TABLE IF NOT EXISTS` + `ON CONFLICT DO NOTHING` let a re-run succeed
   even if `app_meta` was hand-altered; nothing asserts the 3-column shape or that it lives in
   `public`.
5. **`search_path` asymmetry.** 0002 pins `SET search_path TO public` with a comment about a role
   named `boma` capturing unqualified creates; 0001 does not. GUESS: low risk on a fresh DB (no
   `boma` schema yet), but 0001 is the one table later migrations rely on finding unqualified.

## 3. Concrete upgrades for v0.4.0 (with risks)

**U1 — Freeze 0001 and add a bootstrap contract test (structure, do first).** Never edit the file;
add a fresh-PG test running `boma migrate`, asserting `app_meta` has exactly `(key, value,
updated_at)`, the seed row, and that a second migrate is clean. Risks: (a) needs a disposable DB —
reuse the existing CI Postgres job; (b) pinning exactly 3 columns makes a future additive column
fail on purpose, so update the assertion in the same commit as that migration.

**U2 — Make "never readable by a client" enforceable (security, additive DDL).** New migration:
`ALTER TABLE app_meta ENABLE ROW LEVEL SECURITY; ALTER TABLE app_meta FORCE ROW LEVEL SECURITY;
CREATE POLICY app_meta_system_only ON app_meta USING (boma.is_system()) WITH CHECK
(boma.is_system());`, mirroring 0046. Risks: (a) depends on 0002's `boma` schema/function — safe by
ordering but state it; (b) the migrate binary must set `boma.system_reason` if any future migration
writes `app_meta`, else it fails closed; (c) ADR-002's CI enumeration only covers tables with
`space_id`, so add an explicit `app_meta` assertion; (d) FORCE RLS changes access semantics with no
data change — re-verify `boma migrate` and `boma serve` still pass.

**U3 — Additive shape constraints (correctness).** New migration: `ALTER TABLE app_meta ADD
CONSTRAINT app_meta_key_format CHECK (key ~ '^[a-z][a-z0-9_]*$');` plus a `boma` trigger function
and `CREATE TRIGGER` to bump `updated_at` on UPDATE today's write-once column. Risks: (a) the CHECK
rejects a future dotted key — keep it permissive and flag as GUESS; (b) existing key `schema`
passes; (c) trigger changes write behaviour, but there is no current writer; (d) use ADD/CREATE
only, never `ALTER COLUMN TYPE`.

**U4 — Retire the stale version signal without touching the old row (hygiene).** New migration
inserts `ON CONFLICT DO NOTHING` an additive key such as `('app_version','"0.4.0"')` and a COMMENT
recording that `_sqlx_migrations` is authoritative. Risks: (a) still two sources — no tool may
branch on `app_meta`; (b) a version literal in SQL must be bumped every release — add a CI check
against the Cargo version; (c) do NOT overwrite the historical `schema` row (a data change).

**U5 — Pin and lint the bootstrap boundary (process).** Add a CI grep requiring every migration
that creates unqualified objects to set `search_path`, and a new-migration canary asserting
`to_regclass('public.app_meta') IS NOT NULL`. Risks: (a) a migration's `SET` is session-scoped — the
canary detects misplacement, it cannot retroactively fix 0001; be honest about that; (b) the real
fix would edit 0001 (forbidden by U1/checksum), so defer it as a documented invariant; (c) the lint
must not flag the deliberate `SET` in 0002.

## 4. Ordering, invariants, non-goals

Order **U1 → U2 → U3 → U4 → U5** (freeze/test, then enforcement, shape, hygiene, lint). Invariants:
0001 stays byte-for-byte unchanged; no DROP / RENAME / ALTER TYPE / DELETE anywhere; `app_meta`
stays global, non-tenant and FK-free; RLS helpers come only from the `boma` schema created in 0002;
`.sqlx` offline metadata is refreshed when new migrations land; no secrets or tenant data in
`app_meta`. Non-goals: no multi-tenant `app_meta`, no moving domain tables into 0001, no
client-readable `app_meta`, no editing applied migrations, no change to the migration runner.

## 5. Evidence & tests to add

- CONFIRMED — this session's reads: `0001_init.sql:1-18`, `0002_m1.sql:19-21,27-60,62-280`,
  `0046_account_exports.sql:17-21`, `0059_acp_agents.sql:15-21`, `ADR-001:36-49`, `ADR-002:36-56`,
  `STATE.md:1494-1495`, plus the two `grep` results (`app_meta` sites, `search_path` only in 0002).
- GUESS — that no tool reads `app_meta` for a version; that the migrate binary can set a system
  reason (verify before U2); that the key regex is permissive enough; that CI has a disposable PG.
- Suggested names: `m0_bootstrap_migration_is_idempotent_and_shape_stable` (U1),
  `req_043_app_meta_is_system_only` (U2), `m0_app_meta_key_format_and_updated_at_trigger` (U3).

Line budget: <120. Status: proposal only; READ-ONLY worker, no code changed and no tests run.
