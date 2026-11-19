# 0405 — `crates/infra/src/stores/tickets.rs` — architecture lens — v0.4.0 proposal

Worker #405 · module `crates/infra/src/stores/tickets.rs` (242 lines) · lens: architecture (layering,
type safety, query structure, consistency boundaries, duplication) · READ-ONLY review of
`rebuild-workspace/.worktrees/before-m5` (v0.3.0-rc5). Spec anchor: REQ-038. CONFIRMED = read this
session; runtime likelihood/impact = GUESS. Proposal only; no code changed here.

## 1. Current structure (CONFIRMED)

- One adapter `PostgresTicketStore { pool: PgPool }` with `new()`; one `TicketStore` impl with five
  methods: `list`, `create`, `move_to`, `comment`, `get` (ports.rs:3254-3306). No helper module, no
  unit tests in-file; integration coverage exists (`req_038_tickets_round_trip_through_postgres`,
  `crates/infra/tests/m1.rs:2654`; RPC surface tests in `api/tests/rpc_surface.rs:2785,2815`).
- Every method is the same shape: `tenant::with_tenant` → `Box::pin(async move { … })`, whose
  transaction sets `boma.space_id` transaction-scoped (tenant.rs:51-63,107-140). Multi-statement
  writes (`create`, `comment`) are correctly atomic in that one transaction.
- A local positional `TicketRow` 10-tuple (:26-37) and `summary_from` destructor mapper (:39-64)
  convert `String`/`i32` fields to `TicketSummary`. The full `SELECT` projection is inlined three
  times via `concat!` (:71-76, :130-135, :227-232), no shared constant.
- `create` (:86-144): two duplicated `match context` arms extract `space_id` and `created_by`,
  both returning `NotFound` for `System`; then `board_state` upsert (`ON CONFLICT DO UPDATE …
  RETURNING next_seq - 1`) allocates `BOT-<seq>`, insert, then a second `SELECT` rebuilds the
  summary. The counter row lock makes key allocation race-free (migration 0038:7-10).
- `move_to` (:146-166): `UPDATE … WHERE id = $1`, no explicit `space_id` predicate — tenant
  isolation rests entirely on RLS (0038:33-37); returns `rows_affected() > 0`.
- `comment` (:168-217): existence `SELECT id` round trip, insert comment, then bump the
  denormalised `tickets.comment_count` and set `updated_at = now()`.
- `get` (:219-241): `SELECT … WHERE id = $1`, optional row.

## 2. Architecture gaps (CONFIRMED shape)

- **Positional row mapping + repeated SQL.** A column-order change silently reshuffles ten fields;
  the compiler cannot catch it. Sibling stores already show better patterns: `artifacts.rs` uses
  named `SQL_*` consts, `cards.rs` uses `query_as` with `… RETURNING` and row helpers in `rows.rs`.
- **Two clocks.** `create` binds the app-supplied RFC3339 `created_at` string; `move_to`/`comment`
  write `updated_at = now()` from Postgres. `ORDER BY key` is fine, but any future time-based
  query would mix clocks (same finding as worker #398 on signin.rs).
- **Denormalised counter with no repair path.** `comment_count` is maintained only by this one
  `UPDATE`; no trigger, no reconciliation query, no delete method. Any other writer (migration,
  future comment edit/delete) drifts the counter permanently.
- **Strings where the domain already has types.** `boma_domain::ticket::TicketState` with
  `as_str`/`parse` exists (domain/src/ticket.rs:14-56) and a DB `CHECK` (0038:24), but the store
  accepts arbitrary `&str` and lets the DB reject it. `TicketSummary`/`NewTicket` and
  `ticket_id: &str` in the port are plain `String` — the layering stops at the DB constraint.
- **Error classification.** All driver errors become `AppError::Unavailable` via
  `tenant::unavailable`. A duplicate `ticket.id` (caller-supplied PK) or `(space_id, key)`
  collision is a permanent conflict but is reported as a *transient* fault clients retry.
  `tenant::is_unique_violation` exists (tenant.rs:158-164) and is already used in cards.rs:110.
- **Extra round trips.** `create`'s re-SELECT and `comment`'s existence probe are each a statement
  that could be folded into the write (`INSERT … RETURNING` / a CTE). Minor, but it is the same
  "one method = N statements" shape that grows as features are added.
- Non-goal (product decision, not a store fix): `list` has no state/assignee filter or pagination;
  the board filters client-side. Do not add filters without an owner-approved REQ/ADR.

## 3. Concrete upgrades for v0.4.0 (ordered by value/risk)

- **A. Named row + shared projection (low risk, highest value).** Replace the 10-tuple with a
  `#[derive(sqlx::FromRow)] TicketRow` struct with named fields and `boma.rfc3339(...) AS ...`
  aliases; extract the repeated column list into one `const SQL_TICKET_COLUMNS`. Follows
  artifacts.rs/cards.rs. *Risk:* purely local, reversible; must keep the exact projection order for
  `query_as` and re-run the REQ-038 round-trip test.
- **B. Typed error mapping (low-medium).** Map unique violations to `AppError::Conflict` via
  `tenant::is_unique_violation`; keep `Unavailable` for real DB faults. Additive, but check the API
  error mapping so a conflict does not leak internals. *Risk:* observable status change for
  duplicate ids — record in `docs/PARITY.md` if clients can see it.
- **C. Fold the extra round trips (low-medium).** `create`: `INSERT … RETURNING` with server-side
  `boma.rfc3339(created_at/updated_at)` (cards.rs:91-97 precedent), dropping the re-SELECT.
  `comment`: single `WITH found AS (SELECT id …) INSERT … SELECT … FROM found RETURNING …`, then
  the counter bump. *Risk:* must preserve `false`/`None` semantics and the atomic counter bump;
  regression-test both before changing (test-first).
- **D. State/author vocabulary at the boundary (medium).** Parse `TicketState` and `author_kind`
  inside the store (boma_domain::ticket::parse / a closed enum) before binding, instead of trusting
  the DB CHECK; keep `String` only in the port/DTO layer. Longer term, move `TicketId`/`TicketKey`
  newtypes into the port as other stores do. *Risk:* changing port signatures ripples into the
  in-memory double (`testkit/memory.rs:6980+`) and RPC DTOs — needs an ADR, and a DEVIATION entry
  if the wire changes. Can be split: store-side validation first, newtypes later.
- **E. Single source of truth for counters/time (medium-high).** Derive `comment_count` from
  `ticket_comments` (view or trigger) and use one clock for all timestamps (DB `now()` on both
  create and update, or validate the app RFC3339/UTC before binding). *Risk:* new migration +
  trigger and a wire-visible timestamp shape change — ADR, `docs/PARITY.md`, owner sign-off;
  verify no golden relies on the current stored format.

## 4. Risks / non-goals / gates

- Keep every method inside `tenant::with_tenant`; never move `boma.space_id` out of tenant.rs
  (ADR-002). RLS remains the isolation backstop even if explicit predicates are added.
- `unsafe_code` forbidden; clippy `-D warnings`; no `unwrap`/`expect`/`panic` in non-test code;
  keep driver detail at debug level (REQ-054) and never log ticket bodies (REQ-038/privacy).
- Sequence A → B → C → D → E. A/B/C are local and reversible; D/E change contracts and need
  owner approval. `tools/trace_check.py` counts declaration lines, not comments — new tests must
  carry `req_038` in the function name. All impact above is GUESS until those tests run.

## 5. Suggested tests (integration, real PG)

- `req_038_a_duplicate_ticket_id_is_a_conflict_not_unavailable`
- `req_038_create_returns_the_inserted_summary_without_a_second_query_shape_change`
- `req_038_comment_on_a_missing_ticket_yields_none_and_keeps_the_counter_exact`
- `req_038_an_unknown_state_is_refused_before_the_database_check`
- `req_038_comment_count_stays_exact_after_concurrent_comments`

## Bottom line

The store is small and its hard guarantees (transactional key allocation, RLS isolation, atomic
counter bump) are correct. The real v0.4.0 architecture work is making the mapping and SQL DRY
(A), classifying errors honestly (B), removing two redundant round trips (C), pulling validation
up to the domain boundary (D), and removing the denormalised-counter/two-clock drift (E). A–C are
safe now; D–E are contract changes that need ADRs and the owner's sign-off.
