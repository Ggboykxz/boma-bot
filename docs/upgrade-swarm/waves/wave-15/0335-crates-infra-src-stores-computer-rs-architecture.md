# 0335 — `crates/infra/src/stores/computer.rs` — architecture (v0.4.0)

- Worker #335 · Lens: architecture · Target: v0.4.0 · Read-only review; no code was changed.
- Module: `crates/infra/src/stores/computer.rs` (227 lines) at `before-m5` (v0.3.0-rc5): the
  Postgres adapter for `ComputerStore` (REQ-026), exported from `crates/infra/src/lib.rs`.
- Read for context (CONFIRMED by direct reads): `crates/app/src/ports.rs` :1693–1782 (`NewComputer`,
  `ComputerStore`), `crates/infra/src/stores/computer_row.rs` (`ComputerRow`, `COMPUTER_COLUMNS`,
  `computer()`), `crates/infra/src/tenant.rs` (`with_tenant`, `as_system`, `unavailable`),
  `crates/infra/src/stores/sessions.rs` (sibling adapter style), `migrations/0017_computers.sql`
  and `migrations/0020_team_computers.sql` (schema, partial/unique indexes, RLS), `AGENTS.md`
  (Errors / Async / Persistence / Observability / Traceability standards).

## Current structure (CONFIRMED)

`PostgresComputerStore` is a thin `#[derive(Clone)]` wrapper over one `PgPool` (`new` at :21–27;
note `sessions.rs` uses `const fn new` and derives `Debug, Clone`). All six trait methods implement
`ComputerStore`: three reads (`find` by bot_id :31–50, `find_team` by space_id AND `NOT private`
:52–72, `find_by_id` :74–93) and three writes (`create` :95–131, full-row `update` :133–202,
`running_idle` :204–226).

Shape of the structure:

1. Tenant boundary — every read/write runs inside `tenant::with_tenant`, i.e. a `BEGIN`, two
   transaction-scoped `set_config` calls (`boma.space_id`, `boma.system_reason`), the body, then
   commit/rollback (REQ-043, ADR-002). `running_idle` uses `tenant::as_system` with
   `SystemReason::ScheduledJob` because it sweeps across tenants (D-09 closed enum).
2. Row boundary — reads project `COMPUTER_COLUMNS` from `computer_row.rs` via
   `AssertSqlSafe(format!(...))`; timestamps are converted by `boma.rfc3339()` into strings; the
   mapper `computer()` converts i32/u64 and validates all newtypes. `create` writes literal SQL,
   `update` writes plain `sqlx::query` with 16 positional binds.
3. Schema boundary — `UNIQUE (space_id, bot_id)` plus `computers_bot_idx` make `find` single-row
   inside one RLS space; the partial unique index `computers_team_per_space ... WHERE NOT private`
   is what makes `find_team`'s `fetch_optional` safe; `computers_idle_idx` serves the sweep. RLS is
   FORCE-enabled on `computers` with a `space_id`/`is_system()` policy.

Every failure (driver error, constraint violation, zero rows) funnels through
`tenant::unavailable`, which logs the driver detail at `warn` and returns a single `AppError::Unavailable`.

## Guiding principle for v0.4.0

Keep the port signature, the RLS wrapper and the row projection as the three stable seams; harden
what happens *inside* them without changing the public ABI. The adapter is a pure persistence
adapter — no new policy, retry or scheduling belongs here. Because the API error format and goldens
observe these errors, any new variant or message is a DEVIATION to record in `docs/PARITY.md`
before shipping (AGENTS.md).

## Concrete upgrades (5), each with a risk

1. **Factor the three read paths into one private helper.** `find`, `find_by_id` and `find_team`
   repeat the same `with_tenant` + `AssertSqlSafe(format!("SELECT {COMPUTER_COLUMNS} ..."))` +
   `fetch_optional` + `row.map(computer).transpose()` sequence. Extract a private
   `fetch_one_in(context, sql, bind)` that only varies the WHERE clause and binds.
   *Risk:* a mistaken SQL fragment or dropping `with_tenant` would read across tenants; the
   `format!` + `AssertSqlSafe` pattern is load-bearing for compile-time SQL safety.
   *Mitigation:* keep the tenant transaction inside the helper (never outside), keep the exact
   `COMPUTER_COLUMNS` projection, and add a two-space integration test asserting each finder sees
   only its own tenant plus the `NOT private` team rule.

2. **Give `create` real conflict semantics.** Today `ON CONFLICT (id) DO NOTHING ... RETURNING`
   followed by `fetch_one` turns a duplicate into `RowNotFound`, which `tenant::unavailable`
   relabels `Unavailable` (a 503-shaped answer for a client duplicate). Map the conflict to a
   typed error (or return the existing row), and let `is_unique_violation` from `tenant.rs` do the
   classification. *Risk:* the port doc says create errors "when the computer already exists", so
   the outcome must stay an error; changing the variant/text still changes the API error contract
   and goldens. *Mitigation:* introduce the narrowest typed error consistent with the existing
   `AppError` shape, keep the message stable, add a duplicate-create test, and run the contract and
   golden suites before shipping.

3. **Add a concurrency guard to `update`.** It currently overwrites all 15 columns with a blind
   `WHERE id = $1` and ignores `rows_affected()`: two racing writers (e.g. a status transition and a
   screen-generation bump) silently lose one update, and an update to a deleted id returns `Ok(())`.
   Add an expected-version comparison (reuse `screen_generation` as a fence, or add a version
   column via a migration) and check `rows_affected()`; surface not-found/stale as typed errors.
   *Risk:* introducing a `WHERE` guard or a column changes externally visible update behavior and
   needs a migration — a DEVIATION/ADR, and other workers own the runtime that bumps
   `screen_generation`. *Mitigation:* coordinate with the runtime/use-case owners, default to the
   no-migration `screen_generation` guard, add a lost-update regression test and a row-affected
   assertion, and record the chosen semantics in `docs/PARITY.md`.

4. **Stop silently saturating numeric limits.** `create` uses
   `i32::try_from(memory_limit_mb/cpu...).unwrap_or(i32::MAX)` (:120, :122) and `update` does the
   same for five fields (:142, :145, :147); overflow becomes `i32::MAX`, which then violates the
   schema CHECK (64–4096 MB, 1–1440 min) and surfaces as `Unavailable`. Convert with typed errors
   (or reject at the domain boundary) instead of `unwrap_or`. *Risk:* a different error path and
   message is wire-visible, and the mapper in `computer_row.rs` still saturates u64 conversions —
   fixing only this file leaves the asymmetry. *Mitigation:* validate limits once at the port
   boundary, add proptest cases at the CHECK boundaries, and coordinate the mapper fix with the
   `computer_row.rs` worker so both directions agree.

5. **Add structured observability at the adapter seam.** There is no `tracing` instrumentation;
   the only signal is `tenant::unavailable` logging the raw driver error at `warn` on every failure.
   Add `#[tracing::instrument]` spans with a safe field set (`computer_id`, operation, outcome) and
   a single typed-error mapping so constraint/not-found/unavailable are distinguishable in logs.
   *Risk:* logging tenant ids, SQL text or the DSN would leak personal data or connection details
   (REQ-054), and new `AppError` variants affect the API error contract. *Mitigation:* log only
   reason codes and opaque ids, never SQL or connection strings, keep client-facing messages
   verbatim, and verify the OpenAPI/contract plus golden suites; also make `new` `const` and derive
   `Debug` to match sibling stores.

## Cross-cutting risks / sequencing

- Sequence 1 → 4 → 2 → 3 → 5: dedupe and numeric validation first (cheap safety net), then conflict
  semantics, then the concurrency guard (migration/ADR + cross-worker coordination), then logging.
- Freeze the `ComputerStore` trait, `NewComputer`, `COMPUTER_COLUMNS` and the RLS wrapper; the
  use cases (`crates/app/src/usecases.rs`), the runtime that mutates `Computer`, and the
  `computer_row.rs` mapper are other workers' scope — coordinate, do not duplicate.
- Keep every statement parameterized and inside `with_tenant`/`as_system`; never bypass RLS.
- Gate each step with `cargo fmt --check`, `clippy -D warnings`, unit + `tests/` integration,
  golden and OpenAPI contract suites. No new dependency is required for items 1–5.

## Out of scope for this module

The `Computer` domain type and its state machine, the use cases that call this port, the
`computer_row.rs` mapper and its saturating conversions, the migrations and RLS policies, and the
idle-sweep scheduler are other modules. This proposal only restructures the adapter in `computer.rs`.
