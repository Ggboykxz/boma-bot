# v0.4.0 architecture proposal — `migrations/0020_team_computers.sql`

Worker #453 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no source changed).

Claim status: "current state" below is CONFIRMED by direct reads of `migrations/0020_team_computers.sql`,
`0017_computers.sql`, `0024_memory_shared_bot_id_nullable.sql`, `crates/infra/src/db.rs`,
`crates/infra/src/stores/computer.rs`, `crates/app/src/usecases.rs`,
`crates/testkit/src/build.rs`, `spec/01-requirements.md:255-259`, `docs/TRACEABILITY.md:51`,
`docs/STATE.md:1229-1234`. sqlx migration mechanics are CONFIRMED via current sqlx docs
(`-- no-transaction`, SHA-384 checksum, `VersionMismatch`). Forward-looking items are proposals
(GUESS until implemented and verified). English per repo disk convention. Additive only: no edit of
an applied migration, no data deletion, no removed interface.

## Current structure (CONFIRMED)

- Nine lines, two statements: a full-table `UPDATE computers SET private = true WHERE NOT private`
  (:7) and `CREATE UNIQUE INDEX computers_team_per_space ON computers (space_id) WHERE NOT private`
  (:9). No transaction wrapper, no RLS work (0017:79-94 already FORCE-enables RLS on `computers`).
- **Comment/schema mismatch:** :4-5 says `private` "was the default before this migration", but
  0017:23 declares `private boolean NOT NULL DEFAULT false`. The backfill makes existing rows
  explicitly private; the *default* still says team. Today `create`/`update` always bind `private`
  explicitly (`computer.rs:119,185`), so no live path omits it — but any other writer, seed or
  fixture would silently create a team computer, and the second one would hit the partial index.
- The index predicate mirrors `find_team` exactly (`computer.rs:61-63`), which is what makes that
  method's `fetch_optional` safe. `IF NOT EXISTS` is absent (contrast 0024:19); so is
  `CONCURRENTLY`; the backfill is unbatched.
- **Checksum-pinned:** `db.rs:20` embeds migrations with `sqlx::migrate!` and `db.rs:72` runs
  `MIGRATOR.run()`. sqlx stores a SHA-384 checksum of each applied file and aborts with
  `VersionMismatch` on any edit. v0.4.0 changes must therefore be *new* files, not edits to 0020.
- **Structural source of a REQ-031 aliasing gap:** a team row keeps the `bot_id` of the first bot
  that requested it (`usecases.rs:5544`) because 0017:32 makes `(space_id, bot_id)` unique and
  `bot_id NOT NULL`. `find` (`computer.rs:40`) filters by `bot_id` only, with no `private` guard,
  so `ensure_computer(private = true)` for that first bot (`usecases.rs:5529-5533`) can reuse the
  team row. The REQ-031 test creates private computers first (`testkit/src/build.rs:3414-3440`), so
  it never exercises team-then-private for the same bot.

## Guiding principle for v0.4.0

Freeze 0020 as a historical artifact. Every improvement ships as an additive follow-up migration
(new objects, defaults or backfills only), keeps the partial index as the single source of the
"one team computer per space" rule, and preserves the public `computer.start {mode}` surface.
Anything that changes an externally visible outcome is a DEVIATION candidate for `docs/PARITY.md`
before an ADR/owner decision, never a silent fix.

## Upgrades (5 concrete, additive)

1. **Correct the stale `DEFAULT false` in a new migration** `0060_computers_private_default.sql`:
   `ALTER TABLE computers ALTER COLUMN private SET DEFAULT true;` (or `DROP DEFAULT` to force an
   explicit choice). This aligns the default with the backfill's meaning and with `computer.start`
   defaulting to private, and removes the "third team row fails on the index" trap. *Risk:* it
   changes the result of inserts that omit `private` — a DEVIATION candidate if any writer relies
   on the old default. *Mitigation:* all current writers are CONFIRMED to bind the column; add a
   fresh-DB test inserting without `private` and asserting `private = true`; prefer `SET DEFAULT
   true` over `DROP DEFAULT` (no new failure mode for existing callers). No row is rewritten.

2. **Name and document the invariant, and make concurrent first-team starts safe.** Add
   `COMMENT ON INDEX computers_team_per_space IS 'at most one non-private (team) computer per
   space; see REQ-031'` in a follow-up migration, and have `create` use the existing partial index
   as an `ON CONFLICT (space_id) WHERE NOT private DO NOTHING RETURNING` arbiter, then re-select on
   conflict (coordination with worker #335's store scope, not a duplicate edit here). Two current
   `mode = team` starts both see no row and the loser becomes a 503 via `tenant::unavailable`.
   *Risk:* partial-index inference + RLS interplay must be proven on real Postgres, and returning
   success instead of 503 is externally visible. *Mitigation:* prove the arbiter with a two-
   concurrent-start integration test named for REQ-031; record the outcome in `docs/PARITY.md`.

3. **Adopt a deploy-safe shape for any future backfill/index migration in this family.** Keep 0020
   untouched; propose a repo pattern (ADR + runbook) of `-- no-transaction` plus
   `CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS` in its own file, batched DML loops instead of
   one full-table `UPDATE`, and a post-migration check of `pg_index.indisvalid`. *Risk:* a failed
   concurrent build leaves an INVALID index that sqlx will still record as applied, and a
   no-transaction file cannot share a transaction with its DML. *Mitigation:* separate files per
   statement, verify `indisvalid` after every run, and rehearse on a copy of a production-sized
   `computers` table before adopting. This is additive guidance; it does not retroactively change
   0020 on deployed databases.

4. **Close the team/private aliasing with additive objects.** Create
   `space_team_computers (space_id text PRIMARY KEY REFERENCES spaces, computer_id text UNIQUE
   REFERENCES computers)`, backfilled from `WHERE NOT private`, plus
   `CREATE OR REPLACE VIEW computers_with_scope AS SELECT c.*, CASE WHEN t.space_id IS NULL THEN
   c.bot_id END AS owner_bot_id ...`; `find` can then require `private AND owner_bot_id = $1`
   while `find_team` reads the view, without dropping `bot_id NOT NULL` or rewriting 0017. *Risk:*
   two sources of truth (`private` flag vs mapping table) can diverge, and denying a team-owner bot
   its "private" reuse is a wire-visible DEVIATION. *Mitigation:* keep a trigger on `computers`
   maintaining the mapping (an additive object), assert flag/table agreement in the same REQ-031
   test, and confirm the oracle before changing `find`; an ADR is required for the semantic shift.

5. **Add DB-level REQ-031 regression coverage for the invariant.** In `crates/infra/tests/`, assert
   on real Postgres: legacy rows are private after backfill; two `NOT private` rows in one space
   are rejected while two private rows are allowed; team-then-private for the owning bot behaves as
   decided in item 4; and the new default is private. Name the test with `REQ-031`. *Risk:* the
   in-memory store (`testkit/src/memory.rs`) has no index/default enforcement, so a shared
   conformance test could pass in memory and fail on Postgres. *Mitigation:* keep these assertions
   in the Postgres suite and document the intentional divergence; never weaken a test to make it
   green.

## Cross-cutting risks / sequencing

- Sequence 1 → 2 → 5 → 4 → 3. Item 1 is mechanical and lowest risk; 2 needs a real-Postgres
  arbiter proof; 5 locks in behavior before 4 changes it; 4 is the only semantic change and needs
  an ADR + owner approval; 3 is guidance for the next migration, not a code change.
- Never edit 0020 in place: sqlx checksum mismatch breaks every deployed database on startup.
  Keep the partial index predicate and `find_team`'s `WHERE space_id = $1 AND NOT private` in sync.
- All new objects stay inside the existing RLS perimeter; do not bypass `with_tenant`/`as_system`
  from the store. No new dependency is needed for items 1-5.
- Verification gate per step: `cargo fmt --check`, `clippy -D warnings`, integration + golden +
  OpenAPI contract suites, and `sqlx migrate info` against a migrated copy of a real schema.

## Evidence

CONFIRMED — read this session: `migrations/0020_team_computers.sql:1-9`; `0017_computers.sql:13-36,
79-94`; `0024_memory_shared_bot_id_nullable.sql:1-21`; `0051_lowercase_users_email.sql:1-7`;
`crates/infra/src/db.rs:1-95`; `crates/infra/src/stores/computer.rs:31-200`;
`crates/app/src/usecases.rs:5513-5576,5824-5840`; `crates/testkit/src/build.rs:3410-3441`;
`spec/01-requirements.md:255-259`; `docs/STATE.md:1227-1238`; `docs/TRACEABILITY.md:51`;
current sqlx migration docs via Context7 (checksum, `-- no-transaction`, `CONCURRENTLY`).
GUESS — that a production-sized `computers` table makes the original unbatched UPDATE/index build
a real deploy problem; that any writer outside the store omits `private`; that the team-owner
aliasing is observable through the RPC. Verify each before acting on the corresponding item.
