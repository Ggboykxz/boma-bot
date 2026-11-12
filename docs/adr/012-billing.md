# ADR-012 — Billing scope for v1: interfaces and plan limits, payment provider behind a port, deferred

Status: proposed (awaiting owner go) — this is DEC-09

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

Option 1. Money is integer minor units everywhere; credits are micros; every ledger table is
append-only with a balance projection. `SpendingLimit` is enforced before a paid tool call, and an
over-cap request produces the purchase card from REQ-018. The `PaymentProvider` port has `checkout`,
`subscription_status` and `verify_webhook`; the default implementation returns
`billing_unavailable`, which `/readyz` reports so the UI hides paid plans instead of failing at
checkout.

## Consequences

- No card data, no PCI surface, no live keys in the first release.
- The webhook endpoint still exists and is specified (512 KiB cap, signature verification,
  idempotent per event id) so turning on a provider is configuration plus an adapter.
- Credits can be granted manually (operator/admin procedure) for early users without pretending the
  billing loop is complete.
