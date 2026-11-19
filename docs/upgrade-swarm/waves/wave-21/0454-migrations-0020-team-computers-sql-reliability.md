# 0454 — migrations/0020_team_computers.sql — reliability (v0.4.0)

Worker #454 · module `migrations/0020_team_computers.sql` · lens: reliability · READ-ONLY proposal
against `before-m5` (v0.3.0-rc5 era). Proposals only; no product code changed. **Additive only**: no
edit to 0020 — every item below is a NEW later migration (0062+, leaving 0060/0061 to siblings) or a
new test. **CONFIRMED** = read from source this session; **GUESS** = verify before shipping. No tests run.

## Ground truth (CONFIRMED)

- 0020: `UPDATE computers SET private = true WHERE NOT private;` then `CREATE UNIQUE INDEX
  computers_team_per_space ON computers (space_id) WHERE NOT private;`. No `IF NOT EXISTS`, no
  post-condition check, no `-- no-transaction` marker (sqlx wraps it in one transaction — `db.rs` L20/L72-74).
- 0017 L23: `private boolean NOT NULL DEFAULT false`; L83-91: `ENABLE` + `FORCE ROW LEVEL SECURITY`
  and policy `USING/WITH CHECK (space_id = current_space_id() OR is_system())` on `computers`.
- 0002 L27-42: `current_space_id()` reads `boma.space_id` (unset → NULL → deny); `is_system()` is true
  iff `boma.system_reason` is non-empty. `db.rs` `MIGRATOR.run` sets **neither** GUC.
- `crates/domain/src/tenancy.rs` L29/L59: the closed migration system reason is `data_migration`;
  `tenant.rs` L120-126 sets GUCs with `set_config(..., true)` (transaction-local).
- `stores/computer.rs` L52-72: team read is `WHERE space_id = $1 AND NOT private` + `fetch_optional`;
  `create` binds `private` explicitly (L119). 0017 L32 keeps `UNIQUE(space_id, bot_id)` with
  `bot_id NOT NULL`; the team-row sentinel collision is #448 A5, not re-proposed here.

## What it already does right (keep)

- One statement backfills every per-bot row to explicit isolation; the partial unique index then
  enforces "at most one team computer per space" in the DB, not only in the store. `FORCE RLS` is
  inherited from 0017, so 0020 adds no new bypass surface.

## Failure modes

### F1 — CRITICAL — RLS can turn the backfill into a silent no-op (CONFIRMED mechanism, GUESS on role)

`FORCE RLS` binds the table owner. Migrations run with no `boma.space_id` and no `boma.system_reason`,
so for a least-privilege owner role every row is invisible: `UPDATE ... WHERE NOT private` touches
**zero** rows and still "succeeds". Consequences: (a) pre-existing rows stay `private = false`, i.e.
per-bot homes become *team/shared* — a REQ-031 isolation regression shipped silently; (b) the partial
index is now built over all those rows, so any space with ≥2 computers fails the build with a duplicate
key and blocks startup. With a superuser / `BYPASSRLS` migration role the same file works perfectly.
The outcome is deployment-role-dependent — the worst kind of fragility.

### F2 — HIGH — index sees physical rows, backfill sees RLS-filtered rows (CONFIRMED)

Indexes are built from the physical table, not through RLS. Between F1's two statements the same predicate
`NOT private` is evaluated in two visibility worlds; a plain guard is equally blind and reports success while
rows stay shared. Only `data_migration` context (or BYPASSRLS) can verify.

### F3 — MED — default contradicts the comment (CONFIRMED; intent GUESS)

0017 defaults `private` to `false` (shared), but 0020's comment says "`private` was the default before this
migration". App inserts bind it explicitly, so only manual/psql inserts omitting the column create team
computers by default. spec/02 is silent — record the intended default in `docs/STATE.md` before changing it.

### F4 — MED — constraints: reclassification and coherence are unguarded (CONFIRMED)

The index limits team rows per space but nothing stops flipping a live computer to/from `private`
while `computer_runs` / `computer_execution_leases` still reference it (a run can be using a computer
that suddenly becomes isolated), nor ties a team row's `bot_id` to a bot of that space. A system-path
writer can set the row's `space_id` inconsistently with its `bot_id`; RLS then hides it from the
rightful tenant. `private NOT NULL` also means `WHERE NOT private` silently excludes NULL if a later
hotfix makes the column nullable.

### F5 — MED — permissions, idempotency, locking (CONFIRMED/GUESS)

`CREATE UNIQUE INDEX` needs ownership of `computers` (0017's own DDL proves the migrating role had it
once — verify the prod deploy role still does). Re-running 0020 outside the sqlx ledger fails on the
bare `CREATE UNIQUE INDEX`; editing 0020 in place breaks its checksum and startup. The non-concurrent
index build takes a SHARE lock blocking writes to `computers` — negligible at today's size (GUESS),
costly on a large history.

## Concrete hardening (additive, prioritized)

- **P0 — F1/F2:** NEW `0062_team_computers_backfill_repair.sql`. Begin with
  `SELECT set_config('boma.system_reason', 'data_migration', true)` (closed reason from tenancy.rs;
  transaction-local, reverts at commit), re-run `UPDATE computers SET private = true WHERE NOT
  private`, then `DO $$ BEGIN IF EXISTS (SELECT 1 FROM computers WHERE NOT private) THEN RAISE
  EXCEPTION ... END IF; END $$;` plus a `GET DIAGNOSTICS` row count. Guard the index with `IF
  to_regclass('public.computers_team_per_space') IS NULL THEN CREATE UNIQUE INDEX ...`. *Risk:* a
  deliberately kept team row would trip the assertion — confirm "zero" with the owner (it matches
  0020's intent); the GUC must be `true`-scoped or a pooled connection inherits a cross-tenant bypass
  (D-09). Do not set `boma.space_id`: one space cannot backfill all spaces.
- **P0 — tests:** integration test migrating a fresh DB with a **least-privilege owner** (not
  superuser/BYPASSRLS), then asserting through a `data_migration` connection that no row has
  `private = false`. Add a deploy preflight logging the migration role's `rolsuper`/`rolbypassrls`.
  *Risk:* testcontainers cost; the preflight must not print credentials.
- **P1 — F3:** NEW `0063_computers_private_default.sql`:
  `ALTER TABLE computers ALTER COLUMN private SET DEFAULT true` if the owner confirms isolation is
  the intended default. *Risk:* visible behavior change for omitting callers → ADR + `docs/PARITY.md`
  DEVIATION unless parity with the oracle is proven.
- **P1 — F4:** NEW `0064_computers_reclass_guard.sql`: a BEFORE UPDATE trigger rejecting a `private`
  flip while an open `computer_runs` row or unexpired lease exists, plus a check that a team row's
  `bot_id` belongs to its `space_id` (a composite FK needs `UNIQUE(space_id, id)` on `bots` —
  coordinate with #448 A4/A5). *Risk:* triggers change write semantics; audit existing rows first;
  the team-row sentinel is still an open product decision.
- **P2 — F5:** if the table grows, rebuild the index as `CREATE INDEX CONCURRENTLY` in a separate
  non-transactional step and drop the old one. *Risk:* sqlx migrations are transactional by default;
  CONCURRENTLY cannot run inside them and a failure leaves an INVALID index needing manual cleanup.
- **P2 — F4:** instead of a redundant `CHECK (private IS NOT NULL)`, add a CI assertion that no later
  migration drops the NOT NULL, plus a cross-space `bot_id` write test for a non-system role. *Risk:*
  an intentional future nullable column fails the check — allow-list it.

## Explicit non-goals / rejected

No edits to 0020/0017, no drops/renames/type changes, no weaker RLS, no bypass other than the closed
`data_migration` reason, no auto team promotion. Sentinel `bot_id` and the intended `private` default
go to `docs/STATE.md` as decisions, not guesses.

## Rollout and cross-cutting risks

Wave A: P0 repair + least-privilege test + preflight (fix the privacy regression first). Wave B: P1
default/trigger after owner confirmation. Wave C: P2 concurrent rebuild when data size warrants it.
Every guard needs a pre-migration audit and a downgrade (spec 04 §4.9). F1's fix is data-only; F3/F4
may be externally visible → ADR/DEVIATION. REQ-tag tests; keep ids/PII out of NOTICE logs.

## Suggested acceptance tests (REQ-tagged)

- `req_031_backfill_is_not_a_noop_under_force_rls_for_a_least_privilege_owner`
- `req_031_no_computer_is_left_shared_after_the_team_migration`
- `req_031_a_computer_cannot_flip_private_while_a_run_holds_it`
- `req_043_migration_uses_only_the_data_migration_system_reason`
