# Worker #462 — `migrations/0024_memory_shared_bot_id_nullable.sql` — reliability lens — v0.4.0 proposal

READ-ONLY review; no code changed. All "current state" claims are CONFIRMED by direct reads of
`0024_memory_shared_bot_id_nullable.sql`, `0013_memory.sql`, `crates/infra/src/stores/memory.rs`,
`rows.rs:464-473`, `account_export.rs:137-152`, `tests/database.rs:38-56` and `docs/STATE.md:1065-1077`.
Forward-looking items are proposals (GUESS until implemented, tested and measured). Per scope this
proposal is **additive only**: new migration file(s) and new tests; `0013`/`0024` stay byte-exact,
because sqlx's `_sqlx_migrations` ledger records a checksum (the `VersionMismatch(25)` incident in
`STATE.md:1058-1063` forbids editing a recorded migration).

## What 0024 does today (CONFIRMED)

- Drops `NOT NULL` on `bot_id`, drops the old default `''`, re-sets default `NULL`, drops
  `memory_documents_check` with `IF EXISTS`, and adds `memory_documents_scope_bot_id_check`
  (`bot`→non-NULL non-empty `bot_id`; `shared`→`bot_id IS NULL`).
- Adds partial unique index `memory_documents_shared_key ON (space_id, path) WHERE scope='shared'
  AND bot_id IS NULL` as the arbiter for the shared UPSERT (`memory.rs:74`), since the 0013 UNIQUE
  `(space_id, scope, bot_id, path)` cannot discriminate NULLs.
- Moves the shared marker from `''` to `NULL`; the FK `REFERENCES bots (id)` then ignores shared
  rows and they survive bot deletion. Reads use `IS NOT DISTINCT FROM` (`memory.rs:129,159,189`).

## Failure modes

1. **Stale old CHECK silently survives (highest risk).** `0013:19` declares its CHECK *unnamed*;
   the auto-generated name is only *expected* to be `memory_documents_check`. `DROP CONSTRAINT IF
   EXISTS` (0024:10) cannot fail — if the real name differs, the old check
   (`shared`→`bot_id=''`) stays. Adding the new check then either fails (legacy rows) or leaves two
   contradictory checks, and *every shared write with NULL is rejected* — the original REQ-022
   defect returns while the migration still reports success. CONFIRMED mechanism; GUESS that the
   name matches in this build (needs verification on a freshly migrated DB).
2. **Legacy shared rows not converted.** The comment "existing rows are untouched" assumes no
   `scope='shared' AND bot_id=''` row can exist. On any DB seeded by hand, an older schema, a
   restored backup or a system-context insert that bypassed the FK, `ADD CONSTRAINT ... CHECK`
   aborts the whole migration. No backfill, no pre-flight count, no guard. CONFIRMED behavior.
3. **NULL blind spots in consumers.** `account_export.rs:143` selects `bot_id = ANY($1)`; SQL `=`
   never matches NULL, so shared documents are silently omitted from account exports. Before 0024
   shared rows could not exist, so this is a *new* exposure. Other `=`/`<>` uses of `bot_id` need a
   sweep; `memory.rs` itself is correct. CONFIRMED for account_export.
4. **DEFAULT flip.** Omitting `bot_id` now yields NULL (shared) instead of `''`; with the new CHECK
   a `scope='bot'` insert that forgets `bot_id` fails a check instead of the FK — still a failure,
   but a different error class, and any producer relying on the old default changes behavior.
   CONFIRMED from lines 8-11.
5. **RLS / arbiter interaction.** `0013:39-46` uses `FORCE ROW LEVEL SECURITY`; DDL is unaffected,
   but any later backfill DML needs a tenant/system context. The index predicate must match the
   `ON CONFLICT (space_id, path) WHERE scope='shared' AND bot_id IS NULL` target exactly or
   inference fails (it does today). RLS hides other spaces, but `space_id` is in the index key so
   cross-space conflicts cannot occur. GUESS: no test pins the predicate/arbiter pairing.
6. **Rolling deploy window.** Migration-applied + old app still binding `''` for shared → shared
   writes rejected (old code already failed via FK, so no regression, but the new CHECK is a
   second, different failure). New app + migration not yet applied → `bot_id` still NOT NULL, so
   shared writes fail. Deploy order must be migration-first. CONFIRMED logic.
7. **Rollback is not symmetric.** Reverting requires NULL→`''`, but the FK forbids `''` and shared
   rows would violate the old check; no rollback script exists. CONFIRMED.

## Concrete additive hardening for v0.4.0

- **A1 — verification migration, not an edit.** Add `0060_memory_0024_guard.sql`: a DO block that
  asserts (a) no CHECK on `memory_documents` still requires `bot_id=''` for shared, and (b) the
  partial index exists with the expected predicate (`pg_indexes`/`pg_constraint` catalog query),
  raising an exception on drift. Purely additive, re-runnable, no data change.
- **A2 — defensive backfill, guarded.** In the same new migration, `UPDATE memory_documents SET
  bot_id = NULL WHERE scope='shared' AND bot_id=''` before any new check is relied upon; log/assert
  the affected count. If a FK ever allowed such a row this repairs it; otherwise it is a no-op.
  Additive DML, idempotent.
- **A3 — write-path proof test.** Add `req_022_shared_memory_row_is_writable_in_postgres`: on a
  freshly migrated DB write a shared doc, assert `bot_id IS NULL`, revision increments, second write
  hits the partial index (one row, revision 2), bot-scoped doc coexists at the same path, and
  deleting the bot leaves the shared row. This would have caught the original defect.
- **A4 — constraint-name / ledger tests.** Extend `tests/database.rs` to assert the exact set of
  CHECK names after migration and that `_sqlx_migrations` checksums for 0013/0024 are unchanged
  (guards the "never edit a recorded migration" rule).
- **A5 — NULL blind-spot sweep.** Add `req_046_shared_memory_appears_in_account_export` with a
  shared+bot fixture; fix `account_export.rs` with `bot_id IS NOT DISTINCT FROM ANY($1)` or an
  explicit `OR bot_id IS NULL` branch — additive query change, no schema change.
- **A6 — documented deploy/rollback note.** Add an additive runbook under `docs/` stating
  migration-first rollout and a rollback script that deletes or quarantines shared rows (NULL) before
  restoring NOT NULL/old CHECK. Mark any externally visible behavior change in `docs/PARITY.md`.

## Risks and mitigations

- **Guard migration false-negative:** catalog predicates are version-sensitive. Mitigation: query
  `pg_get_constraintdef`/`pg_get_indexdef` and assert semantics, not a literal name; run against the
  real test DB and mark CONFIRMED only then.
- **Backfill hides a real corruption:** silently NULLing a bot row mislabeled `shared` loses
  ownership. Mitigation: assert the old row's FK validity first; emit a warning with the count and
  fail if > 0 in production, GUESS until the original's data is checked.
- **A5 query change is wire-visible:** adding shared docs to exports changes output. Mitigation:
  gate behind a DEVIATION in `docs/PARITY.md` and confirm against the original oracle before shipping.
- **Index/arbiter coupling:** any future edit to the partial index predicate must update the
  `ON CONFLICT` target in lockstep. Mitigation: A1 pins the definition and A3 exercises the path.
- **No new dependencies required**; keep all of A1-A6 additive and sequenced A1→A2→A3→A4→A5→A6,
  each gated by `cargo fmt --check`, `clippy -D warnings`, REQ-022/046 tests and `tools/trace_check.py`.
