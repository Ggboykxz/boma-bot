# Worker 0470 — `migrations/0028_skills.sql` reliability review (v0.4.0)

Lens: reliability. Additive-only; no existing object is dropped, narrowed or rewritten.

## Baseline (CONFIRMED by reading this tree)

- `skills` is one row per `(space_id, name)` with `UNIQUE (space_id, name)`, `id text PRIMARY KEY`,
  `space_id ... REFERENCES spaces(id) ON DELETE CASCADE`, `origin IN ('taught','plugin')`,
  and `ENABLE`/`FORCE` RLS plus one tenant policy using `boma.current_space_id()` / `boma.is_system()`.
- `crates/domain/src/skill.rs` validates names as lowercase-kebab (<=64), descriptions 1..200 chars,
  instructions 1..32768 bytes. The DB enforces **none** of those rules.
- `crates/infra/src/stores/skills.rs::put` uses `ON CONFLICT (space_id, name) DO UPDATE` and sets
  `updated_at = now()` itself; `list`/`get`/`remove` carry no explicit space filter and depend on RLS.
- `crates/infra/src/tenant.rs` sets both GUCs with `set_config(..., true)` (transaction-local) inside
  a transaction that rolls back on error — the main tenant-leak path is already mitigated.
- `crates/infra/src/db.rs` embeds migrations via `sqlx::migrate!` with no `-- no-transaction`
  directive, so Postgres DDL is wrapped per migration.
- `crates/app/src/usecases.rs` (~1343) passes `Some(canonical)` for every plugin skill; taught skills
  pass `None`. The comment's "taught skills carry NULL" invariant is real in code but unenforced in SQL.

## Failure modes

**F1 — name integrity.** The unique key is case- and whitespace-sensitive: `Foo`, `foo`, `foo `
are three legal rows. A model loads by exact name; a near-duplicate is unaddressable or resolves
ambiguously. Nothing stops `''`, `with.dot`, `Upper`, a 10 MB name, or a name shadowing built-in
`interrogate` (built-ins are code, so the DB cannot know that list — app-only guard). Risk:
silent shadowing / injection of malformed names by any writer that bypasses the domain validator.

**F2 — RLS.** The policy is correct and fails closed (unset `boma.space_id` yields NULL = no match;
unset system reason = false). Residual risks are operational, not structural: (a) any future code
that calls plain session `SET` instead of `set_config(..., true)` makes a pooled connection carry a
tenant into the next request, and *every* RLS table including `skills` leaks; (b) `boma.is_system()`
is true whenever `boma.system_reason` is non-empty, so a stale/unset-clear system GUC grants
cross-tenant skill reads and writes; (c) FK cascades ignore RLS — deleting a space removes its skills
even for a caller that cannot select them (intended, but surprising in audits).

**F3 — constraints.** No `CHECK` couples `origin` to `plugin_source`: a `plugin` row with NULL
source breaks the omission report's "name it" promise and makes plugin-uninstall cleanup (which
deletes by source) leave orphans. `description`/`instructions` may be blank or unbounded. `updated_at`
has a default but no trigger: only the one Rust upsert maintains it; admin SQL or a future writer
leaves a lie that poisons sync/invalidation.

**F4 — atomicity.** Plugin install calls `remove_plugin_skills` then N `teach_skill` calls, each in
its own `with_tenant` transaction. A crash mid-install leaves a partially replaced skill set with no
generation marker to detect it.

**F5 — migration safety.** DDL is transactional today; if that changes, `ENABLE`/`FORCE RLS` followed
by a failed `CREATE POLICY` would lock the table wide open-to-nobody (no permissive policy = deny all).

## Hardening proposals (additive)

**H1 — mirror the domain grammar as CHECKs.** Add to `name`, `description`, `instructions`:
`name ~ '^[a-z0-9]+(-[a-z0-9]+)*$' AND length(name) <= 64`,
`btrim(description) <> '' AND length(description) <= 200`,
`btrim(instructions) <> '' AND octet_length(instructions) <= 32768`.
Risk: a deployment that already applied 0028 with legacy rows would fail the constraint; ship as a
new `ALTER TABLE ... ADD CONSTRAINT ... NOT VALID` + `VALIDATE` migration there. Built-in names are
ASCII, so char/byte counts agree. Confirms domain rules at the last line of defense.

**H2 — enforce the provenance invariant.** Add
`CHECK ((origin = 'taught' AND plugin_source IS NULL) OR (origin = 'plugin' AND plugin_source IS NOT NULL AND btrim(plugin_source) <> ''))`.
Risk: any historic plugin row with NULL source is rejected on validation; the current install path
always supplies it (CONFIRMED), so GUESS only for pre-0028 imports.

**H3 — maintain `updated_at` in the database.** Add a `BEFORE UPDATE` trigger setting
`NEW.updated_at = now()` (shared `boma.touch_updated_at()` helper, no other table touched).
Risk: the existing upsert also sets it — harmless double write; verify no caller depends on
preserving an externally supplied `updated_at` on UPDATE (none found here).

**H4 — make name collisions observable.** Keep `ON CONFLICT ... DO UPDATE` but add an explicit
`CREATE INDEX skills_plugin_source_idx ON skills (plugin_source) WHERE origin = 'plugin'`
so uninstall/omission lookups (and orphan detection) stay indexed as skill count grows.
Risk: tiny write overhead; no behavioral change. Add `COMMENT ON COLUMN skills.plugin_source`
documenting the ownership contract.

**H5 — bound the RLS blast radius.** Add a regression test that opens a pooled connection, runs
one member transaction, returns it to the pool, then queries `skills` with no GUC set and asserts
zero rows. Add a CI grep (extend ADR-002's `boma.space_id` check) forbidding session-level
`SET boma.space_id` / `SET boma.system_reason` outside `tenant.rs`.
Risk: test-only; catches the pool-leak regression before it becomes a cross-tenant incident.

**H6 — stage plugin installs.** Add a nullable `install_batch text` column (additive) written with
the same id for one install, so a crashed install can be detected and swept by a maintenance query.
Risk: extra column and sweep job; do not add a strict FK to `plugin_installs(source)` — that table is
migration 0035, created later, and plugin skills may exist without it in some flows.

## Verification checklist

- `cargo test -p boma-infra --test database` with a REQ-035 test asserting a 23514 rejection for
  `Upper`, `a--b`, blank instructions, and a `plugin` row with NULL source.
- Two spaces, same skill name: each sees only its own row; system reason set/cleared between calls.
- Upsert twice: one row, `created_at` stable, `updated_at` advanced by H3.
- `boma migrate` from empty DB and from a DB already at 0028 (H1/H2 as late validation migrations).

## Non-goals / residual risk

No changes to `spaces`, no destructive `DROP`, no reordering of migrations, no built-in-name CHECK
(code-coupled and would need an ADR). H1/H2/H3 change validation strictness for hypothetical
non-conforming writers — externally visible only if such rows exist, so confirm on real data first.
