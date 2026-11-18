# UI proposal 023 — logistics (self-hosted freight and fulfilment desk)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #023 — logistics. Read-only on
product code; only this file is written. Grounding read this session: `docs/UI.md`,
`spec/08-ui-ux.md`, `apps/web/lib/api/rpc.ts` (RPC names CONFIRMED), and two components —
`apps/web/components/activity/activity-screen.tsx` (`ActivityScreen`) and
`apps/web/components/board/board-screen.tsx` (`BoardScreen`). REQ ids checked against
`docs/UI.md` and the `spec/golden/ui/` table.

Charter held: Boma Bot only; no upstream brand, code, assets or wording; BoardUI semantic tokens
only — no raw palette class or hex literal, classes merged with `cx()`; one design system, no second;
no SaaS (self-hosted on the operator's machine, no shipment or customer data leaving the building);
robot-cat identity and product name unchanged.

## 1. Persona and top jobs-to-be-done

**Persona — Yusuf, dispatch coordinator at a regional third-party logistics provider.** He books
carriers, tracks loads, handles exceptions (delay, damage, shortage) and files proof of delivery.
Drivers, warehouse staff and customers work in English, French and Arabic. Boma Bot runs on the
company's own machine: customer addresses, routes and commercial rates never leave the premises.
A wrong or ambiguous dispatch costs a missed pickup window and a chargeback, so legible state and an
explicit "what will happen" matter more than a fast screen.

Top jobs:
1. **Dispatch and track** — see which loads are queued, booked, in transit and delivered, and which
   automation runs are still working, without scrolling a transcript.
2. **Verify before committing** — confirm a bot-proposed carrier booking knowing the exact route,
   carrier, cost and pickup window; never approve on colour alone.
3. **Triage exceptions** — find delays, damages and unread items, tell working runs from stalled
   ones, and snooze non-urgent alerts with a named due time.
4. **Supervise carrier handoff** — take over a legacy transport system when it needs manual entry,
   and leave an attributable handoff record.
5. **Prove and archive** — keep versioned proof-of-delivery / bill-of-lading artifacts, open each in
   the thread where it was produced, and never render untrusted content in the app document.

## 2. Five concrete changes

All new copy is catalogued in `lib/i18n.ts` through `t(locale, key)` (English source, French/Hebrew
translations, a missing key falls back to English — never blank or the raw key). Every colour rides
the `docs/UI.md` token map, type uses composite utilities, and every class list goes through `cx()`.

**(a) Shipment context strip — screen `/app/:botId` (thread header).** A compact, read-only band
above the transcript with chips for "in transit", "awaiting dispatch" and "exceptions", each with a
`Tooltip` carrying the detail (load count, next pickup). Components: `Badge`/`Chip`
(`components/base/badges/chip.tsx`), `Tooltip`, `IconButton` (icons from `@remixicon/react` passed as
references). Tokens: surface `bg-background-secondary-default`, hairline `border-separator-border`,
text `text-text-primary`/`text-text-secondary`, icon `text-foreground-icon-secondary`, type
`text-caption-1-semibold`. Existing RPCs: `botsGet`, `threadsHead`, `runsList`. Bot-produced values
are untrusted text, never HTML; tracking numbers and pickup times stay LTR in `<bdi>`.

**(b) Dispatch approval card — screen `/app/:botId` (pending card, REQ-007/REQ-017).** Make "what
will happen" the dominant line: exact route, carrier, cost and pickup window, with an explicit
confirm label ("Book carrier DHL, 14:00 pickup", not just "Approve"). High-impact bookings pair
`border-border-error-default` + `text-text-error-primary` with a leading icon and the words
"High impact" — never colour alone. Components: `Button`, `Badge`, `Tooltip`; card surface
`bg-background-secondary-default`, `rounded-3xl`, `border-separator-border`, focus
`focus-visible:ring-border-focus-ring`. Existing RPCs: `threadsAnswer` (exactly-once effect, keeps
the `CONFLICT` guard for double answers), `threadsReact`. Reuse `ThreadComposer`'s
`awaitingApproval` prop so the transcript stays readable and other threads reachable; the card is
announced once through the polite live region, not the transcript (spec §8.9).

**(c) Named exception ledger — screen `/app` activity (`activity/activity-screen.tsx`, REQ-011/
REQ-012).** Extend `ActivityScreen` so every logistics run row names its state out loud and adds a
plain-language logistics caption (`queued` → "dispatch queued", `running` → "in transit",
`waiting_takeover` → "awaiting carrier handoff", `failed` → "exception") — the canonical §8.9
vocabulary stays, the caption is secondary text, never a colour-only chip. Working/paused rows keep
Pause/Resume and failed rows keep Retry. Components: `Button`, `Badge`, `Chip`, `Tooltip`; surface
`bg-background-primary-default`, text `text-text-primary`/`text-text-secondary`, status via
`runStatusKey`/`isRetryable` from `lib/run-status.ts`, type `text-caption-1-semibold`. Existing
RPCs already used in the component: `runsList`, `runsPause`, `runsResume`, `runsRetry`, plus
`threadsHead`. Background tracking runs appear here, not in the transcript (spec §8.4).

**(d) Carrier-handoff takeover panel — screen `/app/:botId` right panel (REQ-026/REQ-028/
REQ-029).** Give each computer state a named status (starting, running, awaiting takeover, released)
and a plain caption above the controls: "Leaving this screen pauses nothing — the handoff record
stays open." Takeover and release are explicit, attributable actions. Components: `Button`, `Badge`
and the installed spinner/skeleton states; surface `bg-background-secondary-default`, text
`text-text-secondary`, status as text + icon. Existing RPCs: `computerStatus`, `computerTakeover`,
`computerRelease`, `computerOpenScreen`. Mirror drawer motion in RTL; leave physical screen
coordinates untouched; no customer data in logs.

**(e) Load board and proof-of-delivery archive — screens `/app/board` (REQ-038) and
`/app/artifacts` (REQ-036).** In `board-screen.tsx`, retitle the four flag-gated `COLUMNS` as booked
→ picked → in transit → delivered, each card showing load key, title, assigned bot and last comment;
the named detail panel shows attached tracking runs. Components already used there: `Button`,
`input`, `select`, installed `table`/`chip`; surface `bg-background-primary-default`, hairline
`border-separator-border`, status text + icon. RPCs: `boardsGet`, `ticketsCreate`, `ticketsMove`,
`ticketsComment`. In the artifact grid/viewer, show POD/BOL with a version selector, download and
"open in thread" via `artifactsListSpace`, `artifactsGet`, `artifactsListVersions`. Content is
untrusted — never inject artifact HTML into the document. With the flag off the board route is
absent, not locked (REQ-038).

## 3. Accessibility, i18n and RTL

- All visible strings go through `t(locale, key)` (English source, French/Hebrew translations, a
  missing key falls back to English, never blank or key). No string hardcoded in JSX.
- `DirectionProvider` takes the explicit BCP 47 locale; the boot script writes `lang`/`dir` together.
  Use logical utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`) and
  `useDirection()` to mirror takeover arrows and drawer motion in RTL. Load keys, tracking numbers,
  pickup times, money and URLs stay LTR in `<bdi>`/`dir="auto"`, money in integer minor units via
  `Intl` (ADR-023).
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px `.boma-target`
  on touch controls; token-driven visible focus; a keyboard path for every decision (booking, snooze,
  takeover, board move, version select); run status and high-impact markers are text + icon, never
  colour alone (spec §8.9); 360 px width; `prefers-reduced-motion` honoured; no animation on
  streaming content.
- Error copy is one sentence of what happened plus one of what to do — no tracking ids, addresses,
  rates, stack traces or provider text on screen (correlation id behind "details" only, spec §8.9).
  The high-impact marker is text + icon and the booking decision is announced once via the polite
  live region; the transcript is not a live region, so streaming tokens are not read aloud.

## 4. Playwright tests (REQ ids in the names)

```
test("REQ-001 the dispatch composer sends one optimistic message that is not duplicated", ...)
test("REQ-007 a pending dispatch card takes focus and does not block reading or other threads", ...)
test("REQ-009 the exception divider moves and the snooze shelf returns an alert on time", ...)
test("REQ-011 the activity ledger names each logistics run state with a plain caption", ...)
test("REQ-012 every run status renders a distinct named chip with the right terminal action", ...)
test("REQ-017 a high-impact carrier booking names route, carrier, cost and answers once", ...)
test("REQ-026 the handoff panel names starting, running, takeover and released states", ...)
test("REQ-028 takeover and release announce their status through the polite live region", ...)
test("REQ-036 the proof-of-delivery viewer selects a version and opens it in its thread", ...)
test("REQ-038 the load board moves a ticket and records a comment through the same RPCs", ...)
test("REQ-057 the logistics screens mirror for Hebrew with dir=rtl and a clean axe run", ...)
```

New behaviour without a REQ yet (shipment context strip, high-impact booking marker, handoff
caption, logistics captions on canonical chips) must be assigned a REQ id by the owner before code —
do not invent one in a test name per the traceability gate.

## 5. Highest-impact first slice

**Change (b), the dispatch approval card.** It is the cheapest and the only change that directly
reduces financial risk: `threadsAnswer`, `Button` and `Badge` are installed, the semantic tokens
exist, and no new RPC, component install, data model or token re-tune is needed. Test-first: write
`REQ-017 a high-impact carrier booking names route, carrier, cost and answers once` and extend
`spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant consequence sentence, the
explicit confirm label, the text+icon high-impact marker, keyboard operability and the polite
announcement. Defer (a), (c), (d), (e) until the owner assigns REQ ids and approves.

Provenance: RPC names and the `CONFLICT` guard CONFIRMED from `apps/web/lib/api/rpc.ts`; component
structure (`ActivityScreen`, `BoardScreen`, `ThreadComposer.awaitingApproval`) CONFIRMED by reading
the files this session; REQ ids and golden names CONFIRMED from `docs/UI.md` and
`spec/golden/ui/`. The persona, its jobs and the logistics mapping are GUESS (proposal only — no
logistics sector or REQ exists in the spec). No product code was modified.
