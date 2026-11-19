# Wave 22 - worker #530 - `migrations/0059_acp_agents.sql` (reliability lens)

Target: v0.4.0. Proposal only; **additive changes only** (new objects, non-destructive
constraints, new tests). No drop or rewrite of existing objects, no change to row semantics.
Per AGENTS.md: CONFIRMED = read this session, GUESS = inferred.

## 1. Baseline (CONFIRMED)
- `bot_acp_agents` holds one row per ACP-backed bot: `bot_id` PK/FK to `bots(id) ON DELETE
  CASCADE`, `space_id` FK to `spaces(id) ON DELETE CASCADE`, free-text `executor`,
  `created_at`/`updated_at` both `DEFAULT now()`, CHECK that `executor` is non-blank.
- Single index `bot_acp_agents_space (space_id)`. RLS enabled **and forced**, one `FOR ALL`
  policy using `space_id = boma.current_space_id() OR boma.is_system()` for both USING and
  WITH CHECK. This matches the project-wide pattern (0030, 0040, 0054, 0055, 0057, ...).
- `boma.current_space_id()` (0002_m1.sql:27) returns **NULL** when `boma.space_id` is unset;
  `boma.is_system()` (0002_m1.sql:38) is false when `boma.system_reason` is unset/empty.
- No trigger maintains `updated_at` anywhere in `migrations/` (CONFIRMED by grep); the column
  only changes if the application writes it explicitly.
- REQ-061 (01-requirements.md:620) requires a missing executor to fail the run **legibly**,
  and a crashed executor to requeue/fail - never vanish silently. `bots.detectedExecutors`
  (03-api-contract.md:59) reports executors found on PATH; the stored id is from a compiled-in
  registry, with no DB-side register.

## 2. Failure modes

### 2.1 Stale agents
- **F1 - Unvalidated executor id.** `executor` is any non-blank string; a typo or an id removed
  from the compiled-in registry is accepted and persisted, then fails at claim time (legible
  per REQ-061, but invisible until a user is affected). GUESS: no read-path validation exists.
- **F2 - No staleness signal.** No health column or view joins stored rows to the detected set;
  operators cannot distinguish "installed" from "configured but gone", so dead agents
  accumulate (CONFIRMED: schema has no such column).
- **F3 - Mutable `updated_at`.** Reconnect/executor-change may leave `updated_at` at the
  insertion time unless every writer remembers to set it, making change detection and audits
  unreliable (CONFIRMED: no DB trigger).
- **F4 - Low-severity asymmetry.** `ON DELETE CASCADE` from `bots` and `spaces` is correct, but
  there is no `ON UPDATE CASCADE`; a future id rewrite would be rejected. Ids are expected
  immutable (GUESS).

### 2.2 RLS
- **F5 - Silent full invisibility.** With both GUCs unset (background worker, cron, migration,
  CLI maintenance), `current_space_id()` is NULL and `is_system()` is false: `space_id = NULL`
  is NULL, so the policy denies every row. `SELECT` returns empty and `UPDATE` affects zero
  rows instead of erroring. A reconciliation job for stale agents can therefore "succeed" while
  doing nothing (CONFIRMED from the function definitions and policy text).
- **F6 - FORCE RLS affects the owner too.** The table owner cannot inspect or repair rows
  without setting `boma.system_reason`; an incident responder who forgets it sees an empty
  table. Granting `BYPASSRLS` as a shortcut would break the tenant model and is out of scope
  for reliability hardening (CONFIRMED: FORCE RLS is set).
- **F7 - Denormed `space_id`.** The row carries its own `space_id`, independent of
  `bots.space_id`. If they diverge, the row is hidden from the bot's own space and the FK still
  passes, producing an unreachable-but-valid agent (GUESS: depends on writer discipline).
- **F8 - Policy symmetry is correct (CONFIRMED).** USING == WITH CHECK prevents unreadable
  writes; keep this invariant in tests, no change needed.

### 2.3 Dedup
- **F9 - PK handles "one row per bot", retries do not.** `bot_id` PK makes duplicate rows
  impossible, but a repeated `bots.connectAcp` with plain `INSERT` raises a unique violation.
  The write path must be an explicit upsert; otherwise client retries surface as 500s
  (GUESS: contract does not state idempotency).
- **F10 - Cross-table ambiguity.** `bot_external_agents` (0054) has the same PK shape. Nothing
  prevents one `bot_id` from existing in both tables, which would make dispatch pick an
  arbitrary executor (CONFIRMED: no cross-table constraint in either migration).
- **F11 - No supporting index for reconciliation.** Queries by `(space_id, executor)` must scan
  the `space_id` index then filter; with many agents this slows the periodic stale scan
  (GUESS: depends on row count).

## 3. Additive hardening for v0.4.0

1. **Maintain `updated_at` in the DB** (new migration 0060): create
   `boma.touch_updated_at()` trigger function (`NEW.updated_at := now()`) and a
   `BEFORE UPDATE ON bot_acp_agents` trigger. Additive, no existing rows touched. Risk: an
   application writer that sets `updated_at` explicitly gets overwritten - audit writers
   first, or restrict the trigger to `WHEN (NEW.updated_at = OLD.updated_at)`.
2. **Executor registry as a soft guard**: add `boma.acp_executor_registry (executor text PK,
   known_since timestamptz)` seeded from the compiled-in list, expose
   `boma.is_known_acp_executor(text)`, and add a CHECK calling it. Add the CHECK as `NOT VALID`
   first, backfill, then `VALIDATE` so existing rows cannot break deploy. Risk: registry/compiled
   list drift across releases - require the seed to be generated from the same source as the
   registry and add a CI check that the two match. If drift is unacceptable, ship only the
   validation function plus the view in item 3 and keep the executor column free.
3. **Staleness view**: `boma.bot_acp_agents_health` joining stored rows against the registry and
   (optionally) a `detected` flag refreshed by `bots.detectedExecutors`. Read-only, surfaces
   stale agents without deleting or disabling them (REQ-061 wants legible failure, not silent
   removal). Risk: a stale flag is only as fresh as the last detection refresh - document it.
4. **Tenant consistency trigger** for F7: `BEFORE INSERT OR UPDATE`, verify
   `NEW.space_id = (SELECT space_id FROM bots WHERE id = NEW.bot_id)` and raise a legible error
   on mismatch. Avoids a composite FK that would need a new unique index on `bots`. Risk: if
   a bot ever legitimately moves space, the row must be migrated in the same transaction.
5. **RLS context guard**: add `boma.assert_tenant_context()` that raises when both GUCs are
   unset, and call it at the start of the reconciliation/reporting paths (operationally, not
   in the policy). Keep the existing policy text unchanged. Add a runbook note: maintenance
   must set `boma.system_reason`, never use `BYPASSRLS`. Risk: call sites must be found; missed
   ones still fail silently - cover with tests.
6. **Dedup contract**: mandate `INSERT ... ON CONFLICT (bot_id) DO UPDATE SET executor =
   EXCLUDED.executor, updated_at = now()` in the connect handler; add a unique index
   `(space_id, executor)` only if needed - note it would forbid two bots using the same
   executor in one space, which may be legitimate, so **do not add it** without product input.
   Add a non-blocking cross-table guard for F10 using a trigger guarded by
   `to_regclass('bot_external_agents') IS NOT NULL`, so 0060 does not hard-depend on 0054.
7. **Tests (REQ-061 named)**: `req_061_acp_agent_is_single_per_bot`,
   `req_061_acp_reconnect_is_idempotent`, `req_061_acp_rls_hides_rows_without_tenant_context`,
   `req_061_acp_rls_system_path_sees_all_spaces`,
   `req_061_acp_updated_at_advances_on_change`,
   `req_061_acp_stale_executor_is_flagged_not_deleted`,
   `req_061_acp_space_mismatch_is_rejected`.

## 4. Risk summary

| Item | Risk | Mitigation |
| --- | --- | --- |
| 1 trigger | Overwrites explicit timestamps | `WHEN` clause / writer audit |
| 2 registry | Compiled-list drift | Generate seed from single source, CI parity check |
| 3 view | Stale detection data | Label freshness, never gate dispatch on it |
| 4 trigger | Bot space moves | Update both tables in one transaction |
| 5 guard | Missed call sites | Tests + runbook |
| 6 upsert | Ambiguity on shared executors | Skip unique index pending decision |

No existing row is deleted, renamed or retyped by this proposal; every item is a new object or
a widening/validation guard that can be shipped behind the v0.4.0 migration flag.
