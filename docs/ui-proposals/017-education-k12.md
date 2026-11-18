# UI proposal 017 — education, K-12

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #017. Read-only on product code.
Grounding: `docs/UI.md`, `spec/08-ui-ux.md`, and
`apps/web/components/{thread/thread-composer,shell/bot-rail}.tsx` (read this session). RPC names are
CONFIRMED from `@/lib/api/rpc` usage in those components; component paths and tokens are CONFIRMED
from `apps/web/components/` and `docs/UI.md`. Persona/jobs and screen treatments are GUESS.
Charter: Boma Bot only; no upstream/OpenBot brand, code, wording or assets; BoardUI tokens only,
`cx()` to merge, dark mode via tokens; one design system; self-hosted; keep the robot-cat identity.

## 1. Persona and top jobs-to-be-done

**Persona — Ms. Leila, a middle-school science teacher in a 1:1 school.** She runs Boma Bot
self-hosted on the school's own machine: no lesson draft, student question or parent message leaves
the building. Her classes are multilingual (English, French, Arabic) and mixed-ability. She is not a
technologist; a wrong click that sends an unchecked bot reply to a child is a safeguarding event, so
ambiguity is worse than latency. The bot is a teaching assistant, never a substitute for her judgement.

Top jobs:
1. **Prepare** — before a lesson, see the bot's one-screen summary of open class questions and drafts
   without hunting through the rail.
2. **Draft** — produce an age-appropriate explanation or worksheet fast, without losing the draft to
   a stray Enter or a rerender.
3. **Guard** — review a proposed reply to a student or parent and be certain what will be sent; never
   approve a student-facing action by colour alone.
4. **Triage** — across several classes, find unanswered questions and paused follow-ups, snooze
   non-urgent ones, and know at a glance which runs are still working.
5. **Hand over** — pass an open class thread to a covering colleague or bot at end of day with a
   clear, attributable record and no student data in logs or another class's view.

## 2. Five concrete changes

All strings are new keys in `lib/i18n.ts` through `t(locale, key)` (here `useTranslation()`); no
hardcoded copy. All use BoardUI components under `apps/web/components/` and the semantic tokens in
`docs/UI.md`. Bot-produced content is untrusted: render as text, never HTML.

**(a) Lesson-prep context strip — screen `/app/:botId` (thread header).**
A compact, read-only band above the transcript: chips for "next class", "open questions", "drafts
due", each with a `Tooltip` (`components/base/tooltip/tooltip.tsx`) that expands the detail. Built
from BoardUI `Badge`/`Chip` (`components/base/badges/chip.tsx`), `Tooltip`, `IconButton`;
surfaces `bg-background-secondary-default`, hairline `border-separator-border`, text
`text-text-secondary`/`text-text-primary`, icons `text-foreground-icon-secondary`; type
`text-caption-1-semibold`. RPCs already available: `botsGet`, `threadsHead`, `threadsMessages`.
Keep student names/IDs out of the DOM where possible.

**(b) Student-safety approval card — screen `/app/:botId` (pending card, REQ-007/REQ-017).**
Make the "what will be sent, and to whom" sentence the dominant line; add an explicit consequence
summary and a distinct confirm label ("Send reply to parent", not just "Approve"). Student-facing /
high-impact cards use `border-border-error-default` + `text-text-error-primary` **plus** an icon and
the words "Student-facing" — never colour alone. BoardUI `Button`, `Badge`, `Tooltip`; card surface
`bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`. RPCs: `threadsAnswer`,
`threadsReact`. The card is announced once through the polite live region (spec §8.9); the transcript
is not a live region.

**(c) Guardrail composer — screen `/app/:botId` (composer, REQ-001).**
Extend `ThreadComposer` with a visible guardrail-state chip ("Student-safe replies on") and a shield
`IconButton` with a `Tooltip` explaining the mode; keep Enter-to-send / Shift+Enter-newline exactly
as today, and keep send disabled while an approval is pending (`awaitingApproval`). BoardUI
`IconButton` (`components/base/buttons/icon-button.tsx`) + `Tooltip`; icon reference from
`@remixicon/react` (e.g. `RiShieldCheckLine`); focus `focus-visible:ring-2
focus-visible:ring-border-focus-ring`, 44 px `.boma-target`, `text-foreground-icon-primary`.
RPCs: `threadsSend`; the chip is local state, no new transport.

**(d) Cover handover banner — screen `/app/g/:groupId` (class group, REQ-010/REQ-067/REQ-068).**
Add a dismissal-free banner naming outgoing and incoming teacher/bot with a named status chip,
reusing the existing reassign row. BoardUI `Button`, `Badge`, `Avatar` (`components/base/avatar/avatar.tsx`);
tokens `bg-background-secondary-default`, `border-separator-border`, `text-text-secondary`,
`text-text-error-primary` for a failed handover plus text+icon. RPCs already present: `groupsGet`,
`groupsMessages`, `groupsReassign` (and `runsPause`/`runsResume` if a run must pause). Every message
keeps its bot attribution so the record stays accountable.

**(e) Question triage in the rail — screen `/app` shell left rail (REQ-009/REQ-011/REQ-012).**
Replace the unread count's bare number with `Counter` + a named `Chip` status (e.g. "Needs review",
"Working", "Snoozed till 16:00"), and expose due-time detail via keyboard-focusable `Tooltip`. Group
the snoozed shelf by due time using the installed `date-picker`/`date-calendar` if a picker is added.
BoardUI `Counter` (`components/base/badges/counter.tsx`), `Chip`, `IconButton`; tokens
`text-text-primary`, `text-text-secondary`, `bg-background-tertiary-default`, `animate-number-fade`
per `docs/UI.md`. RPCs: `threadsMarkRead`, `threadsMarkUnread`, `threadsSnooze`, `threadsUnsnooze`,
`runsList`.

## 3. Accessibility, i18n and RTL

- New labels go in `lib/i18n.ts` (English source; French/Hebrew translations; missing key falls back
  to English, never a blank or the key). No string in JSX.
- `DirectionProvider` sets locale/direction; the boot script writes `lang`/`dir`. Use logical
  utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`, `rounded-s`/`rounded-e`).
  Mirror the reassign/back arrows via `useDirection()`; keep class times, student IDs and URLs LTR in
  `<bdi>`/`dir="auto"`.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets via
  `.boma-target`; visible token-driven focus; full keyboard path (card decisions, reassign, snooze);
  status conveyed by text + icon, not colour; 360 px width; `prefers-reduced-motion` honoured.
- Student privacy: error copy is one sentence of what happened plus one of what to do — no student
  name, ID, grade, stack trace or provider text on screen (correlation id behind "details" only,
  spec §8.9). Nothing a teacher must act on is passed to a hover-only tooltip; no student data is
  written to logs or another class's view.

## 4. Playwright tests (REQ ids where they exist)

- `REQ-001 send creates one optimistic message that is not duplicated`
- `REQ-007 pending student-facing approval card names the exact action, takes focus, and does not block reading`
- `REQ-009 unread divider moves and snooze shelf returns the class on time`
- `REQ-012 every run status renders a distinct named chip with the right terminal action`
- `REQ-010 group thread attributes each message and reassigns by keyboard alone`
- `REQ-057 Hebrew RTL loads dir=rtl with a clean axe run`
- New behaviour above without a REQ yet (lesson-prep strip, guardrail chip) must be given a REQ id by
  the owner before implementation, per the traceability gate — do not invent one in test names.

## 5. Highest-impact first slice

**Change (b), the student-safety approval card.** It is the smallest, highest-value step: one screen,
no new transport (`threadsAnswer` already exists), no new component (BoardUI `Button`/`Badge`/
`Tooltip` are installed), and it directly reduces the risk of an unchecked reply reaching a child.
Test-first: write `REQ-007 ... names the exact action` and extend
`spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant consequence sentence, the
explicit send label, and the text+icon student-facing marker. Defer (a), (c), (d), (e) until the
owner assigns REQ ids and approves.
