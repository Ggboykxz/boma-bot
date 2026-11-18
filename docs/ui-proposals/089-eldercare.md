# UI proposal 089 — eldercare (home care and assisted living)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #089. Read-only on product code.
Grounding: `docs/UI.md`, `spec/08-ui-ux.md`, and
`apps/web/components/{thread/computer-panel,thread/thread-composer}.tsx`. Every RPC name below is
CONFIRMED in `apps/web/lib/api/rpc.ts`; every REQ id is CONFIRMED in `spec/01-requirements.md`.
Charter: Boma Bot only; no upstream brand, code, wording or assets; BoardUI semantic tokens only —
no raw palette classes or hex literals — `cx()` to merge; one design system, no second; no SaaS
(self-hosted on the residence's own machine, no resident data off-site); keep the owner-supplied
robot-cat identity.

## 1. Persona and top jobs-to-be-done

**Persona — Rosa, lead caregiver at a 12-resident assisted-living home.** She works from a shared
station between rooms, often mid-task, with a multilingual resident and family panel (English,
French, Arabic). Boma Bot runs self-hosted in the home: no medication list, note or family message
leaves the building. A wrong click on a care action or an ambiguous handover reaches a resident, so
an unclear confirmation is worse than a slow screen.

Top jobs:
1. **Check** — see at a glance which medication check-ins and wellness checks are due, which bot
   runs are active, and which family messages wait, without hunting through the rail.
2. **Capture** — log an observation (fall, refusal, mood) reliably with busy hands, without losing
   the draft to a stray Enter.
3. **Decide** — review a bot-proposed care action (call the GP, send a family update) and be certain
   what will be sent before approving; never confirm by colour alone.
4. **Hand over** — pass an open resident thread to the incoming shift's bot at shift change with a
   clear, attributable record and no resident data leaked to another member's view.
5. **Take over** — let the bot operate the care-planning portal on-screen, and intervene cleanly
   when it needs her hands.

## 2. Five concrete changes

All strings are new keys in `lib/i18n.ts` through `t(locale, key)`; no hardcoded copy. All reuse
installed BoardUI components under `apps/web/components/` and the semantic tokens in `docs/UI.md`.

**(a) Due-checks strip — screen `/app/:botId` (thread header).**
A compact, read-only band above the transcript: chips for "next medication", "wellness check due",
"family reply waiting", each with a keyboard-focusable `Tooltip` for detail. BoardUI `Chip`/`Badge`
(`components/base/badges/chip.tsx`), `Tooltip`, `IconButton`; surface `bg-background-secondary-default`,
hairline `border-separator-border`, text `text-text-secondary`/`text-text-primary`, icons
`text-foreground-icon-secondary`, type `text-caption-1-semibold`. RPCs already present: `botsGet`,
`threadsHead`, `threadsMessages`, `runsList`; the reminder opt-in reads `notificationsStatus`
(REQ-064). Bot-produced content is untrusted — render as text, never HTML; keep resident identifiers
out of the DOM where possible and never expose them in hover-only tooltips.

**(b) Care-action approval card — screen `/app/:botId` (pending card, REQ-007/REQ-017).**
Make the "what will happen" sentence the dominant line; add a consequence summary (which resident,
which message) and an explicit confirm label ("Call the GP", not just "Approve"). High-impact cards
use `border-border-error-default` + `text-text-error-primary` **plus** an icon and the words
"High impact" — never colour alone. BoardUI `Button`, `Badge`, `Tooltip`; card surface
`bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`. RPCs: `threadsAnswer`,
`threadsReact`. Announced once through the polite live region (spec §8.9); the transcript is not a
live region.

**(c) Hands-free observation composer — screen `/app/:botId` (composer, REQ-001; voice REQ-049).**
Extend `ThreadComposer` with a mic `IconButton` (`RiMicLine`) and a visible dictation-state chip, plus
catalogued quick-tags (fall, refused medication, mood) that insert a stable token through the existing
`value`/`onChange` props. Keep Enter-to-send / Shift+Enter-newline exactly as today, and keep send
disabled while an approval is pending (`awaitingApproval`). BoardUI `IconButton` + `Tooltip`
(`components/base/buttons/icon-button.tsx`); focus `focus-visible:ring-2
focus-visible:ring-border-focus-ring`, 44 px `.boma-target` for busy hands, icon
`text-foreground-icon-primary`. RPC `threadsSend`; dictation and tags enter the same `value` state,
so no new transport.

**(d) Care-planning takeover panel — screen `/app/:botId` (computer panel, REQ-026/028/029).**
Name every state (starting, running, awaiting takeover, released) and add a clear "Leaving this
screen pauses nothing — the care plan stays open" caption above takeover/release controls. BoardUI
`Button` + `Badge`, status text + icon from `text-text-secondary`, surface
`bg-background-secondary-default`; never colour only. RPCs: `computerStatus`, `computerTakeover`,
`computerRelease`, `computerOpenScreen`. Mirror drawer motion in RTL via `useDirection()`; keep the
physical screen frame coordinates untouched.

**(e) Follow-up triage in the rail — screen `/app` shell left rail (REQ-009/REQ-011/REQ-012).**
Replace the unread count's bare number with `Counter` + a named `Chip` status ("Awaiting you",
"Checking", "Snoozed till 16:00") and expose due-time detail via a keyboard-focusable `Tooltip`.
Group the snoozed shelf by due time using the installed `date-picker`/`date-calendar` if a picker is
added. BoardUI `Counter`, `Chip`, `IconButton`; tokens `text-text-primary`, `text-text-secondary`,
`bg-background-tertiary-default`, `animate-number-fade` for the count per `docs/UI.md`. RPCs:
`threadsMarkRead`, `threadsMarkUnread`, `threadsSnooze`, `threadsUnsnooze`, `runsList`.

## 3. Accessibility, i18n and RTL

- New labels go in `lib/i18n.ts` (English source; French/Hebrew translations; missing key falls back
  to English, never a blank or the key). No string in JSX.
- `DirectionProvider` sets locale/direction; the boot script writes `lang`/`dir`. Use logical
  utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Mirror the
  takeover and handover arrows via `useDirection()`; keep check times, resident IDs, medication
  names and URLs LTR in `<bdi>`/`dir="auto"`.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets via
  `.boma-target`; visible token-driven focus; full keyboard path (card decisions, takeover/release,
  quick-tags, snooze); status conveyed by text + icon, not colour; 360 px width;
  `prefers-reduced-motion` honoured.
- Privacy: error copy is one sentence of what happened plus one of what to do — no IDs, stack traces
  or provider text on screen (correlation id behind "details" only, spec §8.9). No resident or family
  data in a hover-only tooltip or in logs. The robot-cat mark stays unchanged.

## 4. Playwright tests (REQ ids where they exist)

- `REQ-001 send creates one optimistic message that is not duplicated`
- `REQ-007 pending care-action card names the exact action, takes focus, and does not block reading`
- `REQ-009 unread divider moves and snooze shelf returns the check-in on time`
- `REQ-012 every run status renders a distinct named chip with the right terminal action`
- `REQ-010 group thread attributes each message and REQ-068 handover reassigns by keyboard alone`
- `REQ-026 computer panel names starting, running, takeover and released states`
- `REQ-028 takeover and REQ-029 release are keyboard-operable and announced`
- `REQ-057 Hebrew RTL loads dir=rtl with a clean axe run`
- New visual treatment without a dedicated REQ (due-checks strip, handover banner) must be given a
  REQ id by the owner before implementation, per the traceability gate — do not invent one in test
  names.

## 5. Highest-impact first slice

**Change (b), the care-action approval card.** Smallest, highest-value step: one screen, no new
transport (`threadsAnswer`/`threadsReact` already exist), no new component (BoardUI `Button`/`Badge`/
`Tooltip` are installed), and it directly reduces the risk of an ambiguous resident- or family-facing
action. Test-first: write `REQ-007 ... names the exact action` and extend
`spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant consequence sentence, the
explicit confirm label, and the text+icon high-impact marker. Defer (a), (c), (d), (e) until the owner
assigns REQ ids for the new controls and approves.
