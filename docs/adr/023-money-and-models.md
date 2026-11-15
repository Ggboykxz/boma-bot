# ADR-023 — Money and models: catalogue from the deployment declaration, credits as seeded balance, Stripe behind a port

Status: accepted (2026-11-15, milestone M8 — REQ-018, REQ-051, REQ-052).

## Context

The spec asks for three things (spec/01-requirements.md § Billing and models):

- **REQ-018** — a space can cap model spending; the bot can ask to buy something, and the purchase
  is an approval card with the payment recorded only after a person approves.
- **REQ-051** — paid plans through a payment provider: checkout, subscription state, credits and
  plan limits that gate features.
- **REQ-052** — a model catalogue per provider; a credential validated when it is connected; the
  rejection carrying the *provider's* reason; OAuth logins for providers that offer them; credits
  per account.

The owner's scope decision stands: money becomes real in M8 (REQ-018, REQ-051) and BYOK is the
credential story (DEC-06 timeline, `spec/06-decisions-needed.md`).

## Decision

1. **The catalogue is derived, never authored.** It lists exactly the catalogue keys the deployment
   declares in `DeploymentModels::reachable`, in declaration order, with display metadata from a
   small built-in table of known providers. An unknown key is listed with its raw id, a
   documented default window (128 000 tokens) and zero prices — it is never dropped, because a
   deployment may name its own provider or a local model. The indicative price table is for
   month-to-date spend accounting at the spending limit (REQ-018), not for invoicing; no number
   derived from it is ever tax, FX or debt. `spec/golden/models-catalog.json` stays `pending`:
   the capture is synthetic (`provider_golden`/`model_golden`). The golden contract is pinned by
   domain and RPC tests instead.
2. **BYOK credentials are validated live, then sealed.** `models.connect` calls the provider's
   models endpoint; a refusal answers `400` with the provider's own reason (bounded to 200
   characters, never wrapped in an internal message). The key is sealed under the space data key
   (`space_keys`, ADR-020 machinery) and is never listed again — reads expose the provider, the
   `connected` state and when it was stored, nothing else. Unknown providers are refused without a
   network call. `models.disconnect` forgets one row and is a success when absent.
3. **The person's model preference is per account, not per space.** `models.preferences.get|set`
   reads and writes `user_model_preferences` (`user_id` key); it is applied to runs the person
   starts but it is not a pin (`model_pinned` stays false). An unreachable key is refused visibly,
   never silently downgraded. Clearing returns to the deployment default. The preference is
   exempted from the pin rules of REQ-015; a pinned model the deployment cannot reach is still a
   visible failure.
4. **What the chat run actually executes stays the deployment default.** There is no per-run model
   metering: the seeded credits are visible balance, not a consumed budget. Usage accounting, a
   durable ledger, FX and tax are out of scope (see "Named gaps").
5. **Stripe is the wired provider** behind the `BillingProvider` port (`BOMA_BILLING_PROVIDER=stripe`,
   `BOMA_BILLING_STRIPE_SECRET`, `BOMA_BILLING_STRIPE_PRICE_PRO`). The port is exercised only by
   routes that exist; otherwise the whole billing surface — the `/rpc/billing/*` namespace and
   `POST /api/v1/billing/webhook` — is uniformly absent (404), signed in or not.
   `billing.checkout` opens a Checkout session for the `pro` plan only; the success/cancel URLs
   point at `{BOMA_PUBLIC_BASE_URL}/settings/billing`.
6. **The webhook proves itself by HMAC-SHA256** over `t.body` (`t=,v1=` header) with a ±300 s clock
   window, mirroring Stripe's signature and the REQ-050 pattern. Every failure answers the same
   uniform `404`; unknown events are acknowledged and ignored (`customer.created` et al.). The
   `checkout.session.completed` event activates the subscription: plan `pro`, `active`,
   `provider_ref` from the session, and a seeded credit balance (`PRO_SEEDED_CREDITS_MINOR =
   25_000`, illustrative $250; configuring amounts is a deployment decision left to the operator).
7. **Purchases are a card, checked twice.** `purchase.request` is a `Spend`-class builtin tool.
   Dispatch applies `purchase_gate` in the model's own turn: free plan on a billing deployment
   → refusal with `upgradeRequired: true`; remaining month room under the cap → refusal with
   `limitReached: true`. Approval re-checks the same gate (time and balance may have moved) before
   `purchase_payments` is written. Rejection records nothing. The month room is
   `space_spending_limits` minus `purchase_payments` this calendar month (`YYYY-MM` on the
   deployment clock). `billing.limit.get|set` round-trip it; `set` normalises the ISO currency.
8. **Plan limits gate purchases, for now.** Nothing beyond the purchase path is plan-gated; there
   is no quota of computers, artifacts or memory by plan. A deployment without
   `BOMA_BILLING=true` ships the free behaviour exactly as before M8.
9. **Tenancy is proven, not asserted.** `model_credentials`, `space_spending_limits`,
   `purchase_payments`, `subscriptions` carry RLS in migrations 0043/0044; the unprivileged probe
   role test shows the other space cannot see or forget a credential, cannot read the room or the
   spend, and does not inherit a pro subscription.

## Alternatives considered

1. **Ship a no-op `BillingProvider` and declare billing done.** Rejected: REQ-051 says paid plans
   go through a payment provider; an adapter that cannot open a checkout is a TODO wearing a port.
   Stripe's Checkout (hosted, no card data, no PCI surface) is the smallest correct thing.
2. **Meter every chat token against credits now.** Rejected: the reference meters weekly calls, and
   pricing a token from an indicative table into an invoice we invent would be fiction. The spend
   limit covers the only thing the spec makes a MUST (approval-gated purchases). This is a gap.
3. **A single ledger table for credits with signed rows.** Rejected for now; `purchase_payments` is
   append-only and `subscriptions.credits_minor_units` is a mutable projection, documented here
   as the deviation from ADR-012's first draft.

## Deviations from ADR-012 (billing) and ADR-008 (secrets)

- ADR-012 named a `PaymentProvider` with `subscription_status` and `verify_webhook`; the code has a
  `BillingProvider` (`create_checkout`) behind it and webhook verification in the use cases, since
  the HMAC path carries no provider state at all.
- ADR-012's append-only ledger with a balance projection is `purchase_payments` (append-only) plus
  `subscriptions.credits_minor_units` (projection). No token-usage ledger exists.
- ADR-012's 512 KiB webhook cap and event-id idempotency are 64 KiB and refresh-by-event; the
  known-events path is safe to reapply because activation rewrites the same row.
- Manual credit grants (operator procedure) are not implemented; the only writer left after M8 is
  the verified checkout webhook.

## Named gaps (deferred, and where)

- **OAuth logins for model providers (REQ-052, third bullet).** The M5 OAuth machinery (PKCE,
  single-use state, `oauth.v2`) exists for connectors and could carry a provider that offers
  OAuth; none of the built-in catalogue providers do. Not wired to `models.connect` this milestone.
- **Account-level credit metering (REQ-052, fourth bullet).** Runs do not debit credits; the pro
  balance does not decrease. Credits consumed by chat usage need a usage event on every run with
  a price table — M11 hardening candidate, or a proposal on its own.
- **Feature gating by plan beyond purchases** — no quotas, no gating of computers/artifacts/pages.
- **Reconciliation, refunds, seat management, proration, FX, tax** — all out of the SHOULD scope
  as written ("paid plans go through a payment provider with checkout, subscription state, credits
  and plan limits").
- **A durable webhook event sink.** Unknown event types are ignored; a verified replay of a known
  type re-applies the same state, idempotently by construction.

## Consequences

- A self-hosted operator who never enables billing sees no money surface anywhere (404 on routes,
  namespaces and webhook), and the free plan behaves exactly as it did at M7.
- An operator who enables billing must set all four Stripe settings together; half-configuration is
  a startup error (`ConfigError::Billing`), not a deployment that refuses checkouts silently.
- No card data enters this process; Stripe hosts the page. There is nothing to PCI-scope here
  except the webhook secret and the API key at rest in the operator's own secret store.
- The `models-catalog` golden remains `pending` with a reason recorded in `golden-status.json`;
  the rest of the RPC surface is unchanged for pre-existing goldens.

## Evidence (rebuild)

- `crates/domain/src/catalog.rs` (catalogue, provider name table, endpoint shapes), `crates/domain/src/billing.rs` (signature parse/verify).
- `migrations/0043_models.sql` (models, preferences; RLS), `migrations/0044_money.sql` (`space_spending_limits`,
  `purchase_payments`, `subscriptions`; RLS).
- `crates/app/src/usecases.rs` (`models_catalog|credentials|connect|disconnect|preference|set_preference`,
  `purchase_parts|purchase_gate|dispatch_spend_tool|approved_purchase_request`,
  `spending_limit_get|set`, `billing_status|checkout|webhook`),
  `crates/worker.rs` (tool-call turn parks Spend-class calls on approval cards).
- `crates/api/src/rpc.rs` (`models.catalog|credentials|connect|disconnect|preferences.get|set`,
  `billing.status|checkout|limit.get|limit.set`), `crates/api/src/public.rs`, `crates/api/src/router.rs`.
- Tests: `crates/domain/src/{model,catalog,billing}.rs::req_052_*`, `crates/testkit/src/build.rs::{models_tests,billing_tests}` (20 tests),
  `crates/api/tests/rpc_surface.rs::req_018_and_051_*` and `::req_052_*` (6 tests),
  `crates/infra/tests/m1.rs::req_018_051_052_credentials_limits_payments_and_subscriptions_stay_in_their_space` (probe role, sealed-at-rest row check, cross-space denial).
