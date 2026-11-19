---
swarm: 2026-11-18-001
worker: performance
weight: 1.0
choice: A
confidence: 0.8
---

## Choice: A — contextual approval card upgrade (REQ-007/017)

### Rationale (low-end device / bundle / poor network)
A is the only slice that is near-free at runtime yet improves the hottest path. It touches one reused component — `CardBlock` in `apps/web/components/thread/message-blocks.tsx` (9 KB) — adding a structured effect·reason·reversal summary, explicit status text and reduced-motion gating. No new transport (`threadsAnswer` already exists), no new dependency, no second design system: incremental JS is a few KB of JSX, so the ≤200 KB gzip initial-bundle and LCP/INP/CLS ≤2.5 s / ≤200 ms / ≤0.1 on throttled 4G budgets (docs/PLAN.md §4, §9) stay untouched. Reduced motion plus one polite status announcement also cut render churn on weak CPUs. It reaches 72/100 sectors and proposals 001, 089 and 093 each name it first.

### Concrete cost of each option
- **A (chosen):** one component, ~0 new KB in the initial bundle, ~0 new round trips. Budget risk negligible.
- **B (REQ-036/038):** `apps/web/components/board/board-screen.tsx` is 16 KB; saved filters add client state and likely a saved-filter RPC/payload. Next.js route-splitting spares the thread bundle, but it means extra list/filter traffic on a flag-gated (COULD) screen — poor payoff on slow links, and it still owes a scope decision.
- **C (REQ-057, issue #16):** CONFIRMED only 3 locales ship (`en`, `fr`, `he`; `SUPPORTED_LOCALES` in `apps/web/lib/i18n.ts`, ~1,100 lines already inlined). Shipping 14 means 11 more ~340-key catalogues; inlined naively that ~4×s catalogue data in the initial JS bundle, threatening the 200 KB gzip cap and LCP on throttled 4G for every user — including single-locale ones. The count assertion itself is cheap, but the 14 locales are only honest with real translations plus per-locale dynamic import/code-splitting — real work best done after budgets are measured.

### First budget check / REQ-named test
First gate: bundle report — initial JS ≤200 KB gzip, LCP ≤2.5 s / INP ≤200 ms / CLS ≤0.1 on throttled 4G (must remain green). First test: `REQ-007 the pending approval card names its exact effect, marks impact in text and icon, and announces its state by keyboard in RTL`, extending `spec/golden/ui/REQ-007-pending-card.json`.
