# UI proposal 043 — transportation (fleet and transit operations)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #043 — transportation.
Grounding (CONFIRMED, read this session): `docs/UI.md`, `spec/08-ui-ux.md`,
`apps/web/components/board/board-screen.tsx` (`BoardScreen`),
`apps/web/components/thread/computer-panel.tsx` (`ComputerPanel`), and RPC exports in
`apps/web/lib/api/rpc.ts`. No upstream brand, code, assets or wording consulted or copied.

Charter (CONFIRMED): Boma Bot only; fresh design (ADR-015); the owner-supplied robot-cat is the sole
identity; no upstream brand, code, assets or copy; BoardUI semantic tokens only — no raw palette class
or hex literal, classes merged with `cx()`; one design system, no second library or hand-built lookalike;
self-hosted only (no SaaS — vehicle, driver and passenger data never leave the operator's machine).

## 1. Persona and top jobs-to-be-done

**Persona — "the duty controller"**: a transport-operations supervisor at a municipal transit operator
or regional fleet, running Boma Bot on the depot's own machine. She assigns vehicles and drivers to
runs, watches live service state, authorises bot-drafted dispatches, and recovers a legacy dispatch
console when a bot must hand it back. Staff work in English, French and Arabic. Vehicle plates,
driver names and passenger counts never leave the building. A dispatch approved on a colour cue, or
onto the wrong vehicle or route, is a real service disruption, so legible state and an explicit
"what will happen" matter more than a fast screen. The robot-cat mark is the only identity art.

Top jobs:
1. **Dispatch and track** — see which runs are proposed, dispatched, in service and complete, and
   which automation runs are still working, without reading a transcript.
2. **Verify before committing** — authorise a bot-proposed dispatch knowing exact vehicle, route,
   driver, departure window and cost; never approve on colour alone.
3. **Triage disruption** — find delays and cancelled runs, tell working from stalled runs, and snooze
   a non-urgent alert with a named due time.
4. **Recover the console** — take over a legacy dispatch/telematics screen for manual entry and leave
   an attributable handoff record.
5. **Prove and share** — keep versioned manifests/duty logs as artifacts, open each in its thread, and
   publish a compiled service-status page by link with no login.

## 2. Five concrete UI changes

All new copy is a `lib/i18n.ts` key via `useTranslation`/`t(locale, key)`; no string in JSX. Every class
list goes through `cx()`; every colour rides a `docs/UI.md` token; type uses composite utilities.

**(a) Route/dispatch board — `/app/board` (REQ-038, feature-flagged).** Reuse `BoardScreen`
(`board-screen.tsx`): the four gated `COLUMNS` read proposed / dispatched / in service / complete, each
card showing run key, title, assigned bot and last comment. Status is text + icon (`Chip`), never
colour alone; the detail dialog keeps its focus trap and Escape handling. BoardUI `Button`, `input`,
`select`, installed `table`/`chip`, `Tooltip`; page `bg-background-full`, card
`bg-background-primary-default`, surface `bg-background-secondary-default`, hairline
`border-separator-border`, text `text-text-primary`/`text-text-secondary`, focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`. Existing RPCs: `boardsGet`, `botsList`,
`ticketsCreate`, `ticketsMove`, `ticketsComment`. A 404 keeps the route absent, not locked (§8.6).

**(b) Dispatch approval card — thread `/app/:botId` (REQ-007, REQ-017).** Make "what will happen" the
dominant line: exact vehicle, route, driver, departure window and cost, with an explicit confirm label
("Dispatch route 14, 06:00", not just "Approve"). A high-impact or safety-sensitive dispatch pairs
`border-border-error-default` + `text-text-error-primary` with a leading icon and the words
"High impact" — never colour alone. BoardUI `Button`, `Badge`, `Tooltip`; card
`bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`. Existing RPCs:
`threadsAnswer` (keeps the exactly-once `CONFLICT` guard), `threadsReact`. Reuse `ThreadComposer`'s
`awaitingApproval` so the transcript stays readable and other threads reachable; announce once via the
polite live region, not the transcript (§8.9).

**(c) Disruption ledger — activity + thread (REQ-011, REQ-012, REQ-021, REQ-009).** Extend
`ActivityScreen` so each run row names its state out loud with a transportation caption (`queued` →
"dispatch queued", `running` → "in service", `waiting_takeover` → "awaiting console handoff", `failed`
→ "disruption") while keeping the canonical §8.9 vocabulary; a delay or cancellation shows one
sentence of cause plus one of what to do, a working Retry, and a snooze with a named due time.
Components: `Button`, `Badge`, `Chip`, `Tooltip`; surface `bg-background-primary-default`, text
`text-text-primary`/`text-text-secondary`, status via `runStatusKey`/`isRetryable`, type
`text-caption-1-semibold`. Existing RPCs: `runsList`, `runsPause`, `runsResume`, `runsRetry`,
`threadsHead`, `threadsSnooze`, `threadsUnsnooze`. Background runs appear here, not in the transcript.

**(d) Console-handoff takeover panel — right panel `/app/:botId` (REQ-026, REQ-028, REQ-029,
REQ-030).** Follow `ComputerPanel` (`computer-panel.tsx`): each state named starting / running /
awaiting takeover / released, with a plain caption above the controls ("Leaving this screen pauses
nothing — the handoff record stays open"). Takeover and release are explicit, attributable actions;
frame refresh honours `prefers-reduced-motion`. Components: `Button`, `Badge`, installed spinner and
skeleton states, icon from `@remixicon/react` passed as a reference; surface
`bg-background-primary-default`, status `text-text-secondary`, error `text-text-error-primary`.
Existing RPCs: `computerStatus`, `computerTakeover`, `computerRelease`, `computerOpenScreen`,
`computerOpenScreenFrame`. Mirror drawer motion in RTL but leave physical screen coordinates untouched;
no vehicle or passenger data in logs.

**(e) Duty archive and public service-status page — `/app/artifacts`, `/p/:slugToken` (REQ-036,
REQ-037).** Reuse `ArtifactsGrid`/`ArtifactsViewer`: manifests, duty logs and incident reports with a
type/bot filter, name search and a version chain, "open in thread" and download. Content is untrusted —
render as data, never HTML in the document. Reuse `SharedPageScreen`: no chrome beyond a small footer
with the robot-cat mark; unknown, revoked and expired tokens render one identical "not available"
document (same words, shape and timing). Tokens `bg-background-full`,
`bg-background-secondary-default`, `text-text-primary`. Existing artifact RPCs:
`artifactsListSpace`, `artifactsGet`, `artifactsListVersions`; the shared page uses the existing GET
transport — no new transport. `noindex`/`no-referrer` and its own CSP stay backend concerns.

## 3. Accessibility

WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; a keyboard path for every
primary flow (approve, retry, snooze, takeover/release, board move, version select); visible
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target`; 360 px width;
`prefers-reduced-motion` honoured; no animation on streaming content. Run status, high-impact and
disruption markers are text + icon using the exact §8.9 vocabulary (queued/leased/running,
waiting_input/waiting_takeover, completed/failed/cancelled). Tables carry `<caption>` and `<th scope>`.
Focus lands on a pending card; the uniform unavailable page is announced in identical text — nothing
leaks through the accessibility tree (§8.8).

## 4. i18n and RTL

English is the source catalogue, fr/he the translations; a missing key falls back to English — never a
blank or the raw key. `DirectionProvider`
(`@/components/foundations/direction/direction`) takes an explicit BCP 47 locale; the boot script
writes `lang` and `dir` together. RTL mirrors board columns, the rail and drawer motion via
`useDirection()`; arrows flip. Logical utilities throughout (`ms`/`me`, `ps`/`pe`, `text-start`/
`text-end`, `border-s`/`border-e`). Plate numbers, route and run keys, departure times, money and URLs
stay LTR in `<bdi>`/`dir="auto"`; money in integer minor units via `Intl`. Error copy is one sentence
of what happened plus one of what to do — no plate, driver, passenger count, id, stack trace or
provider text on screen (correlation id behind "details" only, §8.9).

## 5. Playwright tests (REQ ids in the names)

- `test("REQ-038 the dispatch board names text-and-icon columns and moves a run by keyboard", ...)`.
- `test("REQ-038 the board route is absent, not locked, when the flag returns 404", ...)`.
- `test("REQ-007 the dispatch card names vehicle, route, driver, window, cost and takes focus", ...)`.
- `test("REQ-017 a high-impact dispatch answers exactly once and keeps the CONFLICT guard", ...)`.
- `test("REQ-011 the disruption ledger names each run state with a transportation caption", ...)`.
- `test("REQ-021 a delayed or cancelled run shows a plain-language cause and a working retry", ...)`.
- `test("REQ-009 a disruption alert is snoozed with a named due time and returns on time", ...)`.
- `test("REQ-026 the handoff panel names starting, running, takeover and released states", ...)`.
- `test("REQ-028 takeover and release announce their status through the polite live region", ...)`.
- `test("REQ-036 a duty manifest selects a version and opens in its thread", ...)`.
- `test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)`.
- `test("REQ-057 Hebrew RTL mirrors the board and axe reports zero critical or serious findings", ...)`.

Goldens in `spec/golden/ui/` stay byte-exact; a new state means a new `REQ-<id>-<slug>.json` asserting
seeded state, never a click path. New behaviour without a REQ (transportation captions, high-impact
marker) must get an owner-assigned id before code — do not invent one in a test name.

## 6. Best first slice

**Change (b), the dispatch approval card** — GUESS, recommended. Smallest and highest-value: one
screen, no new transport (`threadsAnswer`/`threadsReact` exist), no new component (BoardUI
`Button`/`Badge`/`Tooltip` are installed) and no token re-tune. It directly prevents a dispatch being
confirmed on a colour cue or for the wrong vehicle. Test-first: write
`REQ-007 ... names vehicle, route, driver, window, cost and takes focus`, extend
`spec/golden/ui/REQ-007-pending-card.json`, then ship only the dominant consequence line, the explicit
"Dispatch route 14, 06:00" label, the text+icon high-impact marker, keyboard operability and the
polite announcement. Defer (a), (c), (d), (e) until the owner assigns any missing REQ ids and approves
scope.

Provenance: RPC names CONFIRMED from `apps/web/lib/api/rpc.ts`; `BoardScreen` and `ComputerPanel`
structure CONFIRMED by reading the files this session; REQ ids and golden names CONFIRMED from
`docs/UI.md` and `spec/golden/ui/`. The persona, its jobs and the transportation mapping are GUESS
(proposal only — no transportation sector or REQ exists in the spec). No product code was modified.
