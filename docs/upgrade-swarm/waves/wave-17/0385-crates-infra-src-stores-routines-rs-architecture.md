# Worker 0385 — `crates/infra/src/stores/routines.rs` — architecture proposal (v0.4.0)

- Lens: architecture. Requirement: REQ-072 (local automation API and CLI) / REQ-023/024 (routines).
- Status: PROPOSAL only — read-only review; no source or config changed.

## What I inspected (CONFIRMED)
- Full `routines.rs` (408 lines), `app/ports.rs:1334-1545`, `stores/rows.rs:553-598`,
  `migrations/0014_routines.sql`, `tenant.rs:36-140`, `stores/runs.rs:330-374`,
  `stores/cards.rs:1-60`, `app/usecases.rs:5148-5177,5260-5339`,
  `spec/01-requirements.md:579-585`, worker 0257's domain proposal.
- The store wraps every method in `tenant::with_tenant` / `tenant::as_system`; each of those is a
  real transaction that sets `boma.current_space_id` and rolls back on error. Multi-statement
  writes (`record_fired`, `record_failure`, `record_completed`, `record_skip`) are already atomic.
- `due()` reads with a plain `SELECT ... FOR UPDATE`-free statement under `SystemReason::ScheduledJob`;
  `runs.rs::claim_next_queued` by contrast uses `FOR UPDATE SKIP LOCKED` + a fence.
- `dispatch_routines` reads `due`, checks `find_open_routine`, creates a run, then calls
  `record_fired`. Both `record_fired` and `record_completed` insert an `outcome='fired'` audit row.
- The 18-column projection + `boma.rfc3339(...)` list is repeated verbatim in all six queries.
- REQ-072's `routine-run` path reaches this store through `find`/`record_*` with an owner context,
  never through `due()`; RLS plus explicit `space_id` predicates give two isolation layers.

## Guiding principles
Keep the port contract and stored/wire text byte-stable; a behavior change is a DEVIATION
(`docs/PARITY.md` + owner approval). Prefer schema/query-local fixes over port signature changes.
Keep audit semantics unambiguous and tenant isolation as a belt-and-braces property.

## Upgrades (v0.4.0), each with a risk

### U1 — Make the due wake an atomic claim, not a read
- Add `claim_due(as_of, owner, lease_seconds)` using the `runs.rs` pattern: a CTE selecting due
  rows `FOR UPDATE SKIP LOCKED`, updating a `claimed_at`/`lease_expires_at` column in the same
  statement, returning the claimed set. Keep `due()` as a read-only adapter for the testkit and
  differential tests; have `dispatch_routines` use the claim. A claim that crashes is reclaimed on
  lease expiry, mirroring REQ-013/014 run leases.
- Why: today two dispatcher replicas — or a REQ-072 automation `routine-run` racing the tick — can
  both observe the same due row and both enqueue a run (CONFIRMED from `due()` and usecases).
- Risk: needs a migration plus an app-port method; a wrong lease length can double-fire or stall
  wakes, both externally visible. Keep the default lease generous, add a REQ-named concurrency
  test with two workers before switching the dispatcher over; file the behavior change as a
  DEVIATION if the owner wants the old at-least-once firing preserved.

### U2 — One outcome authority; stop double-recording `fired`
- Extract the `RoutineOutcome → routine_runs.outcome` mapping into a single function used by
  `record_fired`, `record_failure`, `record_completed`, `record_skip`; give `Once` its own
  persisted value (extend the CHECK in a migration) and give completion a distinct `completed`
  value instead of a second `fired` row. Detail strings become the single source for skip reasons.
- Why: the mapping is duplicated in four places and `Once` currently collapses to `skipped`;
  a wake + its completion write two `fired` audit rows for one run (CONFIRMED).
- Risk: `routine_runs` is an audit/golden surface — changing outcome text can break goldens and
  any external consumer. Keep existing literal values first; introduce new values additively,
  verify every golden, and check in `docs/STATE.md` before changing what is persisted.

### U3 — Single projection constant to kill sixfold drift
- Define `const ROUTINE_COLUMNS: &str` (or a `macro_rules!`) for the id…updated_at projection and
  reuse it in `create`, `find`, `list`, `due`, `record_fired`, `record_failure`; build the three
  `UPDATE ... RETURNING` statements from one static SQL fragment. Keep `query_as` compile-time
  checking intact.
- Why: adding a routine column today means editing six identical lists; one omission silently
  maps the wrong field or fails only at runtime (CONFIRMED).
- Risk: low and internal. Generated SQL must stay semantically identical; run the full integration
  suite before and after, since SQLx metadata is checked at build time but row order is not.

### U4 — Typed timestamp and threshold boundaries
- Introduce a store-local RFC3339 `Ts` newtype with `try_new`/`as_str`, bind it in all timestamptz
  slots, and validate `threshold: u32` before binding rather than `i32::try_from(...).unwrap_or(i32::MAX)`.
  In `rows::routine`, replace the silent `u32::try_from(...).unwrap_or(u32::MAX)` with an explicit
  error (the `CHECK (account_failures >= 0)` already makes the fallback unreachable in practice).
- Why: stringly timestamps and silent saturation hide malformed input from automation callers
  (CONFIRMED); this complements worker 0257's typed-instant proposal at the domain boundary.
- Risk: ripples into `app/ports.rs` signatures and testkit builders. Stage with `&str` shims;
  reject invalid input only after confirming no stored/wire timestamp format is in the contract.

### U5 — REQ-072 isolation tests and privacy-safe observability
- Add REQ-072-named integration tests proving a local-owner principal cannot fire, find, or record
  against a routine in another space, and that `due()` cannot be reached from the automation path.
  Add `tracing::instrument` spans to `due`/`record_*` with counts, IDs, and outcome only.
- Why: the owner-context + RLS boundary is the store's contribution to "same authorization rules"
  in REQ-072, yet it is currently implicit, and the module has no tracing (CONFIRMED).
- Risk: log volume and PII — never log `prompt`, `email_senders`, or `detail`; keep spans at DEBUG
  with counts only, and verify no secret is emitted before enabling in production.

## Suggested order (small steps, test-first)
U3 (safe cleanup) → U2 (audit semantics) → U4 (boundaries) → U5 (isolation tests) → U1 (schema/claim,
last because it is the only externally visible behavior change). Each step: `cargo fmt --check`,
`clippy -D warnings`, REQ-named integration tests.

## Verification for v0.4.0
- `fn req_072_automation_cannot_cross_tenant_boundaries()`, `fn req_072_due_is_not_automation_reachable()`.
- `fn req_024_two_dispatchers_fire_a_due_routine_once()` (fails today — the U1 regression test).
- `fn req_024_one_run_produces_one_fired_audit_row()` and `fn req_023_once_has_its_own_outcome()`.
- `fn req_023_routine_columns_projection_round_trips()` (guards U3's shared constant).

## Open questions (GUESS / decisions needed)
- Is at-most-once dispatch acceptable, or must a lost claim still fire (at-least-once parity)?
- Is `routine_runs.outcome` text a public/golden contract, or internal only — may we add values?
- Should the lease be a new column or an advisory lock (boring-technology and migration cost tradeoff)?
