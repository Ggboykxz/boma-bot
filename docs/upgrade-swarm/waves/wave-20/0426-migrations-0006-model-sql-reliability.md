# 0426 — migrations/0006_model.sql — reliability (v0.4.0)

Worker #426 · module `migrations/0006_model.sql` · lens: reliability · READ-ONLY review of
`rebuild-workspace/.worktrees/before-m5` at the v0.3.0-era tree. Proposals only; no code changed
in this pass. Language: English. **Additive only** — every proposal is a new statement or a new
migration file; none edits an existing migration in place.

## Ground truth (CONFIRMED by reading the tree this session)

- 0006 is six `ALTER TABLE ... ADD COLUMN`s: `bots` and `runs` each gain nullable `model_provider
  text`, nullable `model_id text`, and `model_pinned boolean NOT NULL DEFAULT false`. No CHECK,
  COMMENT, index, backfill or policy.
- `bots`, `runs` are tenant tables under `ENABLE` + `FORCE ROW LEVEL SECURITY` with policy
  `space_id = boma.current_space_id() OR boma.is_system()` (0002, DO block). RLS is table-wide, so
  the new columns are already covered; `ALTER TABLE` is DDL and not subject to RLS, but any future
  backfill (DML) would be.
- `boma.current_space_id()` / `boma.is_system()` exist from 0002, before 0006.
- `crates/domain/src/model.rs`: a choice is `provider/model`; `split()` defaults provider to
  `custom` when no slash, and resolve() always returns a `Some(provider)`.
- `Run::model()` (run.rs) returns `None` when **either** provider or id is `NULL`, regardless of
  `model_pinned`. `usecases.rs` (`model_choice_for`) writes `Some(...)` for both on every new run.
- `model_pinned` means "never substitute silently" (REQ-015). 0043 adds `model_credentials`
  (RLS, per space) and `user_model_preferences`; a pinned unreachable model must fail visibly.
- `db.rs` embeds `sqlx::migrate!`; `_sqlx_migrations` stores checksum, migrations run in one
  transaction, `boma migrate` is a deploy step. Editing 0006 in place breaks every existing DB.

## What it already does right (keep, do not re-propose)

- `NOT NULL DEFAULT false` on the flag is the correct fast path: a constant default is a
  metadata-only add on PostgreSQL 11+, no table rewrite and no long lock.
- Nullable provider/id correctly model "no choice yet" (fall through to space/person/deployment).
- No index on the new columns: readers filter by space/PK, not by model; an index is waste.
- No FK to `model_credentials`: credentials are per space and sealed; provider is a catalogue key,
  not a credential row. Keep it that way.

## Failure modes → concrete additive hardening

### F1. Cross-column invariants are unenforced (constraints)

Nothing stops `model_pinned = true` with `model_id IS NULL` (or a NULL provider on `runs`). Such a
row makes `Run::model()` return `None`, so the "pinned, never substitute" guarantee (REQ-015) is
silently lost and the activity-list label is blank. Nothing bounds string length either: a stray
multi-megabyte model key is stored and echoed into logs/UI.

P1. In a **new** `0060_model_columns_hardening.sql`, add to each table (identical predicates!):
   `ADD CONSTRAINT <t>_model_pinned_has_id CHECK (NOT model_pinned OR model_id IS NOT NULL)`
   and `ADD CONSTRAINT <t>_model_provider_needs_id CHECK (model_provider IS NULL OR model_id IS NOT NULL)`.
P2. Add `CHECK (model_id IS NULL OR length(model_id) BETWEEN 1 AND 256)` and the same for
   `model_provider`, with `NOT VALID` first, then `VALIDATE CONSTRAINT` in a later statement —
   validation takes only `SHARE UPDATE EXCLUSIVE`, so concurrent writers are not blocked.
P3. Add `COMMENT ON COLUMN` for all six columns stating the NULL semantics ("NULL = not yet
   resolved"), mirroring 0003's precedent. This is the reliability contract for on-call.
P4. **Do not** require a non-NULL provider when pinned: `model_choice_for` defaults it to `custom`,
   so that stricter CHECK would reject states the app can legitimately write today.

### F2. RLS is correct today but fragile under future writes (RLS)

The columns inherit the existing policy, so tenant isolation is intact. The risk is a later
backfill or data-fix script: under `FORCE RLS` an `UPDATE bots SET model_provider = ...` run without
`boma.system_reason` updates **zero rows and reports success**, leaving half-migrated data.

P5. Add a regression test (name carrying REQ-043) that a tenant connection cannot read or set
   another tenant's `model_*` columns, and that a `boma.system_reason`-scoped update does. Prove
   the new columns are covered rather than assuming it.
P6. Document in the migration/runbook: any backfill of these columns must run under the system
   path (`tenant::as_system`), because the owner is also forced through RLS.
P7. Keep `boma.is_system()` as the only escape in any new policy; never inline `current_setting`.

### F3. Defaults and lifecycle drift (defaults)

`model_pinned DEFAULT false` is a persistent column default — good for inserts. But nothing stops
an `UPDATE` from clearing `model_id`/`model_provider` on a **completed** run, destroying the
"records the choice it made" audit trail promised by the file's own header.

P8. Add `CHECK (status IS NULL OR status <> 'completed' OR model_id IS NOT NULL)` on `runs`
   (`NOT VALID` + later `VALIDATE`), GUESS pending a grep of the exact stored status literals
   (`RunStatus::Completed` in domain/run.rs); if the literal differs, use that one. Skip if any
   legitimate completed-run path writes no model (grep `INSERT INTO runs` first).
P9. Keep the DEFAULT on the flag; do not add a DEFAULT to provider/id — a fake `''` default would
   silently turn "unresolved" into a bogus choice and defeat fallback resolution.

### F4. Pipeline integrity and lock cost (migration process)

Editing 0006 in place changes its SQLx checksum and `MIGRATOR.run` refuses to start — deploy-wide
outage. `ADD CONSTRAINT` without `NOT VALID` takes `ACCESS EXCLUSIVE` and scans a large `runs`
table; a rushed release can stall all writes.

P10. Land every proposal as new migrations (`0060`, `0061`), never as edits to 0006; mirror the
   additive-only gate already used for other applied migrations.
P11. Record the downgrade story required by spec 04 §4.9: `DROP CONSTRAINT` / `DROP COMMENT` for
   `0060`–`0061`, so rollback does not require hand-written SQL under pressure.
P12. Verify `NOT VALID` → `VALIDATE` split on a copy of a populated `runs` table before release;
   the add and the validate can be in the same migration only if the lock window is acceptable.

## Risks and trade-offs

- R1. P1/P8 constrain existing rows: an unexpected state fails `VALIDATE` and blocks the deploy.
  Pre-flight with `SELECT ... WHERE NOT (<predicate>)` on staging; fix data in a separate additive
  migration rather than weakening the check.
- R2. P8 is the only proposal whose stored status literal is GUESS; wrong spelling makes the
  check vacuous. Confirm against real rows first, otherwise ship P1–P7 alone.
- R3. A CHECK cannot reference `model_credentials` (RLS + per-space) and should not: cross-table
  subqueries in CHECKs evaluate under caller RLS and can fail unpredictably. Catalogue validity
  stays an application concern (REQ-015).
- R4. `NOT VALID` leaves the constraint unenforced for old rows until `VALIDATE`; that is
  deliberate and documented, not a hole — new writes are checked immediately.
- R5. No gate, lockfile or observable API behaviour changes; all proposals are independently
  revertable and additive.

## Suggested rollout for v0.4.0

Wave A (this doc): P1–P4, P5–P7, P9–P12. Wave B: P8 once the status literal and completed-run
paths are confirmed. Run `cargo test -p boma-infra` (REQ-043 + REQ-015 coverage) after each wave;
mark claims CONFIRMED only after that output exists.

_Scope note: line/section references are against `before-m5` as read this session. No CI, DB or
test command was executed in this READ-ONLY pass. Exact status literals and pre-existing bad rows
remain GUESS pending the greps/SELECTs noted above._
