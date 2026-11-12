# ADR-002 — Tenancy: PostgreSQL RLS plus a repository boundary

Status: proposed (awaiting owner go) — this is DEC-05 in `spec/06-decisions-needed.md`

## Context

Every bot, thread, run, artifact and routine belongs to exactly one space. REQ-043 requires that a
request for another space's resource returns not-found rather than data, and that system paths run
with a recorded reason. The reference enforces this in the database (26 migrations install RLS, every
query runs inside a tenant context) with an explicit `runAsSystem(reason)` escape hatch, and
`05-known-defects.md` records D-09: that escape hatch has no static gate. One missed filter in a
service-layer design is a cross-tenant leak with no failing test.

## Alternatives

1. **RLS on every tenant table + a repository API that always takes a `Tenant`/`System(reason)`
   context.** Two independent barriers.
2. Service-layer scoping only: every query takes a `space_id` filter, reviewed by convention.
3. A database-per-tenant model.
4. RLS only, with handlers issuing raw SQL.

## Scoring

| Criterion | 1 RLS+repo | 2 service-only | 3 DB per tenant | 4 RLS only |
|---|---|---|---|---|
| 1 fits the spec | 1 | 3 | 4 (unbounded provisioning) | 2 |
| 2 maturity | 1 | 1 | 2 | 1 |
| 3 security | 1 | 5 | 2 | 3 |
| 4 performance | 1 | 1 | 2 | 1 |
| 5 simplicity | 3 | 1 | 5 | 2 |
| 6 typed contract | 2 | 2 | 2 | 5 |
| 7 licence | 1 | 1 | 1 | 1 |

## Decision

Option 1. Concretely:

- Every tenant table carries `space_id` and an RLS policy comparing it to
  `current_setting('boma.space_id')`; the connection-pool checkout sets that value per transaction
  from the request context. No handler opens its own connection.
- The infra crate exposes repositories whose constructors require `TenantContext`, an enum of
  `Tenant(space_id, user_id)` and `System(SystemReason)`, where `SystemReason` is a closed enum, not
  a string. A new system path is a code change that fails to compile until a reason exists.
- A property/integration test seeds two spaces and asserts that every repository rejects the other
  space's ids with not-found.
- A CI check greps for `SET LOCAL boma.space_id` outside the pool module and for `SystemReason::`
  outside the allow-list.

## Consequences

- A system path is cheaper to audit: `rg 'SystemReason::'` lists every tenant bypass in the codebase.
- Migrations must add a policy with every new tenant table; a test enumerates tables with a
  `space_id` column and asserts each has a policy, so forgetting one fails CI.
- Debugging is slightly harder (a query without a tenant context errors instead of returning rows) —
  which is the intended failure mode.
- D-09 is closed by construction rather than by review.
