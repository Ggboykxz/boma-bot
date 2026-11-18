# UI proposal 039 — renewable energy (generation, storage and sustainability operations)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #039 — renewable energy.
Read-only on product code; this file is the only write. Grounding (CONFIRMED, read this session):
`docs/UI.md`, `spec/08-ui-ux.md`, `apps/web/components/thread/thread-composer.tsx`,
`apps/web/components/shell/landing-screen.tsx`, `apps/web/lib/api/rpc.ts`, plus installed board/activity/
artifacts/chip/input/select components under `apps/web/components/`.
No upstream brand, code, assets or wording consulted or copied.

Charter (CONFIRMED): Boma Bot only, fresh design (ADR-015), the owner-supplied robot-cat as the sole
identity; no upstream brand/code/assets/copy; BoardUI semantic tokens only — no raw palette class or
hex literal, every class list merged with `cx()`; one design system, no second library or hand-built
lookalike; self-hosted on the operator's own machine — no SaaS/cloud, no generation, meter, turbine,
curtailment or PPA data leaving the premises.

## 1. Persona and top jobs-to-be-done

**Persona — "the asset-performance lead"**: a renewable-energy operations supervisor at an independent
power producer or a site with solar PV, wind and battery storage. She works across English, French and
Arabic, often outdoors or in a small control room, on a wall display and a handheld. Boma Bot is
installed on the operations network: interval generation, inverter/string faults, state of charge,
curtailment orders, guarantee-of-origin and REC records never leave the building. She is measured on
yield and safety: approving a curtailment or a storage setpoint against the wrong asset, or on a colour
cue, is a real loss of generation, so ambiguity is worse than latency. The robot-cat mark is the only
identity art — no producer, turbine or panel logo is borrowed.

Top jobs:
1. **See the live picture** — one band of generation, forecast-vs-actual, storage state of charge and
   open asset alarms on the thread, without hunting the rail.
2. **Authorise dispatch** — review a bot-proposed curtailment, storage charge/discharge or turbine
   setpoint and know the asset, amount in MW, window and consequence before approving; never decide
   by colour alone.
3. **Track field work** — move fault and maintenance tickets from reported through diagnosed and
   dispatched to restored, keeping the history.
4. **Name run state** — state every forecast, dispatch or reporting run out loud using the §8.9
   vocabulary; retry only failures, pause/resume queued work.
5. **Record and share** — keep interval files, meter reads, RECs/guarantees of origin, PPA schedules
   and audit reports versioned, and publish a compiled sustainability summary to a regulator, offtaker
   or auditor by link with no login or leakage.

## 2. Five concrete UI changes

All copy is a new `lib/i18n.ts` key via `useTranslation` / `t(locale, key)`; no string in JSX. Every class list goes through `cx()`; every colour rides a `docs/UI.md` token.

**(a) Generation and storage status strip — thread `/app/:botId` (header).**
A compact, read-only band above the transcript: named chips for generation, forecast-vs-actual delta,
storage state of charge and open asset alarms, plus a small output trend using the `chart-1`…`chart-5`
tokens with `-active` variants. BoardUI `Chip`/`Badge`, `Tooltip`, `IconButton`
(`components/base/badges/chip.tsx`, `components/base/buttons/icon-button.tsx`); surface
`bg-background-secondary-default`, hairline `border-separator-border`, text
`text-text-secondary`/`text-text-primary`, icons `text-foreground-icon-secondary`, type
`text-caption-1-semibold`. RPCs (CONFIRMED): `botsGet`, `threadsHead`, `threadsMessages`. Bot-produced
values are untrusted — render as text, never HTML; keep asset and meter ids out of logs. Measured
chart geometry stays LTR.

**(b) Curtailment / storage-dispatch approval card — thread `/app/:botId` (pending card, REQ-007/REQ-017).**
Make the "what will happen" sentence dominant; add asset, amount in MW, window and a distinct confirm
label ("Curtail Array B", not just "Approve"). High-impact actions (curtailment, storage setpoint,
turbine stop) pair `border-border-error-default` + `text-text-error-primary` with an icon and the word
"High impact" — never colour alone. BoardUI `Button`, `Badge`, `Tooltip`; card
`bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`. RPCs (CONFIRMED):
`threadsAnswer`, `threadsReact`. Transcript stays readable, other threads reachable, focus lands on
the card; announce once via the polite live region (spec §8.9), the transcript is not a live region.

**(c) Asset fault and maintenance board — `/app/board` (REQ-038, feature-flagged).**
Reuse `BoardScreen` (`components/board/board-screen.tsx`): columns `backlog/todo/in_progress/done`
read as reported / diagnosed / dispatched / restored, with ticket key, title, assignee bot and last
comment; dispatching a ticket to a bot queues work visible here, not in chat. Status is text + icon,
never colour alone. Prefer installed BoardUI `Input`/`Select` over the raw fields in `board-screen.tsx`,
merged with `cx()`; tokens `bg-background-primary-default`, `border-border-button-default`,
`text-text-primary`/`text-text-secondary`, focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`.
RPCs (CONFIRMED): `boardsGet`, `botsList`, `ticketsCreate`, `ticketsMove`, `ticketsComment`. A 404 keeps
the route absent, not locked (spec §8.6).

**(d) Forecast and dispatch-run triage — activity view (REQ-011/REQ-012/REQ-067).**
Follow `ActivityScreen` (`components/activity/activity-screen.tsx`): group generation-forecast,
dispatch and reporting runs by day, with named status chips (queued, leased, running, waiting_input,
waiting_takeover, completed, failed, cancelled) via `runStatusKey` text + icon; pause/resume queued
work, retry only `failed`. BoardUI `Button`, `Chip`; `border-border-button-default`,
`bg-background-primary-default`, `text-text-primary`/`text-text-secondary`, and
`border-border-error-default` + `text-text-error-primary` + `text-foreground-icon-error` on the error
row. RPCs (CONFIRMED): `runsList`, `runsRetry`, `runsPause`, `runsResume`. Working runs sort to the top
and update in place. Alarm arrivals use the existing `notifications.*` policy surface (REQ-064).

**(e) Renewable-data library and shared sustainability report — `/app/artifacts` and `/p/:slugToken` (REQ-036/REQ-037).**
Reuse `ArtifactsGrid` (`components/artifacts/artifacts-grid.tsx`): name interval files, meter reads,
REC/guarantee-of-origin records, PPA schedules and audit reports by type, filter by type/bot, search by
name, show the version chain and "open in thread". Content is untrusted — render as data, never HTML in
the document. BoardUI `Input`, `Select`, `Chip`, `Button`; tokens
`text-text-primary`/`text-text-secondary`/`text-text-tertiary`, `bg-background-primary-default`,
`border-border-button-default`. RPCs (CONFIRMED): `artifactsListSpace`, `artifactsGet`,
`artifactsListVersions`. Publish a compiled sustainability summary to a regulator, offtaker or auditor
with no session and no product chrome beyond a small footer with the robot-cat mark; unknown, revoked
and expired tokens render one identical "not available" screen — same words, shape and timing.
`noindex`/`no-referrer` and its own CSP stay backend concerns. Transport (CONFIRMED):
`GET /api/v1/pages/shared/{token}`. Keep the robot-cat empty state when nothing is filed.

## 3. Accessibility

WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; keyboard-only path for
every primary flow (approve, board moves, pause/resume, retry, filters); visible
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target` for gloved or
field use; 360 px width; `prefers-reduced-motion` honoured; no animation on streaming content. Alarm
severity and run status are text + icon, using the exact §8.9 vocabulary — no colour-only asset
alarms. Tables carry `<caption>` and `<th scope>`. Focus lands on a pending card; the uniform
unavailable screen is announced in identical text — no leak through the accessibility tree. Error copy
is one sentence of what happened plus one of what to do — no asset ids, meter ids, stack traces or
provider text on screen (correlation id behind "details" only).

## 4. i18n and RTL

English is the source catalogue, fr/he the translations; a missing key falls back to English — never a
blank or the raw key. `DirectionProvider` (`@/components/foundations/direction/direction`) gets an
explicit BCP 47 locale; the boot script writes `lang` and `dir` together. RTL mirrors the board's columns,
the rail and the drawer motion via `useDirection()`; arrows flip. Logical utilities throughout
(`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Asset and meter ids, MW and
currency values, timestamps, URLs and OTP digits stay LTR in `<bdi>`/`dir="auto"`; measured chart
geometry is never mirrored. Formatting application data is separate from layout direction.

## 5. Playwright tests (REQ id in each name)

- `test("REQ-001 sending a dispatch instruction creates one optimistic message that is not duplicated", ...)`
- `test("REQ-007 the curtailment card names asset, MW, window, consequence and takes focus without blocking reading", ...)`
- `test("REQ-017 approve and deny answer the dispatch card exactly once with an attributable confirmation", ...)`
- `test("REQ-038 the asset board shows text-and-icon status columns and moves a ticket by keyboard", ...)`
- `test("REQ-038 the board route is absent, not locked, when the flag returns 404", ...)`
- `test("REQ-011 the activity view replaces the list and groups forecast and dispatch runs by day", ...)`
- `test("REQ-012 every run status renders a distinct named chip with the right terminal action", ...)`
- `test("REQ-067 pause and resume announce their status through the polite live region", ...)`
- `test("REQ-036 the renewable-data library filters by type and bot and shows the version chain", ...)`
- `test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)`
- `test("REQ-057 Hebrew RTL mirrors the board and axe reports zero critical or serious findings", ...)`

Goldens under `spec/golden/ui/` stay byte-exact; a new state means a new `REQ-<id>-<slug>.json` asserting
seeded state, never a click path. New behaviour without a REQ (status strip, high-impact marker, day
grouping) needs an owner-assigned id before code — do not invent one in a test name.

## 6. Best first slice

**Change (b), the curtailment / storage-dispatch approval card** — GUESS, recommended. It is the
smallest, highest-value step: one screen, no new transport (`threadsAnswer`/`threadsReact` exist), no
new component (BoardUI `Button`/`Badge`/`Tooltip` are installed) and no token re-tune. It directly
prevents a curtailment or storage setpoint being confirmed on a colour cue or against the wrong asset.
Test-first: write `REQ-007 ... names asset, MW, window, consequence and takes focus`, extend
`spec/golden/ui/REQ-007-pending-card.json`, then ship only the dominant consequence line, the explicit
"Curtail Array B" label and the text+icon high-impact marker. Defer (a), (c), (d), (e) until the owner
assigns any missing REQ ids and approves scope.

**Decision needed from the owner**: confirm the sector framing and whether REQ-038's feature flag is in scope before (c).
