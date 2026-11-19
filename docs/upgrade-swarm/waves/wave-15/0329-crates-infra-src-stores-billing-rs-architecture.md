# Worker #329 — Billing store architecture proposal (v0.4.0)

- Module: `crates/infra/src/stores/billing.rs` · Lens: architecture
- Status: proposal. Claims tagged CONFIRMED were verified by reading this file, `app/src/ports.rs`,
  `infra/src/tenant.rs` and `migrations/0044_money.sql`; GUESS items need owner confirmation.
- REQs in scope: REQ-018 (spending limit + payments), REQ-051 (subscriptions).

## Current structure (CONFIRMED)

`PostgresBillingStore { pool: PgPool }` implements the `BillingStore` port (`ports.rs:3896`) with
seven methods over three tables: `space_spending_limits`, `purchase_payments`, `subscriptions`
(migration 0044). Each method wraps exactly one `tenant::with_tenant` call, which is a real
transaction that sets `boma.space_id` / `boma.system_reason` for RLS. Reads synthesize defaults
when no row exists (disabled limit, `free()` subscription). Money is `i64` minor units — correct
per the standards. Validation is uneven: `set_limit` validates currency/cap in Rust *and* the DB
has CHECK constraints; `record_payment` validates nothing at the boundary.

## Structure target

Keep a single file (~250 lines — no module split yet). Add three private helpers so each upgrade
stays local:

- `fn member_space_id(context) -> Result<&str, AppError>`: dedupe the two repeated
  `TenantContext::Member` matches and reject `System` in one place.
- `fn month_bounds(prefix) -> Result<(DateTime<Utc>, DateTime<Utc>), AppError>`: parse `YYYY-MM`
  once into a half-open UTC range.
- `fn map_write_error(sqlx::Error) -> AppError`: unique/FK/check → domain error, connection/timeout
  → `Unavailable`; reuse the existing `tenant::is_unique_violation`.

## Concrete upgrades

### 1. Idempotent `record_payment` (REQ-018) — do first
CONFIRMED: `purchase_payments.id` is the PRIMARY KEY, and `tenant::is_unique_violation` already
exists, but this store maps every SQL error through `tenant::unavailable`. A retried approval
therefore surfaces as `Unavailable` instead of a clean no-op/conflict. Bind with `ON CONFLICT (id)
DO NOTHING` and treat a unique violation as success when the existing row matches the same
`(space_id, amount, currency, item)`.
Risks: a reused id with *different* payload must be rejected, not silently accepted (GUESS: spec
is silent — ask owner); `DO NOTHING` hides genuine duplicates from observability unless traced;
needs `req_018_payment_retry_is_idempotent` integration test.

### 2. One transaction for allowance + sargable month query (REQ-018)
CONFIRMED: `remaining_allowance` calls `limit()` then `spent_in_month()` — two separate
`with_tenant` transactions, so a payment committed between them yields a torn read. Also
`spent_in_month` uses `to_char(created_at AT TIME ZONE 'UTC','YYYY-MM') = $1`, which cannot use
the existing `purchase_payments_month (space_id, created_at DESC)` index. Compute UTC bounds once
and run `created_at >= $1 AND created_at < $2` inside a single `with_tenant` closure.
Risks: `month_prefix` is currently unvalidated (`YYYY-MM` must be checked); the spec says
"calendar month" but not the timezone (GUESS: keep UTC to match current behavior, flag as
DEVIATION if changed); needs `req_018_allowance_is_atomic` test with a concurrent insert.

### 3. Classify billing errors instead of blanket `Unavailable` (REQ-054)
CONFIRMED: `tenant::unavailable` logs the raw driver error at warn and returns `AppError::Unavailable`
for everything — constraint violations, missing FK (user/card deleted), connection loss alike.
Route through `map_write_error` so check/FK violations become stable domain errors and only
connection/timeout stays `Unavailable`. Never log `item`, `user_id` or `provider_ref`.
Risks: must not leak internals to clients (REQ-054); error-variant changes are observable —
record in `docs/PARITY.md` if the HTTP mapping shifts; add a unit test per variant.

### 4. Harden subscription upsert against webhook replay (REQ-051)
CONFIRMED: `activate_subscription`'s `ON CONFLICT DO UPDATE` overwrites `credits_minor_units` and
can null `provider_ref`. A replayed or stale webhook can reset seeded credits — money loss.
Make activation idempotent: only set credits on transition into paid (`status = 'active'` and no
prior row), and skip updates whose `current_period_end` is older than the stored one.
Risks: needs a provider event id or monotonic timestamp to dedupe reliably (GUESS: no column in
0044 — may require a migration and an ADR); changing replay semantics is a DEVIATION until
approved; needs `req_051_replayed_webhook_keeps_credits` test.

### 5. Close the System-context read hole and add REQ-named tests
CONFIRMED: writes reject `TenantContext::System`, but `limit`, `spent_in_month` and `subscription`
pass it to `with_tenant`, where RLS `is_system()` lifts the filter — so `spent_in_month` under a
System reason sums *all* spaces. Either require a space id for these reads or document the
aggregate as intentional. Add integration tests (testcontainers/Postgres) with REQ ids in names
for defaults, idempotency, atomic allowance and replay.
Risks: rejecting System reads could break an internal caller (audit/export) — grep callers first;
test DB cost in CI; GUESS on whether aggregate reads are used.

## Cross-cutting risks

- Do not split into submodules yet: extra indirection would fight the repo's one-store-per-file
  convention.
- Runtime-checked `sqlx::query` is used repo-wide; moving to `query!` here alone adds offline
  metadata for one file — defer to a workspace ADR, not this wave.
- Every behavior change above is candidate DEVIATION material: log in `docs/PARITY.md` and wait
  for approval before shipping (AGENTS.md).
