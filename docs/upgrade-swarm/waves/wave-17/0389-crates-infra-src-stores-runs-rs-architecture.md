# v0.4.0 architecture proposal — `crates/infra/src/stores/runs.rs` (worker #389, lens: architecture)

Status: proposal only (read-only review). Claims are CONFIRMED from the module, `ports.rs::RunStore`,
`crates/infra/src/tenant.rs`, `spec/01-requirements.md` (REQ-012/013/014/024), `spec/06-decisions-needed.md`
(DEC-02) and `migrations/0002_m1.sql`, `0015`, `0016`, `0008` unless marked GUESS.

## Current structure (CONFIRMED)

`PostgresRunStore { pool: PgPool }` is a thin adapter over a 10-method `RunStore` port. Every method
opens a `tenant::{in_space, with_tenant, as_system}` transaction with a boxed `TenantFuture` and writes
raw parameterized SQL; there is no helper/seam between the port and the statements.

- REQ-012 state machine is split across layers: the domain owns the transition table, `retry` enforces
  `failed → queued` in SQL, but `set_status` binds any `RunStatus` with only a fence predicate — the
  store itself will persist `queued → completed` if a caller asks (CONFIRMED).
- The global claim (`claim_next_queued`) is a single `FOR UPDATE SKIP LOCKED` CTE that also reclaims
  expired leases with an incremented fence; `not_before` hides REQ-024 backoff runs (CONFIRMED).
- `find_open_routine` is a lone SELECT used for REQ-024 coalescing; there is no unique constraint
  making "one open run per routine" true in the database (CONFIRMED from migrations).
- Idempotent create maps *any* unique violation to `Conflict`, then re-reads by nonce; a violation of
  any other constraint ends as `Unavailable` with an error log (CONFIRMED).
- The ~22-column run projection is repeated in eight statements; `find_by_nonce` lists
  `steering_from_seq, steering_consumed_seq, attempt` **twice** (lines 171 and 178, CONFIRMED).

## Upgrade 1 — One projection, not eight (CONFIRMED gap)

Introduce private `RUN_COLUMNS` / `RUN_RETURNING` string constants (and an optional table-alias
constructor) used by every `SELECT`/`RETURNING`; delete the duplicated triple in `find_by_nonce`.
These are runtime `query_as::<_, RunRow>(&str)` calls, so a `const &str` is enough — no new dep, no
compile-time query change. Keep every `boma.rfc3339(...)` alias identical.

- **Risk:** low. A mistyped alias silently maps to the wrong `RunRow` field, so add a mapping test that
  asserts a seeded row round-trips through *each* read method (find, nonce, active, routine, claim,
  list, retry). Preserve all REQ-012/REQ-024 test names verbatim (trace_check counts declarations).
  GUESS: sqlx tolerates duplicate names today (maps by name), so the fix may be behavior-neutral.

## Upgrade 2 — Index the claim and coalescing hot paths (CONFIRMED gap)

The claim filters `status, not_before, lease_expires_at` and orders by `(created_at, id)` with
`FOR UPDATE SKIP LOCKED`; only `(status, created_at)` exists. `runs.routine_id` has no index at all —
not even the partial one `routine_runs` has. Add (a) a partial `runs_claimable_idx (created_at, id)
WHERE status='queued' OR (status IN ('leased','running') AND lease_expires_at IS NOT NULL)` and
(b) `runs_open_routine_idx (routine_id, created_at) WHERE routine_id IS NOT NULL AND status NOT IN
('completed','failed','cancelled')`, matching the predicates exactly (no `now()` in the index —
non-immutable).

- **Risk:** medium. Partial-index predicates must match the SQL verbatim or the planner ignores them;
  verify with `EXPLAIN` on a seeded queue *and* on a queue full of future `not_before` rows, and count
  rows sorted under `SKIP LOCKED`. Two extra indexes cost write amplification on every status change.
  If (b) becomes a UNIQUE index (Upgrade 3), this is a schema migration, not a code-only change.

## Upgrade 3 — Make REQ-024 coalescing a database invariant (CONFIRMED gap + DEVIATION risk)

`find_open_routine` is read-then-create in the usecase: two concurrent fires can both see "no open
run". Upgrade (b) above to a partial UNIQUE index on open routine runs and translate its violation in
`create` to "coalesced: record skip", keeping `find_open_routine` as a fast pre-check. This moves the
invariant from a race-prone query into the schema, which is where REQ-024's "no second run" belongs.

- **Risk:** high. Existing data may already hold two open runs per routine — the migration must detect
  and report that before creating the index. `create` currently cannot tell *which* unique index fired;
  use `sqlx::Error::Database::constraint()` to distinguish the nonce index from the routine index.
  Changing concurrent-create behavior is externally visible: ADR + `docs/PARITY.md` entry and approval.

## Upgrade 4 — Centralize the fence/transition guard (CONFIRMED gap)

`set_status`, `set_steering_consumed` and `claim_next_queued` each hand-roll "fence matches or is NULL"
plus zero-rows ⇒ `Conflict`; `set_steering_from` has no fence at all (it is a member write, not a
worker write — say so explicitly in its doc comment). Factor the fence predicate into one small SQL
builder and add a predecessor guard for terminal transitions in `set_status`
(`WHERE ... AND $2 NOT IN ('completed','failed','cancelled') OR <allowed predecessor>`), derived from
the domain transition table so the two do not drift. Resolve DEC-02 first: the module rejects stale
writes (CONFIRMED) while the spec recommendation is "ignore and warn" (REQ-012/013, CONFIRMED).

- **Risk:** high. Touching transition semantics affects the worker retry gate and the REQ-012/013
  concurrency tests; duplicating the domain table in SQL creates two sources of truth. Prefer a single
  exported predecessor map from `crates/domain/src/run.rs` consumed by both. Any stale-write policy
  change needs the DEC-02 decision recorded in `spec` and traceable test names kept intact.

## Upgrade 5 — Give the cross-tenant claim an explicit boundary (CONFIRMED + GUESS)

`claim_next_queued` picks `SystemReason::ScheduledJob` inside the adapter. Move that policy to the
port (e.g. a `SystemContext`/reason parameter, or a dedicated `claim` method on a system-scoped
trait) so the "why we cross tenants" decision is auditable at the call site, not buried in SQL.
Alongside, collapse the repeated `tenant::*` + boxed-`TenantFuture` boilerplate into a private helper
per operation class, keeping `as_system` for claims only.

- **Risk:** medium. Cross-crate port change: `MemoryRunStore` and all call sites must follow, and
  `SystemReason::ScheduledJob`'s wire value must not change (audit/DEVIATION). GUESS: usecases already
  pass a reason implicitly; confirm before changing the port signature.

## Deferred (GUESS)

Typing `created_at`/`not_before`/`at` from `String` to a timestamp newtype is a cross-crate ports
change; the correlated `messages.blocks` subquery in `list` is fine at page sizes used today.
