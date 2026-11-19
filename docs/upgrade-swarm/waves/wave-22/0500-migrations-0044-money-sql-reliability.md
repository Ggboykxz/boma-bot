# v0.4.0 reliability proposal — `migrations/0044_money.sql` (worker #500, lens: reliability)

## Scope and current design (CONFIRMED — read in this session)

Three RLS-protected tables form the money spine: `space_spending_limits` (one row per space:
`enabled`, `currency text` with only `char_length = 3`, `cap_minor_units bigint >= 0`, `updated_at`),
`purchase_payments` (`bigint >= 0` amount, free `currency text`, nullable `card_id` with
`ON DELETE SET NULL`, `created_at`), and `subscriptions` (one row per space, `plan`/`status` enums,
nullable `provider_ref`, `credits_minor_units bigint DEFAULT 0`, timestamps). All three use
`ENABLE` + `FORCE` RLS with one policy gated on
`space_id = boma.current_space_id() OR boma.is_system()` for both `USING` and `WITH CHECK`.
From `0002_m1.sql`: `current_space_id()` reads `current_setting('boma.space_id', true)` (NULL when
unset) and `is_system()` is a non-empty `boma.system_reason`. `spec/02-data-model.md` requires money
as integer minor units **or micros**; `REQ-017` requires payment effects to be exactly-once;
`REQ-018` gates paid requests on an approved card; `REQ-051` makes the subscription a webhook effect.
All measures below are **additive** (ADD COLUMN/CONSTRAINT, CREATE INDEX/VIEW/TRIGGER, COMMENT);
the two items that cannot be made additive are explicitly deferred with an ADR note.

## Failure modes

### 1. Numeric precision (CONFIRMED schema; severity: medium–high)

- **No exponent/scale is recorded.** All three amounts are bare `bigint` "minor units", silently
  assuming two decimals. JPY has 0 decimals, KWD/BHD have 3, and `spec/02-data-model.md` also allows
  **micros** (1e-6). The same integer means different money depending on currency; any aggregation
  that mixes currencies, or any app that divides by 100 unconditionally, is off by 10x–100000x.
- **No upper bounds.** `bigint` max is ~9.22e18 minor units (~$92 quadrillion at 2 decimals), so
  column overflow is unlikely, but corrupt/negative inputs are accepted: `credits_minor_units` has
  **no CHECK at all** (negative credits, absurd values), and `cap_minor_units`/`amount_minor_units`
  only check `>= 0`. A webhook replay or a bad cast can silently reset or invert a balance.
- **Mutable scalar, no ledger.** The data model specifies append-only credit entries; here credits
  are a single mutable number with no audit trail. A wrong write is unrecoverable and undetectable.
- **Coercion at the Rust edge.** Negative/oversized values can wrap on `i64`/`u64` conversion in the
  spending gate; the DB offers no defence-in-depth and the newtype must be validated at the boundary.

### 2. Currency (CONFIRMED schema; severity: high)

- **Currency is not an ISO code.** Only `char_length = 3` is checked: `usd`, `XXX`, `ABC`, `''`
  (empty is length 0, rejected) — but lowercase and non-ISO codes pass. No uppercase/allow-list.
- **Cap/payment currencies can disagree.** `space_spending_limits.currency` and
  `purchase_payments.currency` are independent free columns. A gate that sums spend by `space_id`
  alone counts a EUR payment against a USD cap and refuses/permits the wrong purchases (REQ-018).
- **No currency on `subscriptions`.** Credits and `provider_ref` carry no currency, so a provider
  switching settlement currency leaves prior credits ambiguous; nothing ties credits to the plan's
  currency.

### 3. RLS / tenant integrity (CONFIRMED policy; severity: high)

- **Fail-closed is good (positive control):** unset `boma.space_id` makes the predicate NULL, so a
  member session sees no rows. Preserve this in any rewrite.
- **`is_system()` is not a security boundary.** PostgreSQL custom settings (`boma.system_reason`)
  are settable by any session role, so any DB user can self-declare "system" and read/write every
  money row across tenants — including caps and subscriptions (D-09 at the DB layer). For money
  this is the highest-severity finding. Additive SQL alone **cannot** close the SELECT-side bypass.
- **Cross-tenant actor/card coherence.** Nothing ties `purchase_payments.user_id` or `card_id` to
  the row's `space_id`; a system-path write can attribute another tenant's user/card to a payment.
- **Financial history is cascade-erased.** `user_id`/`space_id` are `ON DELETE CASCADE` and
  `card_id` is `SET NULL`: deleting a user, space, or approval card silently destroys or orphans the
  payment record, breaking audit/retention. Approval provenance (`run_id`/`approval_id`) is not stored.
- **Pooled-connection residue.** If `boma.space_id` is not reset (ideally `SET LOCAL` per
  transaction), a pooled session serves the previous tenant and RLS happily returns its money rows.

### 4. Idempotency, timestamps (CONFIRMED schema; severity: medium)

- **`provider_ref` is not unique.** Webhook replay (REQ-051) can create/overwrite ambiguous state;
  only the per-space PK happens to dedupe subscriptions. There is no explicit idempotency key.
- **`updated_at` is insert-only.** `space_spending_limits` and `subscriptions` set it via DEFAULT
  but no trigger refreshes it on UPDATE, so stale timestamps mislead reconciliation and audits.

## Concrete additive hardening (with risks)

1. `ADD COLUMN currency_scale smallint` (or a `boma.currency(code text PK, exponent smallint)` table
   + view) on the three tables; `CHECK (currency_scale BETWEEN 0 AND 6)`. *Risk:* GUESS which
   currencies/exponents are supported; default 2 preserves current behavior and avoids rewriting
   amounts. Exposes the exponent to the Rust money newtype.
2. `ADD CONSTRAINT ..._currency_iso CHECK (currency ~ '^[A-Z]{3}$')` on both currency columns.
   *Risk:* rejects lowercase/non-ISO historical rows; use `NOT VALID` + cleanup + `VALIDATE` if any
   exist (GUESS — no money rows in the goldens).
3. `ADD CONSTRAINT credits_nonneg CHECK (credits_minor_units >= 0)` and generous ceilings on cap /
   amount / credits. *Risk:* a hard cap drifts from config and rejects legitimate large spaces;
   keep the ceiling well above any real balance and document it.
4. `ADD COLUMN credits_audit` append-only ledger table mirroring `ModelCreditEntry` plus a trigger
   writing a row on every credits change. *Risk:* trigger must be `SECURITY DEFINER` with fixed
   `search_path`, idempotent, and must not fire on system-path repairs without recording them.
5. `CREATE UNIQUE INDEX subscriptions_provider_ref_key ON subscriptions (provider_ref)
   WHERE provider_ref IS NOT NULL;` plus an explicit `idempotency_key` column on `purchase_payments`.
   *Risk:* a provider may reuse/rotate refs across plan changes — dedupe before creating a UNIQUE
   index (unique constraints cannot be `NOT VALID`); GUESS provider semantics from the webhook code.
6. `ADD CONSTRAINT purchase_payments_space_user CHECK` cannot validate membership across tables;
   instead add composite FKs via an additive unique index on `users(space_id, id)` /
   `cards(space_id, id)` and `FOREIGN KEY (space_id, user_id)`, `(space_id, card_id)`.
   *Risk:* requires those unique keys first (separate migration); `card_id` nullable so the FK must
   allow NULL; legacy cross-tenant rows block validation.
7. Additive `AFTER DELETE` archive trigger on `purchase_payments` writing a `payment_archive` row
   before cascade erasure. *Risk:* trigger ordering under `ON DELETE CASCADE` and RLS must be tested;
   cannot change the FK action without a non-additive migration (deferred with an ADR).
8. `CREATE INDEX purchase_payments_card ON purchase_payments (card_id)` for approval→payment joins.
   Zero functional risk; removes a likely N+1 on the approval path.
9. Additive `BEFORE UPDATE` trigger setting `updated_at = now()` on both timestamped tables;
   `COMMENT ON` every new object with its REQ/D-xx rationale.
10. **Deferred (non-additive, ADR required):** replace the `is_system()` arm with a
    `SECURITY DEFINER` allow-list verifier and restrict `SET` on `boma.system_reason` to a dedicated
    role; rewrite the SELECT-side policy. Additive reason allow-list CHECKs on writes only raise the
    bar and are worth adding, but do **not** claim they close the bypass.

## Verification gate (v0.4.0)

- Migration applies to a seeded DB; fixtures with lowercase/invalid currency, negative credits, an
  out-of-range exponent, a duplicate `provider_ref`, and a cross-tenant `(space_id, user_id)` are
  rejected (constraints `VALIDATE`d after cleanup where `NOT VALID`).
- Test: member session with no `boma.space_id` still sees zero rows (fail-closed preserved).
- Test: a session that sets `boma.system_reason` cannot read another space's money rows after the
  deferred policy rewrite (recorded as a known gap until then).
- Test: updating a spending limit/subscription advances `updated_at`; deleting a user emits exactly
  one archive row; a webhook replay does not double-apply credits.

*CONFIRMED facts were verified by reading `0044_money.sql`, `0002_m1.sql`, `0004_cards.sql`,
`spec/01-requirements.md`, `spec/02-data-model.md` and `spec/05-known-defects.md` this session;
production shapes, supported currencies/exponents, and provider `ref` lifecycle are GUESS. No source files were modified.*
