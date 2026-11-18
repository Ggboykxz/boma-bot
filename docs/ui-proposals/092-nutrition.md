# UI proposal 092 — nutrition (dietitian and meal-plan practice)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #092. Read-only on product code.
Grounding: `docs/UI.md`, `spec/08-ui-ux.md` (§8.3, §8.5, §8.9), and two components under
`apps/web/components/`: `thread/thread-composer.tsx`, `artifacts/artifacts-grid.tsx`. RPC names
cross-checked against `apps/web/lib/api/rpc.ts` (CONFIRMED exports); REQ ids against
`spec/01-requirements.md` (CONFIRMED).
Charter: Boma Bot only; no upstream brand, code, wording or assets; BoardUI semantic tokens only —
no raw palette/hex — classes merged with `cx()`; one design system, no second; no SaaS (self-hosted
on the practice's own machine); keep the owner-supplied robot-cat identity unchanged.

## 1. Persona and top jobs-to-be-done

**Persona — Amira, a registered dietitian in a small multi-lingual practice.** She runs Boma Bot on
the clinic's own machine: a bot that drafts meal plans and a client-facing bot that logs meals and
answers food questions. Clients write in French, English, Arabic and Hebrew, mostly from a phone.
No health, food-log or body-data leaves the building or reaches a log. A wrong allergen cue or a
plan sent to the wrong client is a harm, so an ambiguous confirmation is worse than a slow screen.

Top jobs:
1. **Log a meal fast** — capture a meal or portion in a tap without losing a draft to a stray Enter.
2. **Review before send** — be certain what a bot-proposed plan or client message will do before approving.
3. **Triage the day** — see plans due, logs pending and check-ins waiting at a glance, not buried in the rail.
4. **Keep constraints explicit** — surface allergen/dietary rules as text + icon, never colour alone.
5. **Find a plan again** — keep a searchable, versioned recipe and meal-plan library openable in a thread.

## 2. Five concrete changes

All strings are new keys in `lib/i18n.ts` via `t(locale, key)` / `useTranslation()` — none in JSX.
All colours ride the semantic tokens in `docs/UI.md`; classes merge with `cx()` from `@/utils/cx`.

**(a) Meal quick-log strip — screen `/app/:botId`, composer (REQ-001).**
Extend `ThreadComposer` with a compact row of meal chips (breakfast/lunch/dinner/snack) and a portion
picker that insert a stable, catalogue-authored token into the draft through the existing
`value`/`onChange` props — no new transport. Keep Enter-to-send / Shift+Enter-newline exactly as today
and keep send disabled while an approval is pending. BoardUI `IconButton` + `Tooltip`; icon
`text-foreground-icon-primary`, focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`,
44 px `.boma-target`, pills `rounded-full`. RPC already exported: `threadsSend`. A chip carries a
visible text label and an `aria-label` from the catalogue — an icon alone is insufficient.

**(b) Meal-plan / client-message approval card — screen `/app/:botId`, pending card (REQ-007, REQ-017).**
In the `CardBlock` machinery, make the "what will happen" sentence dominant and add a consequence
summary: which client, which plan, which allergens. Use an explicit confirm label ("Send plan to
<client>", not just "Approve"). High-impact cards use `border-border-error-default` +
`text-text-error-primary` **plus** `RiAlertLine` and the words "Check allergens" — never colour alone.
BoardUI `Button`, `Badge`, `Tooltip`; surface `bg-background-primary-default`, `rounded-3xl`,
`border-border-button-default`, type `text-body-medium`. RPCs: `threadsAnswer`, `threadsSubscribe`.
It scrolls into view and takes focus but does not block reading or other threads (spec §8.3).

**(c) Daily-status triage strip — screen `/app/:botId`, thread header.**
A read-only band above the transcript: chips for "plan due", "log pending", "check-in waiting", each
with a `Tooltip` that expands detail. BoardUI `Chip`/`Badge`, `Tooltip`, `IconButton`; surface
`bg-background-secondary-default`, hairline `border-separator-border`, text
`text-text-secondary`/`text-text-primary`, icons `text-foreground-icon-secondary`, type
`text-caption-1-semibold`. RPCs: `botsGet`, `threadsHead`, `threadsMessages`. Bot-produced content is
untrusted — render as text, never HTML; keep client identifiers out of the DOM where possible.

**(d) Plan and recipe library — screen `/app/artifacts` (spec §8.5).**
Reuse `ArtifactsGrid` as the library: cards for name, type, owner bot and updated time, filtered by
bot and searchable by name; the viewer offers a version selector, download and "open in thread".
BoardUI `Table`/`Card`, `Badge`, `IconButton` with `bg-background-tertiary-default`,
`border-separator-border`, `text-text-primary`. RPCs: `artifactsListSpace`, `artifactsGet`,
`artifactsListVersions`. Content is untrusted: never render artifact HTML in the app document and keep
images inside a bounded frame.

**(e) Check-in reminder shelf — screen `/app` shell left rail (REQ-009, REQ-011, REQ-012).**
Replace the bare unread count with a named `Chip` status ("Check-in due", "Log pending", "Snoozed till
16:00") and expose due-time detail through a keyboard-focusable `Tooltip`. Group by due time. BoardUI
`Chip`, `IconButton`; tokens `text-text-primary`, `text-text-secondary`, `bg-background-tertiary-default`,
`animate-number-fade` for the count per `docs/UI.md`. RPCs: `threadsMarkRead`, `threadsMarkUnread`,
`threadsSnooze`, `threadsUnsnooze`, `runsList` (a routine-driven check-in is a run with a routine
trigger, spec §8.4 — no new client RPC).

## 3. Accessibility, i18n and RTL

- Every string in `lib/i18n.ts` (English source; French/Hebrew/Arabic translations; a missing key falls
  back to English, never a blank or the raw key). No string in JSX.
- `DirectionProvider` + boot script set `lang`/`dir` together; use logical utilities (`ms`/`me`,
  `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Mirror the meal strip and the disclosure
  arrow via `useDirection()`; keep portion numbers, client codes, food quantities and URLs LTR in
  `<bdi>`/`dir="auto"`.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px `.boma-target`;
  visible token-driven focus; full keyboard path (meal chips, card decisions, triage tooltips, library,
  snooze); allergen/diet status by text + icon, not colour; 360 px width; `prefers-reduced-motion`
  honoured; no animation on streaming content (spec §8.9).
- Announce an approval card, a meal-log confirmation and a failed send once through the polite live
  region; the transcript itself is not a live region (spec §8.9).
- Health data: error copy is one sentence of what happened plus one of what to do — no client names,
  ids, stack traces or provider text on screen (correlation id behind "details" only). Food/health data
  is never passed to a hover-only tooltip nor written to logs. The robot-cat mark stays unchanged.

## 4. Playwright tests (REQ ids where they exist)

- `REQ-001 meal quick-log sends one optimistic message that is replaced, not duplicated`
- `REQ-007 pending meal-plan card names the exact action, takes focus, and does not block reading`
- `REQ-017 approval states what will be sent and is idempotent on replay`
- `REQ-009 unread check-in chip and snooze shelf return the reminder on time`
- `REQ-012 every run status renders a distinct named chip with the right terminal action`
- `REQ-057 Hebrew RTL loads dir=rtl with a clean axe run`
- The meal strip, triage chips and allergen cue above have no REQ yet: the owner must assign REQ ids
  before implementation; do not invent them in test names (traceability gate).

## 5. Highest-impact first slice

**Change (b), the meal-plan / client-message approval card.** Smallest, highest-value step: one
screen, no new transport (`threadsAnswer`/`threadsSubscribe` already exist), no new component
(`CardBlock`, `Button`, `Badge`, `Tooltip` are installed), and it directly reduces the worst harm in
this sector — a wrong plan or allergen cue reaching a client. Test-first: write
`REQ-007 pending meal-plan card names the exact action...` and extend
`spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant consequence sentence, the
explicit "Send plan to <client>" label and the text+icon allergen marker. Defer (a), (c), (d), (e)
until the owner assigns REQ ids and approves.
