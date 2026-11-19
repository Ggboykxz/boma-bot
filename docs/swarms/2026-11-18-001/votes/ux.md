---
swarm: 2026-11-18-001
worker: ux
weight: 1.5
choice: A
confidence: 0.82
---

## Choice: A — contextual approval card upgrade (REQ-007/017)

**Rationale (UI/craft).** SYNTHESIS ranks this slice first: 72/100 sectors name a decision card as their highest-impact first slice (REQ-007 338 mentions, REQ-017 187). The three sector proposals I read agree independently: 001 (patient-safety referral), 089 (care action) and 068 (purchase order) each pick the same smallest high-value change — make the consequence dominant, label confirm with the exact effect, never confirm on colour alone. B (board/triage, 71 sectors) is flag-gated and still owes a saved-filter scope decision; its screen uses `Button` but hand-rolls a modal with a raw `bg-black/40` literal and backlog/todo vocabulary rather than the run vocabulary. C is a compliance count assertion (3→14 catalogues): necessary, but almost no craft surface, and "14 locales ship" is not honest without 11 real translations.

The craft debt in `CardBlock` (`message-blocks.tsx`) is concrete: the only place a user commits an external effect shows a generic explain line and generic "Approve"/"Deny", no effect · reason · reversal summary, `aria-label={block.kind}` leaking a raw enum, and hand-written `<button>`/`<input>`/`<svg>` where BoardUI `Button`/`Input` are installed. An empty draft silently submits `"approve"` — an invisible approval. One generic card answers 72 sectors; no new transport (`threadsAnswer` + CONFLICT guard), no Pro, no second design system, no token re-tune. Reduced motion is already guarded at `globals.css:593` — keep it, do not re-solve it.

**BoardUI component/token plan.** Keep the `<section role="region">` shell with `bg-background-secondary-default`, `border-separator-border`, `rounded-3xl`, `gap-2`, merged with `cx()`. Replace raw controls with `Button` (confirm `primary`, deny `secondary`/`ghost`, `danger` for high-impact purchases) carrying translated effect labels ("Approve referral", not "Approve"); `Input` + `Label` for the draft, submit disabled when empty. `Chip` for Pending/Approved/Denied status and `Tooltip` for reason/reversal detail. High impact = `border-border-error-default` + `text-text-error-primary` + `RiAlertLine` (`aria-hidden`) + literal "High impact" — never colour alone. Type: `text-title-3-semibold` effect line, `text-body-regular` reason, `text-body-2-regular` reversal. `ms`/`me`, `text-start`, `<bdi dir="auto">` for ids/amounts; keep `.boma-target` and `focus-visible:ring-border-focus-ring`. All new strings via `t()` (en/fr/he source keys), no palette literals.

**First REQ-named Playwright test.** `test("REQ-007 the pending card states effect, reason and reversal, names the exact effect on confirm, marks impact in text and icon, and is keyboard-operable in RTL", …)` in `thread-ui.spec.ts`: seed and extend `REQ-007-pending-card.json`, assert a translated named region, effect-specific button name, impact text+icon, Tab/focus-visible, activation → "Approved" state, and axe `wcag22aa` clean in light, dark and `dir=rtl`.
