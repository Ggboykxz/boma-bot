# ADR-019 — Test strategy and gates

Status: proposed (awaiting owner go)

## Context

The quality bar requires: every MUST tested, every REQ id in a test declaration, all goldens replayed,
an OpenAPI contract diff, ≥80% line coverage (90% on domain), property tests on critical invariants,
and Playwright + axe for the UI. Spec 4.2 makes concurrency correctness (leases, fencing, idempotency)
a first-class requirement, which unit tests cannot prove.

## Alternatives

1. **Pyramid with real dependencies: unit tests in `domain`, integration tests against a real Postgres
   via testcontainers, property tests for invariants, golden replay, contract diff, Playwright + axe
   for the UI.**
2. Mock-heavy unit tests only (fast, proves little about leases and RLS).
3. End-to-end only (slow, flaky, poor failure localisation).
4. Differential testing against the running original as the primary gate (unavailable in this session,
   and it would bless the reference's defects).

## Scoring

| Criterion | 1 pyramid | 2 mocks | 3 e2e only | 4 differential |
|---|---|---|---|---|
| 1 fits the spec (concurrency, tenancy) | 1 | 4 | 2 | 2 |
| 5 simplicity | 2 | 1 | 1 | 4 |
| reliability of the gate | 1 | 4 | 3 | 3 |

## Decision

Option 1.

- **Unit**: pure `domain` (state machine, money, cron parsing, block validation). Fast, no IO.
- **Property**: transition table totality (no unknown transition is accepted), seq monotonicity, money
  arithmetic never overflows, secret round-trip with AAD binding, cron `@once` fires once.
- **Integration**: real Postgres in `tests/`; two concurrent workers racing a run (fence test), two
  spaces (RLS test), replay of `clientNonce` (idempotency), queue coalescing.
- **Golden replay**: each `spec/golden/*.json` maps to a procedure name in its `request` and is
  replayed as a contract test; a golden without a handler fails the build.
- **Contract**: generated OpenAPI vs `spec/openapi.yaml`, any difference a DEVIATION.
- **UI**: Playwright specs named with the REQ id, plus replay of every `spec/golden/ui/*.json` as a
  seeded state, plus axe with zero critical/serious findings.
- **Traces**: REQ ids appear in test names only (`req_012_…`), enforced by `tools/trace_check.py`.

## Consequences

- Integration tests need Docker in CI; locally, `make test` uses a Postgres container and skips
  gracefully with a clear message when Docker is absent (skip is recorded, never silent).
- Coverage floors are enforced per crate; a new crate without tests lowers the number and fails.
- Differential tests against the original remain desirable but optional (the original needs a running
  stack); where they run, they compare behaviour and never bless a known defect.
