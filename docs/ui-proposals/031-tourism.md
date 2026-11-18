# UI proposal 031 — tourism (self-hosted travel operations desk)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #031 — tourism. Read-only on
product code; only this file is written. Grounding read this session: `docs/UI.md`,
`spec/08-ui-ux.md`, `apps/web/lib/api/rpc.ts` (procedure names CONFIRMED), and two components —
`apps/web/components/base/date-picker/date-picker.tsx` (`DatePicker`) and
`apps/web/components/board/board-screen.tsx` (`BoardScreen`). REQ ids and golden names checked
against `spec/01-requirements.md`, `docs/UI.md` and `spec/golden/ui/`.

Charter held: Boma Bot only; no upstream brand, code, assets or wording; BoardUI semantic tokens only
— no raw palette class or hex literal, classes merged with `cx()`; one design system, no second; no
SaaS (self-hosted on the operator's machine — traveller names, itineraries and prices never leave the
building); robot-cat identity and product name unchanged.

## 1. Persona and top jobs-to-be-done

**Persona — Amira, operations lead at a regional tour operator.** She drafts itineraries, confirms
supplier bookings (flights, rooms, transfers), watches departure dates and traveller exceptions, and
archives vouchers and tickets. Guides and travellers work in English, French and Arabic. Boma Bot
runs on the company's own machine: passenger lists, passport-adjacent details and commercial rates
never leave the premises. A wrongly confirmed or unbilled booking costs a non-refundable loss, so a
legible "what will happen" and an explicit decision matter more than a fast screen. The robot-cat mark
is the only identity — never a supplier, airline or payment-brand logo, never borrowed copy.

Top jobs:
1. **Plan** — draft and revise day-by-day itineraries with firm departure/return windows, without
   hunting through the rail.
2. **Confirm safely** — approve a bot-proposed booking knowing the exact itinerary, supplier, total
   price and cancellation terms; never approve on colour alone.
3. **Meet dates** — track upcoming departures and working runs, tell a working quote from a stalled
   one, and snooze non-urgent follow-ups with a named due time.
4. **Handle exceptions** — take over a legacy reservation system when it needs manual entry, leaving
   an attributable handoff record.
5. **Prove and archive** — keep versioned vouchers, tickets and itineraries, open each in the thread
   where it was produced, and never render untrusted document content in the app document.

## 2. Five concrete changes

All new copy is catalogued in `lib/i18n.ts` through `t(locale, key)` (English source, French/Hebrew
translations, a missing key falls back to English — never blank or the raw key). Every colour rides
the `docs/UI.md` token map, type uses composite utilities, and every class list goes through `cx()`.
Bot-, supplier- and document-produced content is untrusted text, never HTML.

**(a) Trip context strip — screen `/app/:botId` (thread header).** A compact read-only band above the
transcript with chips for "bookings pending", "travellers" and "next departure", each with a
keyboard-focusable `Tooltip` carrying detail (trip name, headcount, date). Components: `Badge`/`Chip`
(`components/base/badges/chip.tsx`), `Tooltip`, `IconButton` (icons from `@remixicon/react` passed as
references). Tokens: surface `bg-background-secondary-default`, hairline `border-separator-border`,
text `text-text-primary`/`text-text-secondary`, icon `text-foreground-icon-secondary`, type
`text-caption-1-semibold`. Existing RPCs: `botsGet`, `threadsHead`, `runsList`. Dates, IATA codes and
confirmation numbers stay LTR in `<bdi>`.

**(b) Booking approval card — screen `/app/:botId` (pending card, REQ-007/REQ-017).** Make "what will
happen" the dominant line: exact itinerary, supplier, total price and cancellation terms, with an
explicit confirm label ("Confirm booking, non-refundable" — not just "Approve"). Non-refundable and
high-value bookings pair `border-border-error-default` + `text-text-error-primary` with a leading icon
and the words "High impact" — never colour alone. Components: `Button`, `Badge`, `Tooltip`; card
surface `bg-background-primary-default`, `rounded-3xl`, `border-separator-border`, focus
`focus-visible:ring-border-focus-ring`. Existing RPCs: `threadsAnswer` (exactly-once effect, keeps the
`CONFLICT` guard for double answers), `threadsReact`. Reuse `ThreadComposer`'s `awaitingApproval`
prop so the transcript stays readable and other threads reachable; the card is announced once through
the polite live region, not the transcript (spec §8.9).

**(c) Travel-date window — screen `/app/:botId` (composer panel).** Compose two installed
`DatePicker` instances (`base/date-picker/date-picker.tsx`, react-aria `Calendar` + `MonthPanel`,
editable date chip, Cancel/Apply) as departure and return, with a named tooltip and a validation line
when return precedes departure. Components: `DatePicker`, `Button`, `Badge`; icon
`text-foreground-icon-primary`, type `text-body-medium`. The values are local state and keep the
existing `threadsSend` transport — no new RPC; format with `Intl` and keep the `DD/MM/YYYY` chip LTR.

**(d) Booking pipeline board — screen `/app/board` (REQ-038).** In `board-screen.tsx` retitle the
four flag-gated `COLUMNS` as enquiry → quoted → booked → travelled; each card shows a trip reference,
traveller group, assigned bot and last comment; the named detail panel shows attached run states with
their canonical vocabulary plus comments. Components already used there: `Button`, `input`, `select`,
the modal detail dialog; surface `bg-background-primary-default`, hairline `border-separator-border`,
status text + icon. Existing RPCs: `boardsGet`, `botsList`, `ticketsCreate`, `ticketsMove`,
`ticketsComment`. With the flag off the route is absent, not locked (REQ-038).

**(e) Travel-document archive — screens `/app/artifacts` and `/app/artifacts/:artifactId` (REQ-036).**
In the grid/viewer, show vouchers, tickets and itineraries with a type chip, a version selector,
download and "open in thread". Components: installed `table`/`card`/`chip` patterns; surface
`bg-background-primary-default`, text `text-text-primary`/`text-text-secondary`, type
`text-body-medium`. Existing RPCs: `artifactsListSpace`, `artifactsGet`, `artifactsListVersions`.
Document content is untrusted: render bounded, as text — never inject HTML into the app document.

## 3. Accessibility, i18n and RTL

- All visible strings go through `t(locale, key)` (English source, French/Hebrew translations, a
  missing key falls back to English, never blank or key). No string hardcoded in JSX.
- `DirectionProvider` takes the explicit BCP 47 locale; the boot script writes `lang`/`dir` together.
  Use logical utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`,
  `rounded-s`/`rounded-e`) and `useDirection()` to mirror the board's disclosure arrows and any drawer
  motion in RTL. IATA codes, confirmation numbers, dates, prices and URLs stay LTR in `<bdi>`/`dir="auto"`.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px `.boma-target` on
  touch controls; token-driven visible focus; a keyboard path for every decision (booking, date apply,
  board move, version select, takeover); run status and "High impact" are text + icon, never colour
  alone (spec §8.9); 360 px width; `prefers-reduced-motion` honoured; no animation on streaming content.
  The `DatePicker` popover keeps both chevrons and its editable chip operable by keyboard.
- Money uses `Intl.NumberFormat` with an explicit currency and integer minor units, never a float; a
  negative value is sign + icon, not red alone. Traveller names and confirmation numbers never reach a
  hover-only tooltip, logs or another bot's view. Error copy is one sentence of what happened plus one
  of what to do — no IDs, stack traces or provider text on screen (correlation id behind "details",
  spec §8.9). Run-status vocabulary stays exactly `queued | leased | running | waiting_input |
  waiting_takeover | completed | failed | cancelled` (spec §8.9).

## 4. Playwright tests (REQ id in the name)

```
test("REQ-001 the itinerary composer sends one optimistic message that is not duplicated", ...)
test("REQ-007 a pending booking card takes focus and does not block reading or other threads", ...)
test("REQ-017 confirm booking names itinerary, supplier, price and terms and answers exactly once", ...)
test("REQ-012 every run status renders a distinct named chip with the right terminal action", ...)
test("REQ-036 the voucher viewer selects a version and opens it in its thread", ...)
test("REQ-038 the booking board moves a trip and records a comment through the same RPCs", ...)
test("REQ-057 tourism screens mirror for Hebrew with dir=rtl, keep dates LTR and axe clean", ...)
```

New behaviour without a REQ yet (trip context strip, travel-date window) must be assigned a REQ id by
the owner before code — do not invent one in a test name per the traceability gate. Every golden under
`spec/golden/ui/` stays byte-exact; a new seeded state means a new `REQ-<id>-<slug>.json`, never a
click path.

## 5. Best first slice

**Change (b), the booking approval card** — GUESS, recommended. It is the cheapest and the only change
that directly reduces financial risk: `threadsAnswer`, `Button` and `Badge` are installed, the semantic
tokens exist, and no new RPC, component install, data model or token re-tune is needed. Test-first:
write `REQ-017 confirm booking names itinerary, supplier, price and terms and answers exactly once`
and extend `spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant consequence
sentence, the explicit confirm label, the text+icon high-impact marker, keyboard operability and the
polite announcement. Defer (a), (c), (d), (e) until the owner assigns REQ ids and approves.

Provenance: RPC names CONFIRMED from `apps/web/lib/api/rpc.ts`; component structure (`DatePicker`,
`BoardScreen` and their RPC use) CONFIRMED by reading the files this session; REQ ids and golden names
CONFIRMED from `spec/01-requirements.md`, `docs/UI.md` and `spec/golden/ui/`. The persona, its jobs and
the tourism mapping are GUESS (proposal only — no tourism sector or REQ exists in the spec). No product
code was modified.
