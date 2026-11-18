# UI proposal 041 — automotive (fleet service and workshop desk)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #041 — automotive. Read-only on
product code; this file is the only write. Grounding (CONFIRMED, read this session): `docs/UI.md`,
`spec/08-ui-ux.md`, `apps/web/components/board/board-screen.tsx` (`BoardScreen`),
`apps/web/components/thread/computer-panel.tsx` (`ComputerPanel`), RPC names in `apps/web/lib/api/rpc.ts`,
REQ ids in `spec/01-requirements.md`, shared-page route in `spec/openapi.yaml`.
No upstream brand, code, assets or wording consulted or copied.

Charter (CONFIRMED): Boma Bot only, fresh design (ADR-015), the owner-supplied robot-cat as the sole
identity; no upstream brand/code/assets/copy; BoardUI semantic tokens only — no raw palette class or
hex literal, every class list merged with `cx()`; one design system, no second library; no SaaS —
self-hosted on the workshop's own machine, no VIN, telematics or customer record leaving the premises.

## 1. Persona and top jobs-to-be-done

**Persona — "the service lead"**: a fleet-maintenance or workshop supervisor who coordinates
technicians, diagnoses fault codes, schedules service and pushes updates to vehicles. She works in
English, French and Arabic, often gloved, between a desktop and a shop-floor tablet. Boma Bot runs on
the workshop's own machine: VINs, telematics, scan logs and customer details never leave it. A pushed
update aimed at the wrong vehicle, or a diagnostic handoff left in an unsafe state, is a real safety
and liability event, so legible state and an explicit "what will happen" matter more than a fast
screen. The robot-cat mark is the only identity art — no manufacturer or dealer logo is borrowed.

Top jobs:
1. **See the fleet pulse** — one band of active faults, charge/fuel, service-due counts and open runs
   on the thread, without hunting the rail.
2. **Verify before committing** — approve a bot-proposed OTA push, service booking, parts order or
   technician dispatch knowing the exact VIN, action, window and consequence; never approve on colour
   alone.
3. **Run the work order** — move a job from reported through diagnosed and in service to released,
   keeping the attached runs and comments with it.
4. **Supervise the handoff** — take over a remote diagnostic session when a bot needs a human at the
   controls, and leave an attributable record of who held it and when.
5. **Prove and share** — keep versioned scan logs, service history and reports, open each in the
   thread that produced it, and publish a compiled vehicle report by link with no login or leakage.

## 2. Five concrete UI changes

All new copy is a `lib/i18n.ts` key via `useTranslation` / `t(locale, key)`; every colour rides the
`docs/UI.md` token map, type uses composite utilities, and every class list goes through `cx()`.

**(a) Fleet-health strip — screen `/app/:botId` (thread header).** A compact, read-only band above
the transcript: named chips for active fault count, charge/fuel where known, service-due count and
open runs, plus a small fault trend using `chart-1`…`chart-5` tokens with `-active` variants.
Components: BoardUI `Badge`/`Chip` (`components/base/badges/chip.tsx`), `Tooltip`, `IconButton`
(icons from `@remixicon/react` passed as references). Tokens: surface `bg-background-secondary-default`,
hairline `border-separator-border`, text `text-text-primary`/`text-text-secondary`, icons
`text-foreground-icon-secondary`, type `text-caption-1-semibold`. RPCs (CONFIRMED): `botsGet`,
`threadsHead`, `threadsMessages`, `runsList`. Fault values are untrusted text, never HTML; VINs and
plate numbers stay out of logs and hover-only tooltips. Measured chart geometry stays LTR.

**(b) Service / OTA approval card — screen `/app/:botId` (pending card, REQ-007/REQ-017).** Make the
"what will happen" sentence dominant: exact VIN, action, window and consequence, with an explicit
confirm label ("Push update to fleet group B at 02:00", not just "Approve"). A high-impact action
pairs `border-border-error-default` + `text-text-error-primary` with a leading icon and the words
"High impact" — never colour alone. Components: `Button`, `Badge`, `Tooltip`; card surface
`bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`, focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`. RPCs (CONFIRMED): `threadsAnswer`
(keep the `CONFLICT` guard for double answers), `threadsReact`. Reuse `ThreadComposer`'s
`awaitingApproval` prop so the transcript stays readable and other threads reachable; the decision
is announced once through the polite live region (§8.9), the transcript is not a live region.

**(c) Work-order board — screen `/app/board` (REQ-038, feature-flagged).** Reuse `BoardScreen`:
retitle the four `COLUMNS` as reported → diagnosed → in service → released; each card shows work-order
key, vehicle/VIN, assigned technician bot and last comment, and the detail panel shows the attached
runs. Prefer installed BoardUI `Input`/`Select` over the screen's raw fields, merged with `cx()`;
tokens `bg-background-primary-default`, `border-separator-border`, `text-text-primary`/
`text-text-secondary`, focus via the focus-ring token. RPCs (CONFIRMED): `boardsGet`, `botsList`,
`ticketsCreate`, `ticketsMove`, `ticketsComment`. A flag-off `404` keeps the route absent, not locked
(§8.6). Status is text + icon, never colour alone.

**(d) Diagnostic handoff panel — screen `/app/:botId` right panel (REQ-026/REQ-028/REQ-029).** Give
each `ComputerPanel` state a named status (starting, running, awaiting handoff, released) with a
`role="status"` captioned line: "Leaving this screen pauses nothing — the handoff record stays open."
Takeover and release are explicit, attributable actions; status names use the exact §8.9 vocabulary.
Components: `Button`, `Badge`, installed spinner/skeleton states; surface
`bg-background-secondary-default`, text `text-text-secondary`, status as text + icon; action error
`text-text-error-primary` with a leading icon. RPCs (CONFIRMED): `computerStatus`, `computerTakeover`,
`computerRelease`, `computerOpenScreen`, `computerOpenScreenFrame`. Mirror drawer motion in RTL; leave
physical screen coordinates untouched; no vehicle data in logs.

**(e) Service-record library and shared report — screens `/app/artifacts` and `/p/:slugToken`
(REQ-036/REQ-037).** In `ArtifactsGrid`, name scan logs, service histories and reports by type, filter
by type/bot, search by name, show the version chain and "open in thread". Content is untrusted —
render as data, never inject artifact HTML into the document. Components: `Input`, `Select`, `Chip`,
`Button`; tokens `text-text-primary`/`text-text-secondary`/`text-text-tertiary`,
`bg-background-primary-default`, `border-border-button-default`. RPCs (CONFIRMED):
`artifactsListSpace`, `artifactsGet`, `artifactsListVersions`. Publish a compiled vehicle report with
no session and no product chrome beyond a small footer with the robot-cat mark; unknown, revoked and
expired tokens render one identical "not available" screen — same words, shape and timing. Transport
(CONFIRMED): `GET /api/v1/pages/shared/{token}`; `noindex`/`no-referrer` and its own CSP stay backend
concerns. Keep the robot-cat empty state when nothing is filed.

## 3. Accessibility

WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; a keyboard-only path for
every primary flow (approve, board moves, takeover/release, retry, filters); visible
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target` for gloved use;
360 px width; `prefers-reduced-motion` honoured; no animation on streaming content. Fault severity and
run status are text + icon using the exact §8.9 vocabulary — no colour-only warnings; tables carry
`<caption>`/`<th scope>`. Focus lands on a pending card; the unavailable screen is announced in
identical text. Error copy: one sentence of what happened plus one of what to do — no VIN, plate,
customer detail, stack trace or provider text (correlation id behind "details" only, §8.9).

## 4. i18n and RTL

English is the source catalogue, fr/he the translations; a missing key falls back to English — never
blank or the raw key. `DirectionProvider` (`@/components/foundations/direction/direction`) gets an
explicit BCP 47 locale; the boot script writes `lang`/`dir` together. RTL mirrors board columns, rail
and drawer motion via `useDirection()`. Logical utilities throughout (`ms`/`me`, `ps`/`pe`,
`text-start`/`text-end`, `border-s`/`border-e`). VINs, plates, odometer values, dates, money, URLs and
OTP digits stay LTR in `<bdi>`/`dir="auto"`; measured screen geometry and chart axes are never mirrored.

## 5. Playwright tests (REQ id in each name)

- `test("REQ-001 the service composer sends one optimistic message that is not duplicated", ...)`
- `test("REQ-007 a pending service card takes focus and does not block reading or other threads", ...)`
- `test("REQ-017 a high-impact approval names VIN, action, window and consequence and answers once", ...)`
- `test("REQ-011 the activity view groups diagnostic runs by day", ...)`
- `test("REQ-012 every run status renders a distinct named chip with the right terminal action", ...)`
- `test("REQ-026 the handoff panel names starting, running, awaiting handoff and released states", ...)`
- `test("REQ-028 takeover and release announce their status through the polite live region", ...)`
- `test("REQ-036 the service-record library filters by type and bot and shows the version chain", ...)`
- `test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)`
- `test("REQ-038 the work-order board moves a ticket by keyboard and is absent when the flag is off", ...)`
- `test("REQ-057 Hebrew RTL mirrors the board and axe reports zero critical or serious findings", ...)`

Goldens under `spec/golden/ui/` stay byte-exact; a new state means a new `REQ-<id>-<slug>.json`
asserting seeded state, never a click path. New behaviour without a REQ (fleet strip, high-impact
marker) needs an owner-assigned id before code — never invent one in a test name.

## 6. Best first slice

**Change (b), the service / OTA approval card** — GUESS, recommended. It is the smallest, highest-value
step: one screen, no new transport (`threadsAnswer`/`threadsReact` exist), no new component (BoardUI
`Button`/`Badge`/`Tooltip` are installed) and no token re-tune. It directly prevents an update or
repair instruction being confirmed against the wrong vehicle or on a colour cue. Test-first: write
`REQ-017 a high-impact approval names VIN, action, window and consequence and answers once`, extend
`spec/golden/ui/REQ-007-pending-card.json`, then ship only the dominant consequence line, the explicit
confirm label, the text+icon high-impact marker, keyboard operability and the polite announcement.
Defer (a), (c), (d), (e) until the owner assigns any missing REQ ids and approves scope. Decision
needed from the owner: confirm the sector framing, and whether REQ-038's flag and the computer
backend (REQ-026) are in scope before changes (c)/(d) ship.

Provenance: RPC names/`CONFLICT` guard CONFIRMED from `apps/web/lib/api/rpc.ts`; component structure
(`BoardScreen.COLUMNS` + 404 flag-off, `ComputerPanel` statuses/takeover/release) CONFIRMED by reading
the files; REQ ids and golden names CONFIRMED from `spec/01-requirements.md`, `docs/UI.md` and
`spec/golden/ui/`. The persona, jobs and automotive mapping are GUESS; no product code was modified.
