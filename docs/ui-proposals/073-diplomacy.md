# UI proposal 073 — diplomacy

Status: PROPOSAL. Sector framing is GUESS (spec is silent on this vertical); file, token, component
and RPC facts below are CONFIRMED by reading. Sector specialist #073. Read-only on product code;
this file is the only write.
Grounding (CONFIRMED): `docs/UI.md`, `spec/08-ui-ux.md`, `apps/web/lib/api/rpc.ts`, and two components —
`apps/web/components/thread/thread-composer.tsx` and
`apps/web/components/group/group-thread-screen.tsx`. Format follows `docs/ui-proposals/001-healthcare-clinics.md`.
Charter (CONFIRMED): Boma Bot only, fresh design (ADR-015), owner-supplied robot-cat as sole identity;
no upstream/OpenBot brand, code, assets or wording; BoardUI semantic tokens only (no raw palette/hex),
`cx()` to merge, dark mode via tokens; one design system, no second theme; self-hosted, no SaaS.

## 1. Persona and top jobs-to-be-done

**Persona — Ms. Layla Haddad, a desk officer in a foreign ministry's political department.** She works
from an embassy or ministry machine with Boma Bot self-hosted on it: cable and démarche records never
leave the building, and there is no cloud tenant. She reads and writes English, French and Arabic in
one day. A wrong recipient, or an outgoing note sent before clearance, is an international incident —
ambiguity is worse than latency. She is not a technologist.

Top jobs:
1. **Draft and clear** — compose a note or démarche with the bot and see at a glance whether it is
   still a draft, awaiting clearance, or cleared to send.
2. **Approve safely** — before a note leaves, read one plain sentence of what will happen and confirm
   with an explicit, named action; never decide on a colour cue.
3. **Multilateral** — follow a negotiation room, know which delegation/bot is speaking and who holds
   the floor, and hand the floor over cleanly.
4. **Triage** — across several postings, find incoming items awaiting a reply or clearance, snooze the
   non-urgent, and know which drafts are still working.
5. **Hand over and publish** — pass an open negotiation to a colleague at shift change with an
   attributable record, and publish a cleared briefing to a link for a delegation.

## 2. Five concrete changes

All strings are new keys in `lib/i18n.ts` through `t(locale, key)` / `useTranslation`; no hardcoded
copy. All surfaces use installed BoardUI components and the tokens in `docs/UI.md`.

**(a) Clearance status strip — screen `/app/:botId` (thread header).**
A compact, read-only band above the transcript: chips "Draft", "Awaiting clearance", "Cleared", each
with a keyboard-focusable `Tooltip` carrying the detail (who cleared it, when). Built from BoardUI
`Chip` (`components/base/badges/chip.tsx`), `Tooltip`, `IconButton`; surfaces
`bg-background-secondary-default`, hairline `border-separator-border`, text `text-text-secondary` /
`text-text-primary`, icons `text-foreground-icon-secondary`, type `text-caption-1-semibold`.
Existing RPCs: `botsGet`, `threadsHead`, `threadsMessages`. Bot-produced content is untrusted — render
as text, never HTML.

**(b) "Before you send" approval card — screen `/app/:botId` (pending card, REQ-007/REQ-017).**
Make the "what will happen" sentence the dominant line: "This sends a démarche to <post> and records it
in the cable log." Add an explicit consequence summary and a distinct confirm label ("Send démarche",
not just "Approve"). High-impact cards use `border-border-error-default` + `text-text-error-primary`
**plus** an icon and the words "High impact" — never colour alone. BoardUI `Button`, `Badge`,
`Tooltip`; card surface `bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`.
Existing RPCs: `threadsAnswer`, `threadsReact`. Announce the card once through the polite live region
(spec §8.9); the transcript is not a live region.

**(c) Negotiation room with a named lead — screen `/app/g/:groupId` (REQ-010/REQ-067/REQ-068).**
Extend `GroupThreadScreen`: the member chips already name each bot (CONFIRMED, lines 162–173), so
promote the active-run row to a named "lead negotiator" band, keep per-message attribution, and reassign
by naming the new lead. Replace the raw `<input>` (line 240) with the installed `ThreadComposer` pattern
(react-aria `TextField`/`TextArea`) so focus and Enter/Shift+Enter match the single thread. Reassign
failure stays a `role="alert"` with text. Buttons/chips/icons are BoardUI `Button`, `Badge`, `Avatar`,
`RiArrowLeftLine`; tokens `bg-background-secondary-default`, `border-border-button-default`,
`text-text-tertiary`, `text-text-error-primary`. Existing RPCs: `groupsGet`, `groupsMessages`,
`groupsReassign`, `threadsSend`, `runsPause`, `runsResume`. Mirror the back arrow in RTL via `useDirection()`.

**(d) Incoming-item triage in the rail — screen `/app` left rail (REQ-009/REQ-011/REQ-012).**
Replace the bare unread number with a `Counter` plus a named `Chip` ("Awaiting clearance", "Working",
"Snoozed till 16:00"), and expose due-time detail via a focusable `Tooltip` (never hover-only). Group the
snoozed shelf by due time using the installed `date-picker`/`date-calendar` if a picker is added.
BoardUI `Counter`, `Chip`, `IconButton`; tokens `text-text-primary`, `text-text-secondary`,
`bg-background-tertiary-default`, `animate-number-fade` per `docs/UI.md`. Existing RPCs:
`threadsMarkRead`, `threadsMarkUnread`, `threadsSnooze`, `threadsUnsnooze`, `runsList`.

**(e) Cleared briefing on a shared link — screen `/p/:slugToken` (REQ-037).**
Reuse `SharedPageScreen`: a compiled briefing published for a delegation with no session and no chrome
beyond a small robot-cat footer. Source is an artifact (the version chain), rendered as text/images in a
bounded frame — never inject artifact HTML into the document. Unknown, revoked and expired tokens show
one identical "not available" screen; `noindex`, `no-referrer`, own CSP. Existing RPCs:
`artifactsGet`, `artifactsListVersions`; read path `GET /api/v1/pages/shared/{token}` (CONFIRMED in
`shared-page-screen.tsx:152`).

## 3. Accessibility, i18n and RTL

- English source keys in `lib/i18n.ts` via `t(locale, key)`; French/Hebrew are translations; a missing
  key falls back to English, never a blank or the key. No string in JSX.
- `DirectionProvider` sets locale/direction; the boot script writes `lang`/`dir`. Use logical utilities
  (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Mirror the back/reassign arrow
  via `useDirection()`; keep cable references, dates, URLs and codes LTR in `<bdi>`/`dir="auto"`.
- WCAG 2.2 AA: zero critical/serious axe findings in light, dark and RTL; 44 px `.boma-target`; visible
  `focus-visible:ring-2 focus-visible:ring-border-focus-ring` / `.boma-focus`; full keyboard path
  (clearance tooltip, approval confirm, reassign, snooze); status by text + icon, not colour; 360 px;
  `prefers-reduced-motion` (120–180 ms changes, no motion on streaming content).
- Announcements — status changes and card arrivals go through a polite live region; the transcript is
  not one. Flag for reconciliation: `group-thread-screen.tsx:207–209` puts `aria-live="polite"` on the
  `role="log"` transcript (CONFIRMED); move announcements to a dedicated region before shipping.
- Error copy (spec §8.9): one sentence of what happened, one of what to do; no ids, stack traces or
  provider text on screen (correlation id behind "details"). No démarche content is written to logs.

## 4. Playwright tests (REQ ids where they exist)

- `REQ-007 pending approval card names the exact send action, takes focus, and does not block reading`
- `REQ-017 approving a send is idempotent under replay`
- `REQ-001 send creates one optimistic message that is not duplicated`
- `REQ-010 group thread attributes each message and routes the mention by keyboard alone`
- `REQ-068 reassign names the new lead and announces the status through the polite live region`
- `REQ-009 unread divider moves and the snooze shelf returns the item on time`
- `REQ-012 every run status renders a distinct named chip with the right terminal action`
- `REQ-037 shared briefing renders without chrome and unknown or revoked tokens show one uniform screen`
- `REQ-057 Hebrew RTL loads dir=rtl with a clean axe run`
- The clearance strip and high-impact marker above have no REQ yet: the owner must assign ids before
  implementation — do not invent ids in test names.

## 5. Highest-impact first slice

**Change (b), the "Before you send" approval card.** It is the smallest, highest-value step: one screen,
no new transport (`threadsAnswer` already exists), no new component (BoardUI `Button`/`Badge`/`Tooltip`
are installed), and it directly reduces the risk of an unsanctioned démarche. Test-first: write
`REQ-007 ... names the exact send action` and extend `spec/golden/ui/REQ-007-pending-card.json` first;
ship only the dominant consequence sentence, the explicit "Send démarche" label, and the text+icon
high-impact marker. Defer (a), (c), (d), (e) until the owner assigns REQ ids and approves.
