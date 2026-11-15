# ADR-012 — Billing scope for v1: spending limits native, payment provider behind a port

Status: accepted (2026-11-15 — REQ-018/051/052 milestone M8 opened with the owner's go). The shape
is settled in ADR-023; this records the scope choice.

## Context

The spec has billing, credits, plan limits, spending limits and purchase approvals (REQ-018, REQ-051).
REQ-018 (spending caps and approval-gated purchases) is a MUST because unattended agents must not
spend freely; the payment provider integration itself is SHOULD. There is no customer yet.

## Alternatives

1. **Implement spending limits, credits and plan limits natively; define a `PaymentProvider` port and
   ship a no-op implementation plus a documented webhook contract; wire a real provider in a later
   milestone.**
2. Implement the full payment provider now.
3. Skip billing entirely in v1.
4. Use a third-party subscription platform for everything.

## Scoring

| Criterion | 1 limits + port | 2 full provider | 3 skip | 4 platform |
|---|---|---|---|---|
| 1 fits the spec | 1 (MUST done, SHOULD deferred and visible) | 1 | 4 (REQ-051 SHOULD unmet without a note) | 2 |
| 2 maturity | 1 | 1 | 1 | 1 |
| 3 security | 1 (no card data ever) | 2 (live keys early) | 1 | 2 |
| 4 performance | 1 | 1 | 1 | 1 |
| 5 simplicity | 1 | 4 (webhooks, reconciliation, tax) | 1 | 3 |
| 6 typed contract | 1 | 1 | 1 | 2 |

## Decision

Option 1. Money is integer minor units everywhere; credits are an amount in minor units.
`SpendingLimit` is enforced before a paid purchase call, and an over-limit request produces the
REQ-018 card from the model's own turn; the cap is enforced a second time at approval. The
`BillingProvider` port has `create_checkout` behind it; a deployment that does not configure one
reports no billing capability, the whole billing surface is 404 and the product runs fully on the
free plan. As built in ADR-023: Stripe is the wired provider (`BOMA_BILLING_PROVIDER=stripe`),
the webhook proves itself by HMAC-SHA256, and credits seed on verified checkout. The `PaymentProvider`
name in this ADR is the same port, renamed when it was written down in code.

## Consequences

- No card data, no PCI surface and no TLS peer beyond the deployment's own provider endpoint in
  the default build.
- The webhook endpoint exists and is specified exactly once: `POST /api/v1/billing/webhook`,
  64 KiB cap, HMAC-SHA256 signature verification, ±300 s clock window, uniform 404 on every
  failure (ADR-023). Unknown events are acknowledged and ignored.
- Credits are visibly seeded by a verified checkout (`PRO_SEEDED_CREDITS_MINOR`, illustrative) so
  the paid plan's value is observable; consumed usage and a ledger are a proposal, not this ADR.
- Several items in this ADR's Decision are deliberately narrowed in ADR-023 (the "append-only
  ledger" is `purchase_payments` + the mutable credit balance; feature gating lands only for
  purchases; `/readyz` reports the billing capability only when configured).
