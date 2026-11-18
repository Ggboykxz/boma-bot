# UI proposal 001 — healthcare clinics (primary care)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #001. Read-only on product code.
Grounding: `docs/UI.md`, `docs/IMPROVEMENTS.md`, `spec/08-ui-ux.md`, and
`apps/web/components/{shell/bot-rail,thread/thread-composer,thread/thread-screen,group/group-thread-screen}.tsx`.
Charter: Boma Bot only; no upstream/OpenBot brand, code, wording or assets; BoardUI tokens only,
`cx()` to merge, dark mode via tokens; one design system; self-hosted; keep the robot-cat identity.

## 1. Persona and top jobs-to-be-done

**Persona — Dr. Amara, family-medicine clinician in a 4-doctor clinic.** Short 15-minute consults,
frequent interruptions, a shared workstation, a multilingual patient panel (French, English,
Arabic). She uses Boma Bot as a self-hosted assistant on the clinic's own machine — no cloud, no
PHI leaving the building. She is not a technologist; a wrong click on a clinical action is a
patient-safety event, so ambiguity is worse than latency.

Top jobs:
1. **Prepare** — before a visit, see the bot's one-screen summary and open questions without
   hunting through the rail.
2. **Capture** — dictate or type a consult note fast, with unreliable hands/keyboard, without
   losing the draft to a stray Enter.
3. **Decide** — review an action the bot proposes (draft referral, refill, patient message) and be
   certain what will happen before approving; never approve by colour alone.
4. **Triage** — across a busy day, find unread results and paused follow-ups, snooze non-urgent
   ones, and know at a glance which runs are still working.
5. **Hand over** — pass an open thread to a covering colleague/bot at shift change with a clear,
   attributable record and no PHI leaked into logs or another member's view.

## 2. Five concrete changes

All strings are new keys in `lib/i18n.ts` through `t(locale, key)`; no hardcoded copy. All use
BoardUI components from `apps/web/components/` and the semantic tokens in `docs/UI.md`.

**(a) Visit-prep context strip — screen `/app/:botId` (thread header).**
A compact, read-only summary band above the transcript: chips for "next visit", "open results",
"open questions", each with a `Tooltip` that expands the detail. Built from BoardUI `Badge`/`Chip`
(`components/base/badges/chip.tsx`), `Tooltip`, `IconButton`; surfaces `bg-background-secondary-default`,
hairline `border-separator-border`, text `text-text-secondary`/`text-text-primary`, icons
`text-foreground-icon-secondary`; type `text-caption-1-semibold`. RPCs already available:
`botsGet`, `threadsHead`, `threadsMessages`. Content is bot-produced and untrusted — render as
text, never HTML; keep identifiers out of the DOM where possible.

**(b) Patient-safety approval card — screen `/app/:botId` (pending card, REQ-007/REQ-017).**
Make the "what will happen" sentence the dominant line of the approval card; add an explicit
consequence summary and a distinct confirm button label ("Approve referral", not just "Approve").
Destructive/high-impact cards use `border-border-error-default` + `text-text-error-primary` **plus**
an icon and the word "High impact" — never colour alone. BoardUI `Button`, `Badge`, `Tooltip`;
card surface `bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`.
RPCs: `threadsAnswer`, `threadsReact`. The card is announced once through the polite live region
(spec §8.9); the transcript is not a live region.

**(c) Hands-free composer — screen `/app/:botId` (composer, REQ-001; dictation path REQ-049).**
Extend `ThreadComposer` with a mic `IconButton` (`RiMicLine`) and a visible dictation-state chip;
keep Enter-to-send / Shift+Enter-newline exactly as today, and keep send disabled while an
approval is pending. BoardUI `IconButton` + `Tooltip` from `components/base/buttons/icon-button.tsx`;
focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`, 44 px `.boma-target` for gloved
use, `text-foreground-icon-primary`. RPCs: `threadsSend`; dictation text enters the same `value`
state, so no new transport.

**(d) Shift-handover banner — screen `/app/g/:groupId` (group thread, REQ-010/REQ-067/REQ-068).**
Add a dismissal-free banner naming outgoing and incoming bot with a named status chip, reusing the
existing reassign row. Buttons and chips are BoardUI `Button`, `Badge`, `Avatar`; tokens
`bg-background-secondary-default`, `border-separator-border`, `text-text-secondary`,
`text-text-error-primary` for a failed handover plus text+icon. RPCs already present:
`groupsGet`, `groupsMessages`, `groupsReassign` (and `runsPause`/`runsResume` if a run must pause).
Every message keeps its bot attribution so the record stays accountable.

**(e) Follow-up triage in the rail — screen `/app` shell left rail (REQ-009/REQ-011/REQ-012).**
Replace the unread count's bare number with `Counter` + a named `Chip` status (e.g. "Awaiting you",
"Working", "Snoozed till 16:00"), and expose due-time detail via keyboard-focusable `Tooltip`.
Group the snoozed shelf by due time using the installed `date-picker`/`date-calendar` if a picker is
added. BoardUI `Counter`, `Chip`, `IconButton`; tokens `text-text-primary`, `text-text-secondary`,
`bg-background-tertiary-default`, `animate-number-fade` for the count per `docs/UI.md`. RPCs:
`threadsMarkRead`, `threadsMarkUnread`, `threadsSnooze`, `threadsUnsnooze`, `runsList`.

## 3. Accessibility, i18n and RTL

- New labels go in `lib/i18n.ts` (English source; French/Hebrew translations; missing key falls back
  to English, never a blank or the key). No string in JSX.
- `DirectionProvider` sets locale/direction; boot script writes `lang`/`dir`. Use logical utilities
  (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Mirror the reassign/back
  arrows via `useDirection()`; keep visit times, patient IDs and URLs LTR in `<bdi>`/`dir="auto"`.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets via
  `.boma-target`; visible token-driven focus; full keyboard path (card decisions, reassign, snooze);
  status conveyed by text + icon, not colour; 360 px width; `prefers-reduced-motion` honoured.
- Clinical privacy: error copy is one sentence of what happened plus one of what to do — no IDs,
  stack traces or provider text on screen (correlation id behind "details" only, spec §8.9). Nothing
  clinical is passed to a tooltip that requires hover alone; PHI is never written to logs.

## 4. Playwright tests (REQ ids where they exist)

- `REQ-001 send creates one optimistic message that is not duplicated`
- `REQ-007 pending approval card names the exact action, takes focus, and does not block reading`
- `REQ-009 unread divider moves and snooze shelf returns the bot on time`
- `REQ-012 every run status renders a distinct named chip with the right terminal action`
- `REQ-010 group thread attributes each message and reassigns by keyboard alone`
- `REQ-067 run pause and resume announce their status through the polite live region`
- `REQ-057 Hebrew RTL loads dir=rtl with a clean axe run`
- New behaviour above without a REQ yet (dictation chip, visit-prep strip) must be given a REQ id by
  the owner before implementation, per the traceability gate — do not invent one in test names.

## 5. Highest-impact first slice

**Change (b), the patient-safety approval card.** It is the smallest, highest-value step: one screen,
no new transport (`threadsAnswer` already exists), no new component (BoardUI `Button`/`Badge`/
`Tooltip` are installed), and it directly reduces the risk of an ambiguous clinical decision.
Test-first: write `REQ-007 ... names the exact action` and extend `spec/golden/ui/REQ-007-pending-card.json`
first; ship only the dominant consequence sentence, the explicit confirm label, and the
text+icon high-impact marker. Defer (a), (c), (d), (e) until the owner assigns REQ ids and approves.
