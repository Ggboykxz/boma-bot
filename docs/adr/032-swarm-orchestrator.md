# ADR-032 — Queen/worker swarm orchestration with deterministic consensus

## Context

The owner wants GrokBot-class orchestration in an open-source, self-hosted Boma Bot: a Queen agent
coordinates a variable number of workers, and a consensus rule decides between their results.
Workers must be provider-agnostic — internal providers or local ACP executors (OpenCode, Claude
Code, later rows) — with no hosted service. Existing group delegation (REQ-010) is model-mediated
and one-hop; it cannot express a tally or an auditable decision.

## Alternatives and scoring (1–5)

| Criterion | 1. Rust-native + deterministic consensus | 2. Prompt-only in group rooms | 3. External CLI tool |
|---|---|---|---|
| Fits spec/charter (once REQ-076/077 land) | 5 | 3 | 2 |
| Maturity of chosen building blocks (runs, events, ACP already shipped) | 5 | 4 | 2 |
| Security / auditability | 5 (every vote stored) | 2 (transcript only) | 3 |
| Low-end performance / poor networks | 4 | 4 | 3 |
| Simplicity for one maintainer | 4 (new tables + RPCs) | 5 | 2 |
| Type-safe contract | 5 | 2 | 2 |
| Licence compatibility | 5 | 5 | 5 |
| Accessibility/i18n of the resulting UI | 4 | 3 | 1 |

## Decision

Adopt (1). Consensus is computed in Rust from stored votes — majority, weighted vote, arbitration
on tie — never by asking a model to declare itself the winner. Additive migrations only; the Queen
and workers reuse the existing run lifecycle and ACP provider port.

## Consequences

- Two new REQ ids in `spec/01-requirements.md` with provenance in `spec/EXPORT.md`; trace budget
  adjusted explicitly and documented until their tests land.
- New tables (`swarms`, `swarm_members`, `swarm_votes`) and RPCs (`swarms.create/vote/decide/get`);
  BoardUI swarm panel; `REQ-076`/`REQ-077` tests at unit, PG and Playwright levels.
- A tie can never be broken silently: arbitration requires a stored rationale visible in the room.
- Provider matrix beyond `opencode` remains issue #18; the swarm works with whatever the build
  already knows, and new executors land one wire-tested row at a time.
