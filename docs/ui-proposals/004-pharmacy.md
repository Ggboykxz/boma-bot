# UI proposal 004 — pharmacy (community and hospital dispensary)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #004. Read-only on product code.
Grounding: `docs/UI.md`, `docs/IMPROVEMENTS.md`, `spec/08-ui-ux.md`, and
`apps/web/components/{thread/thread-composer,shell/bot-rail,thread/thread-screen}.tsx` plus
`apps/web/lib/api/rpc.ts`. This is a use-case lens on Boma Bot's *existing* screens, not a new app.
Charter: Boma Bot only — no upstream/OpenBot brand, code, wording or assets; BoardUI tokens only
(no raw palette/hex), `cx()` to merge, dark mode via tokens; one design system; self-hosted; keep the
robot-cat identity in the product-mark slot. Every string is a new key in `lib/i18n.ts`.

## 1. Persona and top jobs-to-be-done

**Persona — Fatima, a pharmacist at a busy community dispensary.** Counter work, gloved or wet
hands, frequent interruptions, a multilingual panel (French, English, Arabic). Boma Bot runs on the
pharmacy's own machine: no prescription data leaves the building, no hosted service. She is not a
technologist; a wrong click on a verification or dispensing action is a patient-safety event, so
ambiguity is worse than latency.

Top jobs:
1. **Verify** — before releasing a medicine, read the bot's one-screen check summary (interaction,
   dose range, duplicate therapy) without hunting through the rail.
2. **Capture** — log an intervention or counselling note fast, without losing the draft to a stray
   Enter or a card arrival.
3. **Confirm** — review a proposed action (refill reminder, stock order, label draft) and be certain
   what will happen before approving; never approve by colour alone.
4. **Triage** — across the day, find verification tasks awaiting her, paused follow-ups and working
   runs; snooze the non-urgent ones and know what is still in flight.
5. **Hand over** — pass a pending verification to a covering colleague/bot at shift change with a
   clear, attributable record and no dispense data leaked into logs or another member's view.

## 2. Five concrete changes

All use installed BoardUI components and the semantic tokens in `docs/UI.md`; `cx()` merges classes,
icons are `@remixicon/react` component references, dark mode rides the tokens.

**(a) Dispense-check result card — `/app/:botId` (transcript block, REQ-007/REQ-012).**
Render a bot-produced check result as a named card: a severity chip ("High", "Moderate", "Low",
"None") using text + icon + token, never colour alone. High severity uses
`border-border-error-default` + `text-text-error-primary` **plus** `RiAlertLine` and the word
"High alert". Body is bot-produced and untrusted — render as text, never HTML; keep prescription
ids out of the DOM where possible. Components `Chip`/`Badge`, `Tooltip`, `IconButton`
(`components/base/badges/chip.tsx`, `.../buttons/icon-button.tsx`, `.../tooltip/tooltip.tsx`);
surface `bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`, type
`text-caption-1-semibold`/`text-body-regular`. RPCs: `threadsMessages`, `threadsAnswer`,
`threadsReact`. The card announces once via the polite live region (spec §8.9); the transcript
itself is not a live region.

**(b) Consequence-first approval card — `/app/:botId` (pending card, REQ-007/REQ-017).**
Make the "what will happen" sentence the dominant line; give an explicit confirm label ("Send
refill reminder", not just "Approve") and a "verify before dispensing" marker in text + icon +
token. Destructive or high-alert cards add `border-border-error-default` and
`text-text-error-primary` alongside the icon and word — never colour alone. Components `Button`,
`Badge`, `Tooltip`; tokens `bg-background-primary-default`, `border-border-button-default`,
`text-text-primary`, `text-text-secondary`. RPCs: `threadsAnswer`. The card takes focus but does
not block reading or other threads (spec §8.3).

**(c) Verification-queue triage — `/app/board` (feature-flagged) and shell rail (REQ-009/REQ-011/
REQ-012/REQ-038).** Give prescription-review tickets named status columns ("Awaiting verification",
"Working", "Ready to release", "Snoozed till …") as `Chip` statuses; replace the rail's bare unread
number with `Counter` + a named chip and a keyboard-focusable `Tooltip` for the due time. Group the
snoozed shelf by due time using the installed `date-picker` if a picker is added. Components
`Counter`, `Chip`, `IconButton`, the installed `table` (per `docs/UI.md`); tokens
`text-text-primary`, `text-text-secondary`, `bg-background-tertiary-default`,
`border-border-table`, `animate-number-fade`, focus `focus-visible:ring-border-focus-ring`. RPCs:
`boardsGet`, `ticketsCreate`, `ticketsMove`, `ticketsComment`, `runsList`, `threadsSnooze`,
`threadsUnsnooze`, `threadsMarkRead`. No decorative spinner outlives its work.

**(d) Multilingual patient-message preview — `/app/:botId` (right panel/drawer, REQ-026/REQ-036).**
A bot-drafted counselling message or label, shown read-only in a panel with a language toggle
(English/French/Hebrew) via BoardUI `Tabs` + `ScrollFade` for the scrolled top edge. The Hebrew
text renders RTL inside an isolated `<bdi dir="auto">` region; drug names, codes and URLs stay LTR.
Content is untrusted: plain text only, never app HTML. Components `Tabs`, `Tooltip`, `IconButton`,
`ScrollFade` (`components/foundations/scroll-fade/scroll-fade.tsx`); tokens
`bg-background-secondary-default`, `border-separator-border`, `text-text-secondary`,
`text-foreground-icon-secondary`, type `text-body-2-regular`. RPCs: `botsGet`, `threadsMessages`,
`artifactsListSpace`, `artifactsGet`. A first-run empty state keeps the robot-cat mark.

**(e) Coverage handover in a room — `/app/g/:groupId` (REQ-010/REQ-067/REQ-068).**
For a verification pending at shift change, a dismissal-free banner names outgoing and incoming
bot with a named status chip and a reassign control; buttons and chips are BoardUI `Button`,
`Badge`, `Avatar`, `Tooltip` with tokens `bg-background-secondary-default`,
`border-separator-border`, `text-text-error-primary` for a failed handover plus text+icon. Each
message keeps its bot attribution so the record stays accountable. RPCs: `groupsGet`,
`groupsMessages`, `groupsReassign`, `runsPause`, `runsResume`.

## 3. Accessibility, i18n and RTL

- Every new label is a key in `lib/i18n.ts` through `useTranslation()` / `t(locale, key)` (English
  source; French/Hebrew translations; a missing key falls back to English, never a blank or the key).
  No string in JSX.
- `DirectionProvider` sets locale/direction; the boot script writes `lang`/`dir`. Use logical
  utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Mirror the
  reassign and disclosure arrows via `useDirection()`; keep medicine names, prescription numbers,
  times and URLs LTR in `<bdi>`/`dir="auto"`. The language toggle changes text and layout direction
  together — it does not flip the physical coordinates of any measured geometry.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets via
  `.boma-target` for gloved use; token-driven visible focus; full keyboard path (card decision,
  reassign, snooze, language tabs, ticket move); status by text + icon, not colour; 360 px width;
  `prefers-reduced-motion` honoured.
- Dispense privacy: error copy is one sentence of what happened plus one of what to do — no ids,
  stack traces or provider text on screen (correlation id behind "details" only, spec §8.9). No
  dispense data is passed to a hover-only tooltip; nothing clinical is written to logs.

## 4. Playwright tests (REQ ids where they exist)

- `REQ-007 dispense-check card names severity, takes focus, and does not block reading`
- `REQ-017 approval names the exact dispensary action and the high-alert marker uses text+icon`
- `REQ-012 every verification run status renders a distinct named chip with the right terminal action`
- `REQ-038 board verification queue shows named status columns and moves a ticket by keyboard`
- `REQ-009 unread divider moves and the snooze shelf returns the verification on time`
- `REQ-010 group room attributes each message to a bot and reassigns coverage by keyboard alone`
- `REQ-036 patient label artifact viewer keeps drug names and URLs LTR`
- `REQ-067 run pause and resume announce their status through the polite live region`
- `REQ-057 Hebrew RTL loads dir=rtl with a clean axe run`
- New behaviour without a REQ yet (severity tiers, language toggle, typed intervention chip) must be
  assigned a REQ id by the owner before implementation — do not invent one in a test name.

## 5. Highest-impact first slice

**Change (a), the dispense-check severity card.** One screen, no new transport (`threadsMessages`/
`threadsAnswer` already exist), no new component (BoardUI `Chip`/`Badge`/`Tooltip` are installed),
and it directly reduces the risk of misreading a check result. Test-first: write
`REQ-007 dispense-check card names severity…`, extend `spec/golden/ui/REQ-007-pending-card.json`,
and add an axe assertion (light, dark, RTL). Ship only the named severity chip, the text+icon
high-alert marker, and text-only untrusted content. Defer (b)–(e) until the owner assigns REQ ids
and approves.
