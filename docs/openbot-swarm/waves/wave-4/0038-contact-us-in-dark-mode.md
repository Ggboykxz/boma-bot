# 0038 — "contact us in" through the dark-mode lens (Boma Bot parity proposal)

- Brief: id 38, area `contact-us-in`, lens `dark-mode`, slug `0038-contact-us-in-dark-mode`.
- Status: **proposal**. Upstream read as **data only** (`/tmp/opencode/openbot-parity`, HEAD
  `559d5ac`): `packages/ui/src/features/billing/LicenseSection.tsx`, `packages/ui/src/features/servers/AddServerDialog.tsx`,
  `HostedPricingPage.tsx`, `src/renderer/src/WorkspaceOverlays.tsx`, `SettingsModal.tsx`,
  `src/main/ipc/app-handlers.ts` + `app-inputs.ts`, `packages/contracts/src/hosted-servers.ts` +
  `feedback.ts`, `packages/i18n/src/messages/en/{server,billing,sitePage}.ts`. Product code read-only; this
  file is the only artefact written. No upstream code, asset, wording or brand enters Boma Bot; no Pro
  component. English (AGENTS.md disk convention).
- **CONFIRMED** = read from a local source this session; **GUESS** = recommendation for the owner.

## 1. The upstream capability, in my own words (inspiration only)

Upstream offers a **"Contact us" outbound affordance exactly when the self-serve path cannot
finish**: the license purchase dialog swaps its primary action to this contact action when the seat
count is already at the plan ceiling, and the same action appears in the hosted-server add flow, the
hosted-plans page and the site footer (CONFIRMED). It is **not an in-app form that silently sends**: it
opens a prefilled external channel (a mail draft with a fixed subject and a body template inviting
"additional context"), routed through a main-process allowlist of named external destinations, so the
user always presses send themselves (CONFIRMED, `app-handlers.ts`, `app-inputs.ts`, `hosted-servers.ts`).
A sibling "report a problem" path builds a prefilled bug-report URL from runtime facts (platform,
version, os, provider) instead of free text (CONFIRMED, `feedback.ts`).

Design properties worth restating: the offer is **conditional** (only where self-serve genuinely stops),
**explicit** (the user sends; nothing is dispatched silently), **shared** (one destination name and one
label across surfaces), and **minimal in data** (a template, not a questionnaire). The dark-mode interest:
on purchase and plan surfaces this is a high-consequence swap of the primary action, and its label,
reason and any resulting state must stay legible in dark mode — never signalled by colour alone.

## 2. What Boma Bot actually has (the parity target)

Boma Bot has **no contact/support endpoint, address or mailto flow**; adding one is a new feature and a
DEVIATION requiring owner approval (AGENTS.md). Parity here is **dark-mode parity of the in-spec surfaces
where "contact the operator" semantics already live**:

- REQ-021 (MUST): failures surface as a run status plus a reason (CONFIRMED, `spec/01-requirements.md:170`).
- §8.9 "Error copy": one sentence of what happened, one of what to do; the **correlation id sits behind
  "details" for support** — the in-product contact affordance for a failure (CONFIRMED,
  `spec/08-ui-ux.md:198`). The failed row in `activity-screen.tsx` already pairs `text-text-error-primary`
  with `RiErrorWarningLine` and never colour alone (CONFIRMED).
- §8.7 `/admin/plugins`: the submissions queue renders **submitter contact** — a human-entered address
  shown as text to read; the error and queue surfaces must keep it readable in dark (CONFIRMED,
  `spec/08-ui-ux.md:154`). REQ-058 (SHOULD) covers administration surfaces (CONFIRMED, :473).
- REQ-057 / REQ-073: `.dark` on `<html>`, token-only styling, live user palettes (CONFIRMED).

## 3. Dark-mode parity, grounded in BoardUI tokens (never raw `dark:`)

1. **Tokens flip; we add nothing.** The error row, the "for support" disclosure and the queue's contact
   cell use only semantic tokens: `bg-background-secondary-default`, `border-separator-border`,
   `border-border-error-default`, `text-text-primary/-secondary/-tertiary`,
   `text-text-error-primary`, `text-foreground-icon-primary/-secondary/-error`, focus
   `focus-visible:ring-2 ring-border-focus-ring`, merged with `cx()`. No `dark:` utility, hex or oklch
   literal anywhere (CONFIRMED, AGENTS.md BoardUI block, docs/UI.md §Foundations 3).
2. **Re-map, don't repaint.** Follow the `styles/boma.css` precedent: dark re-maps
   `--color-text-secondary` → `neutral-400` and `text-tertiary` → `neutral-450`; `text-error-primary`
   keeps `red-400` on dark surfaces. Any contact/support tint follows that pattern — retune a token,
   never a literal (CONFIRMED, docs/UI.md "Local deviations" and "Design-system local edits").
3. **No colour-only state.** "For support", copied/sent/failed and queue review state are a sentence
   plus an icon, never a coloured chip alone — WCAG 1.4.1/1.4.11 (§8.9 run-status vocabulary).
4. **Address readability.** The submitter contact and any URL render in `text-text-primary`, `<bdi>` or
   `dir="auto"`, stay LTR under RTL, and meet 4.5:1 on the secondary surface in dark; links carry the
   `.boma-focus` ring (§8.9 RTL, docs/UI.md "Copy conventions").
5. **Type, target, icons.** Composite utilities `text-body-medium`, `text-body-2-regular`,
   `text-caption-1-semibold`; 44 px `.boma-target`; `@remixicon/react` icons as component references.

## 4. REQ-073 palettes under the dark-mode lens

A saved user palette (`themes/` JSON, REQ-073) must supply **both scheme mappings** for the roles these
surfaces use: text, secondary surface, separator, error, focus ring. With `.dark` on, the dark mapping
must land on the same tokens, so the contact affordance keeps contrast with no `dark:` class (CONFIRMED:
REQ-073 applies tokens live). A malformed palette is ignored and the saved choice stands (REQ-073).
GUESS: treat a palette whose dark contact-surface pairs fall below AA as *effectively malformed* — ignore
it with a named log line and keep the saved palette. This extends the "ignore with a log line" contract
to a contrast check and needs owner sign-off. Live reload must re-run the axe gate so a palette swap
cannot silently drop the contact text below AA. No `themes/` directory exists yet (CONFIRMED).

## 5. axe contrast gates

WCAG 2.2 AA, zero **critical/serious** findings in both schemes and both directions, reusing `analyse()`
in `a11y.spec.ts` (TAGS `wcag2a…wcag22aa`) (CONFIRMED). Check `color-contrast` specifically with (a) the
error row and its "details / for support" disclosure open in dark, (b) the `/admin/plugins` queue with a
submitter contact in dark, (c) both under a saved custom palette. Thresholds: 4.5:1 normal text, 3:1 UI
component boundaries (button edge, focus ring, icon). Gate mid-fade too — the `number-fade` precedent
starts at 0.75 for the same reason (docs/UI.md). Keep motion inside `prefers-reduced-motion`.

## 6. One REQ-named Playwright test (first gate)

Add beside the existing axe gate (reuse `seedRpc`, `analyse()`), REQ id in the title so
`tools/trace_check.py` counts it:

```
test("REQ-073 the support/contact surfaces keep AA contrast in dark mode under a saved palette", ...)
```

Steps: seed a failed run carrying a correlation id and one plugin submission with a submitter contact;
`emulateMedia({ colorScheme: "dark" })`; load `/app/bot_golden/` → open the "for support" disclosure →
assert the id is visible in a named region; load `/admin/plugins` → assert the contact renders in
`text-text-primary`; expect `analyse()` → `[]` on both; write a valid two-scheme palette into the theme
folder, trigger live reload, re-assert both routes axe-clean; write a palette with a failing dark
text/surface pair and assert it is ignored (saved choice stands, log names the file) with axe still
clean; assert no raw `dark:` class on either subtree; repeat under `boma-locale=he` for RTL.

## 7. First slice

Presentation-only, in-spec: (a) audit the activity error row, the "for support" disclosure and the
plugins queue contact cell for token-only dark styling, text+icon state and address readability;
(b) the REQ-073 test above; (c) a short palette contract doc listing the token roles a valid palette must
map in both schemes. **No** new RPC, endpoint, address, mailto or contact channel. A real
contact-the-operator channel stays a separate, owner-approved slice with its own REQ id and ADR.

## Guardrails

Inspiration only — no upstream code, asset, wording or brand; no Pro; one design system. Proposal only:
nothing was built, no test was run, no product file changed.
