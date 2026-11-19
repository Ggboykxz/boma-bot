# 0499 — `migrations/0044_money.sql` — architecture (v0.4.0)

- Worker #499 · Lens: architecture (structure, boundaries, coupling, debt) · Target: v0.4.0.
- Status: PROPOSAL (read-only review; no migration or code changed). CONFIRMED = verified by direct
  reads in this worktree; GUESS = reasoned, not executed against a live DB.
- Module: `migrations/0044_money.sql` (58 lines): the money spine for REQ-018 (a space caps model
  spend; a payment exists only after approval) and REQ-051 (plan, credits, subscription state).
- Read for context (CONFIRMED): `0044` itself, `0043_models.sql`, `0056_notifications.sql` (an
  unrelated table also named `subscriptions`), `crates/infra/src/stores/billing.rs`,
  `crates/app/src/ports.rs:3815–3950`, `crates/infra/tests/m1.rs:3181`,
  `crates/testkit/src/build.rs:6903`, `spec/01-requirements.md` REQ-018/REQ-051,
  `spec/02-data-model.md:145–158`, `spec/openapi.yaml:345` (webhook idempotent per event id).

## Current structure (CONFIRMED)

Three tables, each with `ENABLE` + `FORCE ROW LEVEL SECURITY` and the same
`space_id = boma.current_space_id() OR boma.is_system()` policy on USING and WITH CHECK:
- `space_spending_limits`: `space_id` PK/FK `spaces ON DELETE CASCADE`, `enabled bool DEFAULT false`,
  `currency text DEFAULT 'USD' CHECK (char_length(currency) = 3)`, `cap_minor_units bigint >= 0`,
  `updated_at`.
- `purchase_payments`: `id text` PK (caller-generated), FKs `spaces`/`users` CASCADE and `cards`
  `SET NULL`, `item text`, `amount_minor_units bigint >= 0`, `currency text`, `created_at`. One index
  `purchase_payments_month (space_id, created_at DESC)`.
- `subscriptions`: `space_id` PK/FK `CASCADE`, `plan CHECK ('free','pro')`,
  `status CHECK ('active','canceled','past_due')`, nullable `provider_ref`,
  `credits_minor_units bigint NOT NULL DEFAULT 0` (no non-negative CHECK), `current_period_end`,
  `created_at`, `updated_at`. No row means `free` (store `free()` at `billing.rs:26`); there is no
  `billing_events` table although the data model lists `BillingAuditEvent`.
- Store coupling (CONFIRMED): `spent_in_month` (`billing.rs:137`) sums `amount_minor_units` with
  `to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM') = $1`, so the existing index cannot serve the
  predicate and the sum mixes currencies with no filter. `set_limit`, `record_payment` and
  `activate_subscription` are plain `ON CONFLICT` upserts; the payment insert carries no approval,
  run or event key, and activation overwrites plan/status in place with no history.

## Guiding principle for v0.4.0

Additive only: never edit `0044` (its sqlx checksum is recorded), never DROP/RENAME/rewrite a column,
constraint, row or policy. Every item is a NEW object, a new `NOT VALID` constraint, or a new test.
Keep the wire-visible contract — the `free`/`pro` and status strings, `SpendingLimit` fields, the
`YYYY-MM` month key and the RLS semantics — byte-stable; any external delta is a DEVIATION to record
in `docs/PARITY.md` (AGENTS.md).

## Concrete upgrades (5), each with a risk

1. **Idempotency keys on payments.** Add nullable `approval_id text`, `run_id text`, `event_id text`
   to `purchase_payments` plus partial UNIQUE indexes `WHERE <col> IS NOT NULL`, so REQ-018's
   "payment only after approval" and the webhook's "idempotent per event id" become DB-enforced.
   *Risk:* legacy rows have NULLs (partial indexes tolerate them), but two approvals with different
   ids can still double-charge — the gate must reserve the key before insert. *Mitigation:* land the
   columns unchanged in behaviour, add a REQ-018 test for a replayed event id, keep the existing
   caller-generated `id` as PK.

2. **Currency-aware spend.** Add `money_currencies (code text PRIMARY KEY)` seeded with the codes
   actually in use, `NOT VALID` FKs from both `currency` columns (text-to-text types match), and a
   read-only `spend_by_month_currency` view grouping payments by UTC month and currency. *Risk:* a
   legacy three-letter but non-ISO code, or an unseeded code, makes a later `VALIDATE` fail.
   *Mitigation:* pre-flight `SELECT DISTINCT currency` on both tables, seed every observed code,
   validate only when the read shows no orphans, and leave the `char_length = 3` CHECK untouched.
   *GUESS:* cross-currency summing is not exercised by the current tests.

3. **Make the month aggregation indexable.** Add a trigger-maintained `billing_month char(7)` column
   and an index `(space_id, billing_month)`, then let the store use an equality predicate. *Risk:* a
   generated column cannot express `to_char(created_at AT TIME ZONE 'UTC','YYYY-MM')` because the
   timezone conversion is STABLE, not IMMUTABLE, so a trigger plus a backfill is required before the
   app reads it. *Mitigation:* keep `to_char` as the fallback until a REQ-018 test compares both
   aggregates on a seeded DB; the zero-schema alternative is a half-open `created_at` range, which
   already uses `purchase_payments_month` (CONFIRMED the index exists).

4. **Append-only billing audit ledger.** Add `billing_events` (id, `space_id` FK CASCADE, kind,
   related subscription/payment id, `provider_event_ref`, `payload jsonb`, `created_at`) with the same
   RLS pattern, and a trigger on `subscriptions` recording plan/status transitions. *Risk:* a trigger
   runs under the writer's RLS context, so webhook system writes need the `boma.is_system()` branch,
   and replayed provider events can duplicate rows. *Mitigation:* dedupe on `provider_event_ref` with
   a partial unique index and test both member and system write paths with a REQ-051 test.

5. **Freeze the shape and the effective-plan semantics.** Add
   `req_018_051_money_shape_and_rls` in `crates/infra/tests/m1.rs` asserting the three tables'
   column names/types/nullability, policy names, RLS enabled+forced and the existing index; add a
   `NOT VALID` `credits_minor_units >= 0` CHECK (absent today, CONFIRMED) and a read-only
   `v_effective_subscription` view mapping "no row / canceled / past period end" to the exact
   `free()` strings. *Risk:* pg_catalog assertions are version-brittle and a second source of truth
   for plan status can drift from the Rust enum; legacy negative credits would block validation.
   *Mitigation:* assert by object name/prefix on the pinned embedded PG, derive expectations from
   shared constants, pre-flight for negative credits, and prove the view against `free()` byte-for-byte.

## Sequencing and gates

- Land 5 first (freeze the shape), then 2 → 1 → 4 → 3: contract, currency, idempotency, ledger,
  query speed. Each item is one forward-only migration and one commit mentioning REQ-018/REQ-051.
- Keep `0044` byte-identical; regenerate SQLx offline metadata for every new object. CI migrates a
  fresh DB, replays the REQ-018/REQ-051 tests and runs `tools/trace_check.py` (REQ id in test names).
- Coordinate with `crates/infra/src/stores/billing.rs`, the only writer of these tables
  (CONFIRMED): a migration-only change stays inert until the store uses the new columns. No new crate.

## Out of scope

The `BillingStore` port and webhook signature verification, plan-limit policy, the unrelated
`notification_subscriptions` table in `0056`, and any in-place edit of `0044` belong to other briefs.
