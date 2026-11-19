# Brief 0018 — "always allow on" through the dark-mode lens

- Brief: id 18, area `always-allow-on`, lens `dark-mode`, slug `0018-always-allow-on-dark-mode`.
- Status: **proposal**. Upstream read as **data only** (`/tmp/opencode/openbot-parity`, HEAD
  `386085ff`): `CHANGELOG.md` lines 1362, 1670, 7192, 7751, 8379. No upstream code, asset, wording
  or brand reproduced here; no Pro component or licence. Product code read-only; this file is the
  only artefact written. English (AGENTS.md disk convention).
- **CONFIRMED** = read from a local source this session; **GUESS** = recommendation for the owner.

## 1. The upstream capability, in my own words (inspiration only)

A tool call that changes files or runs a command can be answered in two ways, Allow or Deny. While
a bot is in the ask-each-time mode, a third answer is offered on the waiting card: make this one
answer permanent for that whole class of call. Taking it moves the bot to an unrestricted standing
that no longer stops for those calls; it clears the cards already parked for that bot and keeps the
same conversation instead of starting a fresh session (CONFIRMED, CHANGELOG 8379, 7192, 7751). The
widening is gated: an admin changing the permission from another machine also clears waiting cards,
and a compact companion surface omits the permanent answer so nobody authorises text they cannot see
(CONFIRMED, 7751, 1362); some deployments force the level so it cannot be narrowed back (1670).

Accessibility interest: the permanent answer is a high-consequence widening decision on the card. In
dark mode its state (waiting / granted / now-unrestricted) must stay legible — never a colour alone —
because removing a safety gate is exactly the kind of state a user must read in any theme.

## 2. What Boma Bot actually has (the parity target)

Boma Bot deliberately has no agent "power levels" and no persistent "always allow" control — adding
one would be a new feature and a DEVIATION needing owner approval (AGENTS.md). Parity here is
**dark-mode parity of the decision surface**, not feature-for-feature copying:

- REQ-007 / REQ-017 — a pending approval card gates a side effect until answered; the card is a named
  region with a polite `status` ("Waiting for your decision") and Approve / Deny buttons
  (CONFIRMED, `spec/01-requirements.md`, `apps/web/components/thread/message-blocks.tsx`).
- REQ-057 — dark mode is the `.dark` class on `<html>`; components never carry `dark:` overrides
  (CONFIRMED, `docs/UI.md` §Foundations 3).
- The axe gate already scans every route in both schemes and demands zero critical/serious findings
  (CONFIRMED, `apps/web/tests/a11y.spec.ts`).
- REQ-073 — user-written JSON palettes are read from the deployment theme folder and applied live;
  a malformed file is ignored and the saved choice stands (CONFIRMED, `spec/01-requirements.md:587`).
  No themes directory exists yet (CONFIRMED, `find` for `themes/` returned nothing).

## 3. Dark-mode parity, grounded in BoardUI tokens (never `dark:` raw colours)

1. **Tokens flip automatically; we add no overrides.** The decision card and any future grant control
   use only semantic tokens: `bg-background-secondary-default` surface, `border-separator-border`
   hairline, `text-text-primary` / `text-text-secondary` copy, `text-foreground-icon-primary` icons,
   error state `text-text-error-primary`, focus `focus-visible:ring-2
   focus-visible:ring-border-focus-ring`, merged with `cx()`. Dark mode rides the `.dark` class; no
   `dark:text-*`, hex or oklch literal is introduced (CONFIRMED, AGENTS.md BoardUI block).
2. **No colour-only state.** Waiting / granted / unrestricted is a sentence plus an icon, never a
   coloured chip alone (REQ-007 region text; §8.9 "run status vocabulary"). This keeps the state
   readable in either scheme and satisfies WCAG 1.4.1 / 1.4.11.
3. **Re-map, don't repaint.** The existing precedent is `styles/boma.css`: `--color-text-secondary`
   is re-mapped to `--color-neutral-400` inside `.dark` because BoardUI's default reached 3.95:1
   (below 4.5:1); 7.4:1 is the same role in dark (CONFIRMED, `docs/UI.md:87`, `styles/boma.css`).
   Any future always-allow state tint follows this pattern — retune a token, never a literal.
4. **Type, direction, target.** Composite utilities `text-body-medium`, `text-body-2-regular`,
   `text-caption-1-semibold`; logical `ps/pe`, `ms/me`, `text-start` under `DirectionProvider`;
   44 px `.boma-target`; icons from `@remixicon/react` as component references. All grant copy stays
   in the i18n catalogue (REQ-057).

## 4. REQ-073 palettes under the dark-mode lens

- A custom palette (e.g. an `ocean.json`) must supply a light **and** a dark mapping for the roles the
  decision surface uses: text, secondary surface, separator, focus ring and error. When `.dark` is
  on, the dark values must land on the same tokens, so the card keeps contrast without a `dark:`
  class (CONFIRMED: REQ-073 applies tokens live; AGENTS.md forbids overrides).
- A malformed palette is ignored and the saved choice stands (REQ-073). GUESS: treat a palette whose
  decision-surface token pairs fall below AA as *effectively malformed* — ignore it with a named log
  line and keep the saved palette rather than render an unreadable grant button. This extends the
  "ignore with a log line" contract to a contrast check and needs owner sign-off.
- Live reload must re-run the axe gate: a palette changing while the card waits cannot silently drop
  the waiting/granted text below AA.

## 5. axe contrast gates

- Gate: WCAG 2.2 AA, zero **critical** or **serious** axe findings in both schemes and both
  directions, reusing `analyse()` in `a11y.spec.ts` (TAGS `wcag2a…wcag22aa`) (CONFIRMED).
- Contrast thresholds: 4.5:1 for normal text (effect/reversal sentences, the status line), 3:1 for
  large text and UI component boundaries (button edge, focus ring, icon). Check the `color-contrast`
  rule specifically while a pending card is on screen in dark mode.
- Gate the card in three states — waiting, answered, and (if the control is ever approved) the
  now-unrestricted label — because axe scans mid-fade: a token passing at full opacity can fail under
  a low-alpha overlay (`docs/UI.md:165`). Keep animation within `prefers-reduced-motion`, above the
  AA alpha floor, and run with a saved custom palette so REQ-073 cannot regress the dark surface.

## 6. One REQ-named Playwright test (first gate)

Add to `apps/web/tests/theme-palettes.spec.ts` (new file for REQ-073; reuse `seedRpc` and the
`analyse()` axe helper from `a11y.spec.ts`), named so the REQ id appears in the title:

```
test("REQ-073 the always-allow decision surface keeps AA contrast in dark mode under a saved palette", ...)
```

Steps: (1) seed a pending approval via `seedRpc` as in `approval-card.spec.ts` and load
`/app/bot_golden/` with `page.emulateMedia({ colorScheme: "dark" })`; (2) assert the region
"Pending decision" and its `status` are visible and `expect(await analyse(page)).toEqual([])`;
(3) write a valid two-scheme palette into the theme folder, trigger live reload, assert the card is
still visible and the axe result is still empty; (4) write a palette with a failing dark text/surface
pair and assert it is ignored (saved choice stands, log names the file) with axe still clean;
(5) assert no element uses a raw `dark:` colour class on the card subtree. Zero critical/serious
findings throughout, in both directions.

## 7. First slice

Keep it presentation-only and in-spec: (a) audit the approval card (`message-blocks.tsx`) for
token-only dark-mode styling and no colour-only state, (b) the REQ-073 contrast test above, (c) a
documented REQ-073 palette contract listing the token roles a valid palette must map in both schemes.
No new RPC, grant control or deployment switch. Land it, run `a11y`, `approval-card`, `golden` and
`motion` specs, then propose the persistent always-allow control separately for owner approval (new
REQ id, ADR, PARITY DEVIATION) per AGENTS.md.

## 8. Evidence

CONFIRMED — `briefs-1000.jsonl` id 18; `spec/01-requirements.md` REQ-007/017/057/073;
`spec/08-ui-ux.md` §8.9; `docs/UI.md` Foundations/token re-map/contrast; `apps/web/styles/boma.css`;
`apps/web/tests/a11y.spec.ts`, `approval-card.spec.ts`; upstream data only — CHANGELOG 1362, 1670,
7192, 7751, 8379. GUESS — contrast-failure-as-malformed rule, palette contract, grant control shape.
