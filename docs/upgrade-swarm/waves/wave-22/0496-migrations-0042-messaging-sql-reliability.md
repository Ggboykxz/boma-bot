# 0496 — migrations/0042_messaging.sql — reliability (v0.4.0)

Worker #496 · module `migrations/0042_messaging.sql` · lens: reliability · READ-ONLY review of
`rebuild-workspace/.worktrees/before-m5` at the v0.3.0-era tree. Proposals only; no code changed in
this pass. Language: English. **Additive only** — every proposal is a new statement, a new index or
a new migration file; none edits 0042 or any applied migration in place.

## Ground truth (CONFIRMED by reading the tree this session)

- 0042 adds four columns to `threads`: `kind text NOT NULL DEFAULT 'bot' CHECK (kind IN
  ('bot','external'))` plus nullable `external_platform`, `external_workspace_id`,
  `external_conversation_id` (no CHECK, length bound or COMMENT). Partial unique index
  `threads_external_identity (external_platform, external_workspace_id, external_conversation_id)
  WHERE kind = 'external'`; NULL identity columns are therefore *not* uniqueness-guarded.
- `messaging_installations`: PK `id`; FKs to spaces/bots/users `ON DELETE CASCADE`; sealed
  `token_ciphertext/nonce bytea NOT NULL`, nullable `webhook_ciphertext/nonce`, `reply_url`,
  `allow_private DEFAULT false`, `status CHECK IN ('active','removed') DEFAULT 'active'`.
  Unique index `(platform, workspace_id)` has **no** `WHERE status = 'active'` predicate and no
  `space_id`. RLS `ENABLE` + `FORCE`, policy `space_id = boma.current_space_id() OR boma.is_system()`.
- `messaging_install_states`: `state` PK, three CASCADE FKs, `expires_at NOT NULL`, same RLS policy.
  No index on `expires_at`, no cleanup of consumed/expired rows.
- Helpers `boma.current_space_id()` / `boma.is_system()` predate 0042 (0002).
- `crates/infra/src/db.rs`: `sqlx::migrate!("../../migrations")`, `MIGRATOR.run` is one forward-only
  transaction; editing an applied file changes its checksum and refuses to start (deploy-wide outage).
- `crates/infra/src/stores/messaging.rs`: install uses `ON CONFLICT (platform, workspace_id) DO
  NOTHING` then returns `find()` (not `find_active()`); `find_active_by_platform_workspace` filters
  `status='active'`; `mark_removed` is `UPDATE ... WHERE status='active'`; `take_install_state` is an
  atomic `DELETE ... WHERE state=$1 AND expires_at > now() RETURNING`; `resolve_external_thread`
  does select-then-`INSERT ... ON CONFLICT DO NOTHING`-then-reselect under `tenant::with_tenant`.
- `crates/infra/tests/m1.rs` (REQ-048) already pins: second delivery reuses the same external thread,
  a stale signature is refused, and a removed installation refuses inbound delivery.

## What it already does right (keep, do not re-propose)

- Constant `DEFAULT 'bot'` and NOT NULL FKs with CASCADE: metadata-only add on PG 11+, no rewrite.
- Single-use handshake via `DELETE ... RETURNING` is race-safe; atomic, no "consumed" flag needed.
- `token_* NOT NULL` pair and `FORCE RLS` on both new tables are correct; `is_system()` is the sole
  escape and should stay that way.

## Failure modes → concrete additive hardening

### F1. State and ordering (install lifecycle)

A `removed` installation still owns `(platform, workspace_id)` because the unique index is not
partial on `status`. `install()`'s `ON CONFLICT DO NOTHING` then `find()` returns the *removed* row as
if the install succeeded — onboarding cannot recover a workspace after removal. The full index also
cannot be replaced additively (no DROP allowed here). Additive: (P1) add a regression test named for
REQ-048 that pins current reinstall behaviour, so the intended semantics become explicit rather
than accidental; (P2) document the defect in `docs/STATE.md` and route the index fix to an owner-
approved ADR + non-additive major migration (a co-existing partial `WHERE status='active'` index is
harmless but does **not** change `ON CONFLICT` inference — do not ship it as a fix). Expired install
states are never reclaimed; additive: (P3) `CREATE INDEX ... ON messaging_install_states
(expires_at)` in a new migration and a scheduled cleanup that runs under `tenant::as_system`, since
owner DML is also `FORCE`d through RLS and would silently delete zero rows.

### F2. RLS visibility at ingress

All inbound lookups go through `tenant::with_tenant(context, ...)`. If the Slack/generic callback or
webhook ever resolves with a *member* context of a different space, the active installation is
invisible under RLS and the message is rejected as if the install were removed (fail-closed, but
misdiagnosed). Likewise a foreign-tenant member must not be able to `take_install_state` of another
space's handshake. Additive: (P4) a REQ-048 test that inbound routing succeeds with a system context
and that a member of space B cannot read, reveal or consume space A's installation/state; (P5) a
runbook note that any backfill or cleanup of these tables must use the system path, never a bare
`UPDATE`/`DELETE`.

### F3. Constraints and cross-column integrity

Nothing enforces the identity invariants the partial index relies on. A direct write can store
`kind='external'` with a NULL identity column (escaping the unique index entirely → duplicate thread
identity), or `kind='bot'` with identity columns set. The sealed webhook pair can be half-written
(ciphertext set, nonce NULL) and fail only later at reveal time. Additive, in a new `006x` migration:
(P6) `CHECK ((kind='bot' AND external_* IS NULL) OR (kind='external' AND external_* IS NOT NULL))`
on `threads`, `NOT VALID` then `VALIDATE` (SHARE UPDATE EXCLUSIVE, writers not blocked); (P7)
`CHECK ((webhook_ciphertext IS NULL) = (webhook_nonce IS NULL))` on installations; (P8) bounded
lengths on `workspace_id`, `workspace_name`, `external_*`, `reply_url` (`NOT VALID` + `VALIDATE`,
limits GUESS pending a grep of the wire parsers). Do **not** add a DB CHECK tying `reply_url`/
`allow_private` to `platform`: the private-host opt-in is an app/audit concern (D-05, REQ-033), and
a strict CHECK risks rejecting legitimate rows.

### F4. Migration process and lock cost

`CREATE UNIQUE INDEX` on `threads` takes a SHARE lock; because `MIGRATOR.run` is one transaction,
`CREATE INDEX CONCURRENTLY` is impossible inside 0042 or any migration file. Additive: (P9) schedule
index/constraint work in a maintenance window and pre-flight populated-table size; (P10) record the
downgrade story required by spec 04 §4.9 (`DROP INDEX` / `DROP CONSTRAINT`) for every new `006x`
file so rollback is not hand-written under pressure; (P11) land everything in new files only.

## Risks and trade-offs

- R1. P6–P8 constrain existing rows: one bad row fails `VALIDATE` and blocks the deploy. Pre-flight
  with `SELECT ... WHERE NOT (<predicate>)` on staging; fix data in a separate additive migration
  rather than weakening the check.
- R2. External identity is deployment-global (no `space_id`) while RLS is per-space: a cross-space
  collision makes `ON CONFLICT DO NOTHING` + reselect return `Unavailable`. Tightening the key with
  `space_id` changes routing semantics (a DEVIATION) — ADR and owner sign-off required, not additive.
- R3. The reinstall fix (F1) needs a non-additive index replacement; shipping P2 without it leaves a
  known defect documented, not repaired. Honest trade-off for the additive-only gate.
- R4. `NOT VALID` leaves old rows unchecked until `VALIDATE`; deliberate and documented, new writes
  are checked immediately. P8 length limits are GUESS — confirm against real wire values first.
- R5. No gate, lockfile or observable API behaviour changes; all additive proposals are independently
  revertable.

## Suggested rollout for v0.4.0

Wave A (this doc): P1, P3–P11 plus the runbook notes. Wave B: P2 index replacement and R2 key change,
only after an ADR and owner approval. Run the REQ-048 infra test (`crates/infra/tests/m1.rs`) and
`cargo test -p boma-infra` after each wave; mark claims CONFIRMED only after that output exists.

_Scope note: references are against `before-m5` as read this session. No CI, DB or test command was
executed in this READ-ONLY pass. Reinstall intent, wire-field max lengths and ingress context remain
GUESS pending the greps/SELECTs noted above._
