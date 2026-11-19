# Worker #371 — Architecture proposal: `crates/infra/src/stores/pending_tool_calls.rs` (v0.4.0)

Lens: architecture. Scope: the Postgres `PendingToolCallStore` adapter implementing REQ-017. The
port, domain type, memory adapter, use cases and migrations were read as context only. Read-only
worker: no code changed, no tests run. Everything below is CONFIRMED by files read in this worktree
on 2026-11-19; forward-looking items are marked GUESS until implemented and measured.

## Current structure (CONFIRMED)

- `PostgresPendingToolCallStore { pool }` is stateless: `const fn new`, one `#[async_trait]` impl
  with four methods (`create`, `find_pending`, `mark_applied`, `mark_refused`, lines 65–204) plus a
  private `pending_row_state` helper (208–233).
- Every method opens its own transaction through `tenant::with_tenant` (tenant.rs 51–63), which
  delegates to `in_space`/`as_system` and sets transaction-local `boma.space_id` /
  `boma.system_reason`; isolation relies on the RLS policy in `0021_pending_tool_calls.sql`
  (21–36). All SQL failures are flattened to `AppError::Unavailable` by `tenant::unavailable`
  (tenant.rs 153–156).
- The 10-column projection (`run_id … answered_at`, with `boma.rfc3339` on both timestamps) is
  spelled out **four times** (lines 81–83, 115–117, 150–152, 186–188) against one
  `PendingToolCallRow` (19–32) and one mapper `into_pending_tool_call` (34–49). The helper's
  `SELECT status` is a fifth, different shape.
- Idempotence is enforced twice: the DB primary key `(run_id, call_id)` with `ON CONFLICT DO
  NOTHING` on `create` (77–80), and `WHERE status = 'pending'` on the two conditional `UPDATE`s
  (149, 185). A replayed `create` returns `Conflict` (98–103), which the use case absorbs
  (usecases.rs 6906–6912); a failed conditional update disambiguates via `pending_row_state`
  (`Conflict` if the row exists, `NotFound` otherwise).
- The *effect-level* idempotence key lives elsewhere: the app derives `tool:{run_id}:{call_id}` and
  calls `stores.effects.record` before executing (usecases.rs 7104–7153). The pending row stores no
  effect key, so the two sources of "already applied" are separate and can disagree.
- Schema (`0021` + `0022`): PK `(run_id, call_id)`, status CHECK, RLS policy; **no** secondary
  index for `find_pending` (`WHERE run_id = $1 AND status = 'pending' ORDER BY created_at, call_id`,
  115–121). `created_at`/`answered_at` are `timestamptz` in DB but opaque `String`s in the port,
  converted by `boma.rfc3339` and bound as text.
- The memory adapter (memory.rs 3112–3217) is mostly conformant, but on an invisible space its
  `create` returns `NotFound` (3119–3121) whereas the Postgres INSERT is rejected by RLS and
  surfaced as `Unavailable`. No `tracing::instrument`, no store-level tests and no REQ-017-named
  test exist in this module.

## Concrete upgrades for v0.4.0 (with risks)

### U1 — One projection, one row mapping (HIGH value, LOW risk)
Hoist the 10-column `RETURNING`/`SELECT` list into a single `const PENDING_COLUMNS: &str` (or one
`query_as` builder) and reuse it in all four statements; keep `PendingToolCallRow` as the only row
type and `into_pending_tool_call` as the only mapper. Risk: the `boma.rfc3339` aliases must stay
byte-identical or `FromRow` breaks; these are runtime `sqlx::query*` calls, so compile-time
offline checking does not protect them — add a conformance test that round-trips every status.
Net effect: ~30 fewer lines and one place where the projection can drift.

### U2 — Make tenant scope explicit and convergence-tested (HIGH)
Decide one rule for `TenantContext::System` (RLS bypass) and for a `NewPendingToolCall` whose
`space_id` differs from the member context, bind/assert the space explicitly where cheap, and add
a dual-adapter conformance test replaying create/find/answer under member-of-space, member-of-
other-space and system contexts. Risk: system maintenance callers may currently rely on silent
cross-tenant reads; ADR-002's static `boma.space_id` grep and single-setting invariant must stay
intact; do not widen scope. GUESS: no system-context caller exists today — confirm with a grep
before tightening.

### U3 — Typed error mapping at the adapter boundary (MEDIUM)
`tenant::unavailable` collapses FK violations, RLS denials, serialization failures and a dead DB
into one `Unavailable`. Classify constraint violations as `Conflict`, missing FK as `NotFound` and
keep connection failures as `Unavailable`; keep driver detail in `tracing` only (REQ-054). Risk:
error shape is part of the stable API format — any externally visible change needs a `docs/PARITY.md`
entry, must match `MemoryPendingToolCallStore`, and needs a REQ-017-named regression test. Preserve
`Conflict` on replay (usecases.rs 6908) and `Conflict`/`NotFound` from `pending_row_state`.

### U4 — Single-source the "exactly once" authority (MEDIUM/HIGH, cross-aggregate)
Persist the effect key (`tool:{run_id}:{call_id}`) on the pending row via a new migration, and have
`mark_applied` record the effect and flip the status in one transaction, so a crash cannot leave a
pending row whose effect already ran and replay cannot produce a different "already applied" note.
Risk: touches a migration, the port structs, `MemoryPendingToolCallStore` and usecases.rs 7104–7153;
the replay note is an externally visible tool-result body → potential DEVIATION needing approval;
cross-aggregate refactor → ADR first. Until then, keep the two guards but add a test proving the
crash window is handled (GUESS: it is, via `effects.record` Conflict on replay).

### U5 — Hot-query index, temporal typing, observability and traceability (LOW/MEDIUM)
Add a partial index `(run_id, created_at, call_id) WHERE status = 'pending'` for the `find_pending`
hot path; make `created_at`/`answered_at` typed instants at the boundary instead of `String` (or at
least validate RFC3339 on bind); add `#[tracing::instrument(skip_all)]` spans with run/call ids but
**never** log `arguments` or tool results; add REQ-017-named integration tests for replay, double
answer, cross-tenant isolation and `find_pending` ordering. Risk: the port uses `String`, so typed
timestamps have a large blast radius (DEVIATION if public) — prefer internal validation first; the
index adds write cost and its benefit is GUESS until an `EXPLAIN` on a seeded run; spans add log
volume, so keep fields redacted.

## Sequencing (GUESS)

U1 → U2 → U3 first (structure, then scope, then error semantics); U4 is independently shippable
but needs an ADR and a PARITY decision; U5 can land incrementally, index and tests before any
temporal change. No new features outside REQ-017, no silent port or error-shape change.
