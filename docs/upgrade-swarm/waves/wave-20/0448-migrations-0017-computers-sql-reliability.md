# 0448 — migrations/0017_computers.sql — reliability (v0.4.0)

Worker #448 · module `migrations/0017_computers.sql` · lens: reliability · READ-ONLY review of the
`before-m5` v0.3.0-era tree. Proposals only; no source changed. **Additive only**: every item below is
a new migration file (numbered 0062+, leaving 0060/0061 to sibling proposals) or a new test — nothing
edits 0017–0020 in place (SQLx checksum immutability, spec 04 §4.9). **CONFIRMED** = read from source
this session; **GUESS** = verify with a test/oracle before shipping.

## Ground truth (CONFIRMED)

- 0017 creates `computers` (status CHECK over six values, `screen_generation >= 0`,
  `memory_limit_mb 64..4096`, `cpu_limit 0.25..4`, `idle_minutes 1..1440`, `UNIQUE(space_id,bot_id)`),
  `computer_runs` (PK `(computer_id,run_id)`, partial unique index `one_open WHERE ended_at IS NULL`),
  `computer_execution_leases` (PK `run_id`, `owner`, `fence`, `expires_at`), `agent_homes` and
  `browser_profiles` (`UNIQUE(space_id,storage_key)`), all with `ON DELETE CASCADE` FKs.
- 0018 adds `screen_capabilities`; 0019 adds `takeover_by`/`takeover_at` to `computers` with no CHECK;
  0020 backfills `private = true` and adds `computers_team_per_space` (`WHERE NOT private`).
- 0002 installs `boma.current_space_id()` / `boma.is_system()`. The 0017 DO block applies `ENABLE` +
  `FORCE ROW LEVEL SECURITY` and one policy to all five tables: `USING (space_id =
  current_space_id() OR is_system())` and the same `WITH CHECK`.
- `runs` (0002:190) has PK `id` and `space_id` but **no** `UNIQUE(space_id,id)`; `computers` has
  `UNIQUE(space_id,bot_id)` but no `UNIQUE(space_id,id)`. No lease-expiry index exists anywhere.

## Failure modes

### A. Constraints

- **A1 (HIGH, CONFIRMED):** no CHECK couples `status` to the timestamp columns. `running` with
  `stopped_at` set, `stopped` with `started_at` but no `stopped_at`, or `queued` with `started_at` are
  all representable. `running_idle` (store layer) keys only on status + `last_used_at`, so such a row
  drives idle-release decisions with contradictory evidence.
- **A2 (MED, CONFIRMED):** no CHECK that `created_at <= updated_at`, nor that a provisioned status
  (`running`/`interrupted`) has `provider`, `external_id` and `home_id`. `provider` accepts any text
  including `''`; REQ-027's fail-closed `none` provider is enforced only in application code.
- **A3 (HIGH, CONFIRMED):** `computer_execution_leases` has no index on `expires_at` (or `(owner,
  expires_at)`). Every lease-recovery sweep — the mechanism that reclaims runs whose owner died
  (spec REQ-012/REQ-026) — scans the whole table; the cost grows with run history.
- **A4 (HIGH, CONFIRMED):** no composite FK ties `space_id` across `computers` → `computer_runs` →
  `computer_execution_leases`. Each table only has `space_id REFERENCES spaces(id)`, so a
  system-path writer can attach a lease or run to the wrong space's computer; RLS then hides the row
  from the rightful tenant and the sweep cannot reconcile it.
- **A5 (MED, CONFIRMED):** `UNIQUE(space_id,bot_id)` plus `bot_id NOT NULL` conflicts with the team
  computer of 0020 (one row per space). A team row must carry a sentinel `bot_id`, which reserves
  that bot's private slot and can collide with a real private computer (REQ-031).
- **A6 (LOW, GUESS):** `screen_generation integer` has no upper bound; a `+1` at `INT_MAX` overflows.
  Combined with the `unwrap_or(u64::MAX)` sentinel flagged in worker 0338, one bad read can invalidate
  every screen capability indefinitely (REQ-030).

### B. Takeover state

- **B1 (HIGH, CONFIRMED):** 0019's `takeover_by` / `takeover_at` have no CHECK that both are NULL or
  both set. A half-set pair is projected verbatim and drives `ScreenTaken` decisions with no time.
- **B2 (HIGH, CONFIRMED):** `takeover_by` has no FK to a member and no index. A removed member's id
  can linger forever; a forgotten takeover has **no expiry**, so the bot's input stays refused
  indefinitely (REQ-029) with no SQL-level recovery path.
- **B3 (MED, GUESS):** nothing prevents a takeover on a `stopped` computer, and no audit row records
  who took/released control. Verify against the app's takeover path before adding SQL constraints.

### C. RLS

- **C1 (HIGH, CONFIRMED):** `is_system()` is a blanket bypass on all five tables. Any request that
  sets `boma.system_reason` sees every tenant; a leaked system reason on one tenant request is a
  full cross-tenant read. Accept the escape but make its use on these tables observable.
- **C2 (HIGH, CONFIRMED):** `FORCE RLS` also binds the table owner. Future data-fix migrations that
  UPDATE these tables without `boma.system_reason` will affect **zero rows** and appear to succeed —
  a silent repair failure. (0017's own DDL is unaffected; DML later is not.)
- **C3 (MED, CONFIRMED):** `current_space_id()` casts `current_setting('boma.space_id', true)::text`
  with no validation. An empty or malformed setting simply matches no row (default-deny — correct),
  but a *stale* setting from a pooled transaction can expose the wrong tenant's rows if the app forgets
  to reset it.
- **C4 (MED, CONFIRMED):** the five policies are generated in one DO loop; adding a sixth tenant table
  without the same loop silently gets no RLS. No CI check asserts every tenant table carries a policy.

## Concrete hardening (additive, prioritized)

- **P0 — A3:** `0062_computers_lease_index.sql`: `CREATE INDEX computer_leases_expires_idx ON
  computer_execution_leases (expires_at);` plus `(owner, expires_at)` if sweep plans show it helps.
  *Risk:* minor write amplification; negligible at sweep cadence.
- **P0 — A4:** `0063` adds `UNIQUE (space_id, id)` on `runs` and `computers` (prerequisite), then
  composite FKs `(space_id, computer_id) → computers(space_id,id)` and `(space_id, run_id) →
  runs(space_id,id)` on `computer_runs`, and `(space_id, run_id) → runs(space_id,id)` on leases.
  *Risk:* fails on existing cross-space drift — run an audit query first; extra unique indexes cost writes.
- **P1 — A1/A2/B1):** `0064_computers_invariants.sql` adds CHECKs: status↔timestamps coherence,
  `created_at <= updated_at`, `provider <> ''`, and `(takeover_by IS NULL) = (takeover_at IS NULL)`.
  *Risk:* historic rows may violate — audit/backfill before enforcing, else the migration aborts.
- **P1 — B2:** in `0064`, FK `takeover_by → users(id) ON DELETE SET NULL` plus a BEFORE UPDATE/DELETE
  trigger that clears both takeover columns atomically, and an index on `takeover_by`. Add a
  `takeover_expires_at timestamptz` column so the app can expire forgotten holds.
  *Risk:* trigger + column need app changes; pair-clearing must be atomic with the FK action.
- **P1 — C1:** `0065_tenant_dml_audit.sql`: a trigger writing system-path DML on these five tables to
  a new system-only audit table, so blanket `is_system()` use is visible. Keep the policy unchanged.
  *Risk:* trigger overhead on hot lease writes; scope to UPDATE/DELETE if INSERT is too noisy.
- **P2 — A5:** resolve the private/team `bot_id` collision: either allow `bot_id NULL` for team rows
  (new migration + narrow CHECK) or forbid a private computer for the sentinel id. *Risk:* changes
  the `UNIQUE(space_id,bot_id)` contract; needs a spec decision, not a silent schema tweak.
- **P2 — C4/C2:** add a CI grep/test that every table with a `space_id` column has a tenant policy,
  and a regression test that a data-fix UPDATE without `boma.system_reason` affects zero rows.
  *Risk:* a table intentionally exempt would fail the grep — allow-list it explicitly with a comment.
- **P2 — A6 (GUESS):** if the sentinel risk is confirmed, `ALTER COLUMN screen_generation TYPE bigint
  USING ...::bigint`. *Risk:* table rewrite/lock; only after worker 0338 fixes the read sentinel.

## Rollout, risks and decisions

- Land A3/A4 first (pure indexes/constraints, no behaviour change), then stage invariants as
  audit → log-only → enforce. Never edit 0017–0020; reconcile cross-space drift before P0.
- Every FK/CHECK above may reject existing rows: each needs a pre-migration audit query and a
  documented downgrade (`DROP CONSTRAINT` / `DROP INDEX`) per spec 04 §4.9.
- Takeover expiry and the private/team `bot_id` contract are product decisions (record in `docs/STATE.md`
  "Decisions needed"). No API/data-format change is proposed; no `docs/PARITY.md` DEVIATION needed.

## Suggested acceptance tests (REQ-tagged)

- `req_026_lease_recovery_scan_uses_the_expires_index`
- `req_026_leases_cannot_cross_space_boundaries`
- `req_026_computer_status_timestamps_are_coherent`
- `req_029_takeover_pair_is_set_or_null_together`
- `req_029_a_forgotten_takeover_can_expire`
- `req_031_private_and_team_computers_do_not_collide`
- `req_043_every_tenant_table_has_a_policy_and_system_dml_is_audited`
