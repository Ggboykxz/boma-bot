# v0.4.0 reliability proposal — `crates/infra/src/stores/billing.rs`

Lens: reliability. Worker #330. Scope: the Postgres `BillingStore` adapter and the write/read
paths that feed it. All claims below are CONFIRMED by reading `billing.rs`,
`migrations/0044_money.sql`, `crates/app/src/usecases.rs` (`billing_webhook`,
`approved_purchase_request`), `crates/domain/src/billing.rs`, `crates/app/src/ports.rs`, and
`crates/infra/src/tenant.rs` in this worktree. No code was changed; this is read-only analysis.

## Failure modes

### 1. Stale entitlement (CONFIRMED)
- `subscription()` (lines 184-211) returns the stored `plan`/`status` verbatim and never compares
  `current_period_end` to "now". A pro row whose period has lapsed still reads as `pro/active`.
- `billing_webhook` only applies `checkout.session.completed`. Cancellation/downgrade events
  (`customer.subscription.deleted`, `customer.subscription.updated`, `invoice.payment_failed`)
  are acknowledged and ignored, so a canceled payer stays entitled indefinitely — the store has
  no path that ever downgrades a row. This is the root of stale entitlement, not just a read bug.
- `activate_subscription` (lines 213-246) re-seeds `credits_minor_units` to the fixed
  `PRO_SEEDED_CREDITS_MINOR` on every apply; a late replay wipes consumed credits back to the seed.
- `limit()`/`subscription()` use `fetch_optional` and fall back to safe defaults; that part is sound.

### 2. Webhook races (CONFIRMED)
- The upsert at lines 221-231 is an unconditional blind overwrite (`DO UPDATE SET ...`). There is
  no version, `event_created`, or `WHERE updated_at < EXCLUDED.updated_at` guard, so delivery order
  wins, not event order. A stale `checkout.session.completed` arriving after a newer state write
  reverts status and re-seeds credits (last-writer-wins).
- Postgres retries are at-least-once. `billing_webhook` keys nothing by provider `event.id`; a
  second delivery with a different event id for the same subscription applies twice. The signature
  window (5 min, `domain/billing.rs`) prevents old-body replay but not duplicate fresh deliveries.
- Clock split: inserts use the app-clock `created_at` for both columns, but `updated_at` uses DB
  `now()` on conflict. App/DB clock skew makes ordering by `updated_at` unreliable even once a
  guard is added — a monotonic provider timestamp is needed instead.
- Each write is atomic at row level (single upsert in `with_tenant`'s transaction), but there is no
  compare-and-swap across the read-modify-write, so concurrent webhook workers can interleave.

### 3. Idempotency (CONFIRMED)
- `record_payment` (lines 151-182) is a plain `INSERT` on PK `id`. A duplicate of the same id
  violates the PK and `tenant::unavailable` maps it to `AppError::Unavailable` (a 503-class error)
  instead of a clean idempotent success. `tenant::is_unique_violation` already exists but is unused.
- The id itself is minted per attempt (`new_id("pay")` in `approved_purchase_request`), so a retried
  approval produces a new id and double-counts spend — the PK cannot dedupe retries at all.
- Cap enforcement is TOCTOU: `purchase_gate` reads `remaining_allowance` in one transaction, the
  payment is inserted later in another. Two concurrent approvals both pass and both record,
  exceeding the cap. No `FOR UPDATE` on the limit row and no atomic check-and-insert.
- `spent_in_month` (lines 128-149) sums *all* currencies into one number and compares it to a
  single-currency cap (`remaining_allowance`, lines 115-126); `record_payment` never validates
  `currency`. Mixed-currency spend silently corrupts the allowance.
- `to_char(created_at AT TIME ZONE 'UTC','YYYY-MM') = $1` wraps the column, defeating the
  `purchase_payments_month (space_id, created_at DESC)` index. Allowance checks sit on the hot path
  of model calls, so cost grows with payment history — a latency/availability risk under load.

## Concrete hardening (v0.4.0)

1. **Monotonic, idempotent webhook apply.** Add a migration with a `billing_events` table keyed by
   provider `event.id`, plus an `event_created timestamptz` column on `subscriptions`. In one
   transaction: insert the event id (`ON CONFLICT DO NOTHING` → no-op if already applied), then
   upsert with `... DO UPDATE SET ... WHERE subscriptions.event_created < EXCLUDED.event_created`.
   Update `NewSubscription` to carry `event_id` + `event_created`. Ordering then follows the
   provider, not arrival. Risk: port/migration change; dedupe scope must match the provider's event
   id lifecycle; clock guard must tolerate provider skew (use provider timestamp, not DB `now()`).

2. **Entitlement transitions + expiry.** Handle cancellation/downgrade/failed-invoice events so a
   row can move to `canceled`/`past_due`/`free`, and compute an effective status at read time from
   `current_period_end` (expired pro is not entitled). Preserve credits on non-new subscriptions.
   Risk: changes externally visible `billing.status` → record a DEVIATION in `docs/PARITY.md` and
   wait for approval; define a grace window rather than hard cutoff to avoid abrupt lockouts.

3. **Idempotent payment write.** Change to `INSERT ... ON CONFLICT (id) DO NOTHING`; treat
   `rows_affected == 0` as success. Classify unique violations with `is_unique_violation` instead
   of collapsing them into `Unavailable`. Plumb a stable idempotency key from the approval boundary
   (approval/tool-call id) so retries reuse the id. Risk: needs a caller-side key that genuinely
   survives retries; changing error→success is a behavior change (DEVIATION/approval).

4. **Atomic cap enforcement.** Perform the gate check and insert in one transaction with
   `SELECT ... FOR UPDATE` on the space's limit row, or a single
   `INSERT ... SELECT ... WHERE (SELECT remaining) >= amount`. Risk: row lock serializes approvals
   per space (bounded contention); spans usecase+store, so keep the lock duration short and test
   the over-cap concurrent case.

5. **Currency-correct, index-friendly spend.** Pass half-open range timestamps
   (`created_at >= $start AND created_at < $end`) instead of a `to_char` prefix to use the existing
   index; validate `currency` in `record_payment` and filter spend by the cap's currency (or store
   per-currency caps). Risk: port signature change and mixed-currency semantics change (DEVIATION);
   verify with `EXPLAIN` on a seeded large table.

6. **Observability.** Add `tracing` spans/counters for duplicate-event skips, stale-event skips,
   cap rejections and zero-row payment inserts; never log card ids, provider refs or PII. Risk: low;
   ensure counters use low-cardinality labels (space id as a label would explode cardinality).

## Cross-cutting risks
- Items 1, 2, 3, 5 alter observable behavior or schemas → DEVIATION entries + owner approval before
  shipping; item 4 changes concurrency semantics. Sequence: migration → port update → store →
  usecase → tests, one commit each.
- Every write already runs in `with_tenant`'s transaction with transaction-local RLS settings; keep
  that invariant — do not open raw connections for the new tables.

## Test plan (REQ-tagged names)
- `req_051_replayed_webhook_is_idempotent`, `req_051_out_of_order_event_does_not_downgrade_state`,
  `req_051_canceled_subscription_loses_entitlement`, `req_051_expired_period_is_not_entitled`.
- `req_018_duplicate_payment_is_idempotent`, `req_018_concurrent_approvals_respect_cap`,
  `req_018_spend_is_currency_scoped`, `req_018_spent_in_month_uses_index` (EXPLAIN assertion).

## Open questions (GUESS)
- Whether the provider emits a stable `event.id` and an authoritative `created` timestamp for all
  relevant event types — assumed yes; confirm against the spec/provider docs before item 1.
- Grace window length for expired periods — assumed 0 pending a product decision (mark GUESS in
  `docs/STATE.md`).
