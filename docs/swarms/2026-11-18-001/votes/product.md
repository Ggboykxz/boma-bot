---
swarm: 2026-11-18-001
worker: product
weight: 1.5
choice: A
confidence: 0.66
---

# Vote: A — contextual approval card upgrade (REQ-007/017)

## Owner / product-value rationale

The approval moment is where the product's value is won or lost: it is the single highest-stakes interaction in Boma Bot. On the wrong click, a student-facing reply reaches a child (017), a load shed hits the wrong feeder (036), or untrusted plugin code is approved (057). Every sector specialist independently named a decision surface as their best first slice — including the board-sector proposal (036) and the devops proposal (057, which chose plugin *review*, not locales). That is an unusually strong, convergent product signal.

The cross-sector data backs it: SYNTHESIS counts the approval/decision card as the highest-impact first slice in **72 of 100 sectors** (REQ-007 338 mentions, REQ-017 187), versus 71 for board/triage and REQ-057 as a cross-cutting *gate*, not a sector ask. One generic contextual card — effect · reason · reversal, explicit status text, text+icon impact marker, keyboard path, RTL-safe, reduced-motion — answers 72 sectors with a single reusable component instead of 72 variants. For a single maintainer (AGENTS criterion #5), that maximum reuse with minimum future surface is the best possible product outcome. It is also the owner's own #1 recommendation in SYNTHESIS.

## Fastest ship with the widest impact

The foundation is already shipped in M34: `CardBlock` in `message-blocks.tsx`, `threadsAnswer`, the `REQ-007-pending-card.json` golden, focus/keyboard handling, the `.animate-check-draw` confirmation, and installed BoardUI `Button`/`Badge`/`Tooltip`. The remaining delta is the structured summary (effect/reason/reversal) plus explicit status text and a golden extension — **no new transport, no new component, no second design system, no token re-tune**. That makes A fast relative to its reach. By contrast, B (REQ-036/038) is flag-gated and raises a scope decision plus a saved-filter transport question; C (REQ-057, issue #16) is fastest in isolation but narrow — a 3→14 catalogue with a count assertion, and "14 locales ship" is only honest if real translations exist, so it is compliance, not the widest product value. Charter is satisfied: our own card, BoardUI semantic tokens only, no upstream brand or assets.

## First REQ-named test

Test-first, before any code: `test("REQ-007 pending card names the exact effect, reason and reversal, marks impact by text+icon, and takes focus without blocking reading", …)` and extend `spec/golden/ui/REQ-007-pending-card.json` first. The wire-shape/new-fields deviation needs the short ADR SYNTHESIS already flags; record it before implementation.
