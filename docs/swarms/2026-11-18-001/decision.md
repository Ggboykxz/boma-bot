# Swarm 2026-11-18-001 — Queen decision

**Question:** which single UI slice should Boma Bot implement next?
A = contextual approval card (REQ-007/017), B = board/triage (REQ-036/038), C = locales (REQ-057).

## Workers and votes (weight)

| Worker | Weight | Choice | Confidence |
|---|---|---|---|
| accessibility | 1.3 | A | 0.85 |
| security | 1.3 | A | 0.85 |
| product | 1.5 | A | 0.66 |
| ux | 1.5 | A | 0.82 |
| performance | 1.0 | A | 0.80 |
| i18n | 1.2 | C | 0.90 |
| rust | 1.0 | B | 0.70 |

## Consensus

- **Majority: A** — 5 of 7 votes (> 50 % of cast votes).
- **Weighted: A** — 6.6 of 8.6 total weight (C 1.2, B 1.0).
- **Arbitration: not needed** — both rules agree; no tie.

## Queen's note on the dissent

The Rust worker voted B and objected that A implies a wire deviation (`Block::Card` carries only
`cardId`/`kind`). The Queen accepts the objection as a **scope guard**, not as a veto: the slice
actually dispatched is copy-and-presentation only — translated effect/reversal text, a translated
region name, an explicit status line and BoardUI buttons/input. **No wire field, migration or
golden data change is part of this decision.** Durable answered state / structured effect payloads
remain a separate proposal requiring an ADR and owner approval.

## Implemented first slice

`apps/web/components/thread/message-blocks.tsx` CardBlock, with two REQ-named Playwright tests
(`REQ-007`, `REQ-017`), three catalogues, semantic tokens only, no Pro component, no new transport.
