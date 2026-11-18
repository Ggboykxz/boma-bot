# UI proposal 029 — hospitality (guest services on the operator's own machine)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #029 — hospitality. Read-only on
product code; only this file is written. Grounding read this session: `docs/UI.md`,
`spec/08-ui-ux.md`, `apps/web/lib/api/rpc.ts` (RPC names CONFIRMED), `apps/web/lib/run-status.ts`
(status vocabulary CONFIRMED), and two components — `apps/web/components/thread/thread-composer.tsx`
(`ThreadComposer`, `awaitingApproval` CONFIRMED) and `apps/web/components/board/board-screen.tsx`
(`BoardScreen`, `COLUMNS`, flag gate CONFIRMED). REQ ids and golden names checked against `docs/UI.md`
and `spec/golden/ui/`.

Charter held: Boma Bot / robot-cat identity and product name unchanged; no upstream brand, code,
assets or wording; BoardUI semantic tokens only — no raw palette class or hex literal, classes merged
with `cx()`; one design system, no second; no SaaS (self-hosted on the operator's machine — passports,
card details and guest PII never leave the building); no new RPC, component install or token re-tune.

## 1. Persona and top jobs-to-be-done

**Persona — Amira, guest-relations manager at a mid-size hotel group.** She confirms reservations and
room changes, handles check-in/out and guest requests (late checkout, upgrade, extra bed), routes
housekeeping and maintenance issues, and keeps guest ID scans and incident reports. Staff work in
English, French and Hebrew. Boma Bot runs on the hotel's own machine: guest PII and commercial rates
stay on premises. An ambiguous confirmation costs a double booking or a chargeback, so legible named
state and an explicit "what will happen" matter more than a fast screen.

Top jobs:
1. **Confirm or decline a change** — know the exact room, rate, dates and cancellation terms before a
   bot-proposed booking or upgrade takes effect; never approve on colour alone.
2. **Triage guest requests** — see which housekeeping/maintenance items are reported, assigned, in
   progress and resolved, and which automation runs are still working, without reading a transcript.
3. **Name every run state** — tell a queued request from a working one and a stalled one, and spot the
   failed confirmation that needs a retry before the guest arrives.
4. **Take over the legacy PMS** — when manual entry is needed, take over and release with an
   attributable handoff record, leaving no guest data in logs.
5. **Prove and archive** — keep versioned guest-document and incident-report artifacts, open each in
   the thread where it was produced, and never render untrusted content in the app document.

## 2. Five concrete changes

All new copy is catalogued in `lib/i18n.ts` through `t(locale, key)` / `useTranslation()` (English
source, French/Hebrew translations, missing key falls back to English — never blank or the raw key).
Every colour rides the `docs/UI.md` token map, type uses composite utilities, and every class list
goes through `cx()`.

**(a) Reservation context strip — screen `/app/:botId` (thread header).** A compact, read-only band
above the transcript with chips for "arriving today", "in house", "departing" and "open request",
each with a `Tooltip` carrying the detail (booking count, next arrival). Components: `Badge`/`Chip`
(`components/base/badges/chip.tsx`), `Tooltip` (`components/base/tooltip/tooltip.tsx`), `IconButton`
(icons from `@remixicon/react` passed as references). Tokens: surface `bg-background-secondary-default`,
hairline `border-separator-border`, text `text-text-primary`/`text-text-secondary`, icon
`text-foreground-icon-secondary`, type `text-caption-1-semibold`. Existing RPCs: `botsGet`,
`threadsHead`, `runsList`. Bot-produced values are untrusted text, never HTML; booking refs, room
numbers and dates stay LTR in `<bdi>`.

**(b) Booking/upgrade approval card — screen `/app/:botId` (pending card, REQ-007).** Make "what will
happen" the dominant line: exact room type, rate, dates and cancellation terms, with an explicit
confirm label ("Confirm upgrade to suite, 3 nights, free cancellation", not just "Approve"). A
non-refundable or charged change pairs `text-text-error-primary` with a leading icon and the words
"Chargeable change" — never colour alone. Components: `Button`, `Badge`, `Tooltip`; card surface
`bg-background-secondary-default`, `rounded-3xl`, `border-separator-border`, focus
`focus-visible:ring-border-focus-ring`. Existing RPCs: `threadsAnswer` (exactly-once effect, keeps the
`CONFLICT` guard for double answers), `threadsReact`. Reuse `ThreadComposer`'s `awaitingApproval` prop
(CONFIRMED in the file) so the transcript stays readable and other threads reachable; the card is
announced once through the polite live region, not the transcript (spec §8.9).

**(c) Named housekeeping ledger — screen `/app` activity (REQ-011/REQ-012).** Give each automation run
row its canonical state out loud plus a plain hospitality caption (`queued` → "request queued",
`running` → "in progress", `waiting_takeover` → "awaiting PMS handoff", `failed` → "needs attention").
The §8.9 vocabulary stays exact; the caption is secondary text, never a colour-only chip. Working/
paused rows keep Pause/Resume and failed rows keep Retry. Components: `Button`, `Badge`, `Chip`,
`Tooltip`; surface `bg-background-primary-default`, text `text-text-primary`/`text-text-secondary`,
status via `runStatusKey`/`isRetryable` from `lib/run-status.ts`, type `text-caption-1-semibold`.
Existing RPCs: `runsList`, `runsPause`, `runsResume`, `runsRetry`, plus `threadsHead`. Background
(ticket) runs appear here, not in the transcript (spec §8.4).

**(d) PMS handoff takeover panel — screen `/app/:botId` right panel (REQ-026/REQ-028/REQ-029).** Give
each computer state a named status (starting, running, awaiting takeover, released) and a plain
caption above the controls: "Leaving this screen pauses nothing — the handoff record stays open."
Takeover and release are explicit, attributable actions. Components: `Button`, `Badge` and the
installed spinner/skeleton states; surface `bg-background-secondary-default`, text
`text-text-secondary`, status as text + icon. Existing RPCs: `computerStatus`, `computerTakeover`,
`computerRelease`, `computerOpenScreen`. Mirror drawer motion in RTL; leave physical screen
coordinates untouched; put no guest PII in logs.

**(e) Housekeeping board and guest-document archive — screens `/app/board` (REQ-038) and
`/app/artifacts` (spec §8.5).** In `board-screen.tsx`, retitle the four flag-gated `COLUMNS`
(`backlog`, `todo`, `in_progress`, `done`) as reported → assigned → in progress → resolved, each card
showing room/guest reference, title, assigned bot and last comment; the named detail panel shows
attached runs. Components already used there: `Button`, `input`, `select`, installed `table`/`chip`;
surface `bg-background-primary-default`, hairline `border-separator-border`, status text + icon.
RPCs: `boardsGet`, `ticketsCreate`, `ticketsMove`, `ticketsComment`. In the artifact grid/viewer, show
ID scans and incident reports with a version selector, download and "open in thread" via
`artifactsListSpace`, `artifactsGet`, `artifactsListVersions`. Content is untrusted — never inject
artifact HTML into the document. With the flag off the board route is absent, not locked (REQ-038).

## 3. Accessibility, i18n and RTL

- All visible strings go through `t(locale, key)` / `useTranslation()` (English source, French/Hebrew
  translations, missing key falls back to English, never blank or key). No string hardcoded in JSX.
- `DirectionProvider` takes the explicit BCP 47 locale; the boot script writes `lang`/`dir` together.
  Use logical utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`) and
  `useDirection()` to mirror takeover arrows and drawer motion in RTL. Booking refs, dates, room
  numbers, money and URLs stay LTR in `<bdi>`/`dir="auto"`, money in integer minor units via `Intl`.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px `.boma-target`
  on touch controls; token-driven visible focus; a keyboard path for every decision (confirm, snooze,
  takeover, board move, version select); run status and "chargeable change" markers are text + icon,
  never colour alone (spec §8.9); 360 px width; `prefers-reduced-motion` honoured; no animation on
  streaming content.
- Error copy is one sentence of what happened plus one of what to do — no booking ids, passport or
  card data, rates, stack traces or provider text on screen (correlation id behind "details" only,
  spec §8.9). The booking decision is announced once via the polite live region; the transcript is
  not a live region, so streaming tokens are not read aloud.

## 4. Playwright tests (REQ ids in the names)

```
test("REQ-001 the guest-request composer sends one optimistic message that is not duplicated", ...)
test("REQ-007 a pending upgrade card takes focus and does not block reading or other threads", ...)
test("REQ-009 the request divider moves and the snooze shelf returns an alert on time", ...)
test("REQ-011 the housekeeping ledger names each automation run state with a plain caption", ...)
test("REQ-012 every run status renders a distinct named chip with the right terminal action", ...)
test("REQ-021 a failed confirmation row names the cause and offers one retry", ...)
test("REQ-026 the PMS handoff panel names starting, running, takeover and released states", ...)
test("REQ-028 takeover and release announce their status through the polite live region", ...)
test("REQ-038 the housekeeping board moves a ticket and records a comment through the same RPCs", ...)
test("REQ-057 the hospitality screens mirror for Hebrew with dir=rtl and a clean axe run", ...)
```

New behaviour without a REQ yet (reservation context strip, "chargeable change" marker, handoff
caption, hospitality captions on canonical chips) must be assigned a REQ id by the owner before code —
do not invent one in a test name per the traceability gate.

## 5. Highest-impact first slice

**Change (b), the booking/upgrade approval card.** It is the cheapest and the only change that
directly reduces financial and privacy risk: `threadsAnswer`, `Button` and `Badge` are installed, the
semantic tokens exist, and `ThreadComposer.awaitingApproval` is already in the tree — no new RPC,
component install, data model or token re-tune is needed. Test-first: write
`REQ-007 a pending upgrade card takes focus and does not block reading or other threads` and extend
`spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant consequence sentence, the
explicit confirm label, the text+icon "chargeable change" marker, keyboard operability and the polite
announcement. Defer (a), (c), (d), (e) until the owner assigns REQ ids and approves.

Provenance: RPC names and the `CONFLICT` guard CONFIRMED from `apps/web/lib/api/rpc.ts`; status
vocabulary and `runStatusKey`/`isRetryable` CONFIRMED from `apps/web/lib/run-status.ts`; component
structure (`ThreadComposer.awaitingApproval`, `BoardScreen.COLUMNS`, flag gate) CONFIRMED by reading
the files this session; REQ ids and golden names CONFIRMED from `docs/UI.md` and `spec/golden/ui/`.
The persona, its jobs and the hospitality mapping are GUESS (proposal only — no hospitality sector or
REQ exists in the spec). No product code was modified.
