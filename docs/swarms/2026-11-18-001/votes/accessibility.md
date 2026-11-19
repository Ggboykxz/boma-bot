---
swarm: 2026-11-18-001
worker: accessibility
weight: 1.3
choice: A
confidence: 0.85
---

## Choice: A — contextual approval card upgrade (REQ-007/017)

### Rationale
The approval card is the only place a user commits an external effect: an inaccessible card is a high-consequence defect, not cosmetic. Clinical and eldercare proposals both call an ambiguous confirmation worse than a slow screen; 72/100 sectors name this slice and the synthesis ranks it first. The current `CardBlock` (`apps/web/components/thread/message-blocks.tsx`) has concrete debt: its region is named by a raw internal enum (`aria-label={block.kind}`), confirm/deny labels are generic, high-impact state is implied, and an empty text answer silently submits `"approve"`. Fixing one component yields the largest keyboard/screen-reader gain using installed BoardUI components and semantic tokens only: no new transport (`threadsAnswer` exists), no second design system, no upstream assets, dark mode via tokens.

### WCAG / RTL risks
- **4.1.2 / 2.4.6**: `aria-label={block.kind}` exposes an untranslated enum as the region name (also a REQ-057 breach). Give it a catalogued label.
- **2.5.3 Label in Name**: "Approve" omits the effect; voice/SR users cannot tell what they authorise. Name it ("Call the GP").
- **1.4.1 / 1.4.11**: high impact needs `border-border-error-default` + `text-text-error-primary` **plus** an `aria-hidden` icon and the literal "High impact" — never colour alone.
- **4.1.3 Status Messages**: one polite announcement for arrival/answer; the transcript stays non-live (§8.9).
- **2.4.3 / 2.4.7**: focus into the card on arrival, keep `focus-visible:ring-2 focus-visible:ring-border-focus-ring`, drop the empty-draft → `"approve"` default.
- **RTL / 1.4.12**: `ms`/`me`, `text-start`, `border-s`/`border-e`; `<bdi dir="auto">` for IDs, amounts, emails; mirror the check glyph and gate `animate-check-draw` behind `prefers-reduced-motion`; keep 44 px `.boma-target` and 360 px wrap.

### First REQ-named Playwright test
`REQ-007 the pending approval card names the exact effect, marks high impact in text and icon, and announces its state by keyboard in RTL` — seed the card; assert `getByRole("region", { name: <translated label> })`, an effect-specific confirm name, "High impact" text + icon with the error token, Tab/focus-visible on both actions, keyboard activation reaching the named "Approved" state, and zero critical/serious `AxeBuilder` `wcag22aa` findings in light, dark and `dir=rtl` (he). Extend `REQ-007-pending-card.json` as the golden.
