# UI proposal 059 — data analytics (self-hosted reporting and dashboards)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #059 — data-analytics. Read-only
on product code; this file is the only write. Grounding (CONFIRMED, read this session): `docs/UI.md`,
`spec/08-ui-ux.md`, `spec/01-requirements.md` (REQ ids), `apps/web/lib/api/rpc.ts` (RPC names), and two
components: `activity/activity-screen.tsx` (named status in words, pause/resume/retry) and
`board/board-screen.tsx` (columns, flag-off 404, focus-trapped dialog, raw form fields). No upstream
brand, code, assets, wording or capture consulted or copied. `lib/run-status.ts` fixes the vocabulary.

Charter (CONFIRMED): Boma Bot only; fresh design (ADR-015); the owner-supplied robot-cat mark is the
sole identity; no upstream brand/code/assets/copy; BoardUI semantic tokens only — no raw palette
class or hex literal, type via composite utilities, every class list merged with `cx()`; one design
system, no second library or hand-built lookalike; self-hosted on the owner's own machine — no SaaS,
no dataset, query, report or dashboard leaving it. Product name and mark unchanged.

## 1. Persona and top jobs-to-be-done

**Persona — "the analytics lead"** (GUESS): a data/BI lead at a self-hosted organisation who asks
bots to pull figures, build reports and assemble dashboards. She works in English, French and Arabic,
between a laptop and a phone; the warehouse extracts and customer figures never leave the building.
A report published against the wrong dataset or version, or on a colour cue alone, is a real decision
loss — ambiguity is worse than latency. Top jobs:
1. **Request** — ask a bot for a report or dashboard in the composer without losing the request to a
   stray Enter (REQ-001, REQ-039).
2. **Publish** — review a bot-proposed report publication or dataset refresh knowing the exact
   dataset, version, destination and effect; never approve on colour alone (REQ-007, REQ-017).
3. **Track** — name ingest, refresh, transform and report runs out loud with the §8.9 vocabulary,
   retry only failures, pause/resume queued work (REQ-011, REQ-012, REQ-067).
4. **File and share** — keep versioned reports and bot-built dashboards, open each version in the
   thread that produced it, and share a reviewed dashboard by link with no login or leakage
   (REQ-036, REQ-037, REQ-039).
5. **Triage** — move an analytics request from intake to published, with attached runs and notes
   (REQ-038).

## 2. Five concrete UI changes

All new copy is a new `lib/i18n.ts` key via `useTranslation` / `t(locale, key)` (English source, fr/he
translations; a missing key falls back to English — never blank or the raw key). Every colour rides
the `docs/UI.md` token map, type uses composite utilities, and every class list goes through `cx()`.
Bot-produced data and uploaded extracts are untrusted text/bytes, never HTML (spec §8.5).

**(a) Report-request composer — thread `/app/:botId` (REQ-001, REQ-039).** Extend `ThreadComposer` with
a named dataset-chip and a labelled "build dashboard" affordance; keep Enter-to-send /
Shift+Enter-newline and send-disabled-on-empty as today. Components: BoardUI `Chip`, `IconButton`
(`@remixicon/react` references, not elements), `Tooltip`; surfaces `bg-background-primary-default`,
border `border-border-button-default`, icons `text-foreground-icon-primary`, focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`, 44 px `.boma-target`. Existing RPCs
(CONFIRMED): `threadsSend`, `artifactsGet` (attached data). A generated dashboard renders only in a
sandboxed document (REQ-039) — never inject its markup into the app document.

**(b) Publication/refresh approval card — thread `/app/:botId` pending card (REQ-007, REQ-017).** Make
the "what will happen" sentence dominant: exact dataset and version, destination and effect, with a
distinct confirm label ("Publish DASH_07 v4 to the leadership link", not just "Approve"). A high-impact
action (publish, overwrite a report, refresh production data) pairs `border-border-error-default` +
`text-text-error-primary` with a leading icon and the words "High impact" — never colour alone.
Components: BoardUI `Button`, `Chip`, `Tooltip`; card `bg-background-primary-default`, `rounded-3xl`,
`border-border-button-default`. Existing RPC (CONFIRMED): `threadsAnswer` (keep its `CONFLICT` guard
against a double answer). Transcript stays readable, other threads reachable, focus lands on the card,
and the decision is announced once via the polite live region (§8.9); the transcript is not a live region.

**(c) Analytics run monitor — `/app` Activity view and thread status row (REQ-011, REQ-012, REQ-067).**
Follow `ActivityScreen`: group ingest/refresh/transform/report runs by day with prompt snippet, bot
name, duration and a link to the starting message; working runs sort to the top and update in place.
Add a small volume/status trend using only the `chart-1`…`chart-5` tokens (`-active` variants) with a
text equivalent. Components: installed `table`/`chip`, `Button`, `IconButton`, `Tooltip`; surfaces
`bg-background-primary-default`/`bg-background-secondary-default`, hairline `border-separator-border`,
text `text-text-primary`/`text-text-secondary`; status as text + icon using the exact `run-status.ts`
vocabulary. Existing RPCs (CONFIRMED): `runsList`, `runsRetry`, `runsPause`, `runsResume`,
`notificationsStatus`. Measured chart geometry stays LTR.

**(d) Report library and shared dashboard — `/app/artifacts` and `/p/:slugToken` (REQ-036, REQ-037,
REQ-039).** Extend the grid and viewer: name reports, extracts and dashboards by type, filter by
type/bot, search by name, show the version chain and "open in thread". Keep the safe-byte walls —
images in a bounded frame, PDFs/text natively, other bytes download-only, bytes never injected as
markup — and replace any raw `<select>` with the installed BoardUI `Select`. Components: `Input`,
`Select`, `Chip`, `Button`; tokens `text-text-primary`/`text-text-secondary`/`text-text-tertiary`,
`bg-background-primary-default`, `border-border-button-default`. Existing RPCs (CONFIRMED):
`artifactsListSpace`, `artifactsGet`, `artifactsListVersions`, `artifactsRemove`. Reuse the shared-page
path for a reviewed dashboard: transport `GET /api/v1/pages/shared/{token}` (CONFIRMED); unknown,
revoked and expired tokens render one identical "not available" document — same words, shape and
timing. `noindex`/`no-referrer` and its own CSP stay backend concerns. Keep the robot-cat empty state
when nothing is filed.

**(e) Analytics request board — `/app/board` (REQ-038, feature-flagged).** Reuse `BoardScreen`:
retitle the four `COLUMNS` to intake → in analysis → drafted → published, each card showing key,
title, assignee bot and note count; the detail dialog shows attached runs with named status chips.
Replace its raw `<input>`, `<textarea>` and `<select>` with the installed BoardUI `Input` and `Select`
merged with `cx()`; keep the existing focus-trapped modal and keyboard path. Tokens:
`bg-background-primary-default`, `border-separator-border`, `text-text-primary`/`text-text-secondary`.
Existing RPCs (CONFIRMED): `boardsGet`, `botsList`, `ticketsCreate`, `ticketsMove`, `ticketsComment`,
`runsList`. A flag-off 404 keeps the route absent, not locked (§8.6); status is text + icon.

## 3. Accessibility

WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; a keyboard-only path
for every primary flow (dataset select, publication decision, run pause/resume, board moves, version
select); visible `focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target`;
360 px width; `prefers-reduced-motion` honoured; no animation on streaming content. Status and
high-impact markers are text + icon, never colour alone. Charts get a text equivalent / data table so
figures are not colour- or shape-only. The uniform unavailable screen is announced in identical text —
no leak through the accessibility tree; the board dialog traps and returns focus as today. Error copy
is one sentence of what happened plus one of what to do — no dataset name, id, query, stack trace or
provider text on screen (correlation id behind "details" only, §8.9). Tables carry `<caption>`/`<th scope>`.

## 4. i18n and RTL

English is the source catalogue, fr/he the translations; a missing key falls back to English — never
blank or the raw key. `DirectionProvider` (`@/components/foundations/direction/direction`) gets an
explicit BCP 47 locale; the boot script writes `lang`/`dir` together. RTL mirrors the board columns,
the rail and drawer motion via `useDirection()`; navigation and disclosure arrows flip. Logical
utilities throughout (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Percentages,
figures, dates, query strings, URLs, code and dataset keys stay LTR in `<bdi>`/`dir="auto"`. Preserve
physical coordinates for measured chart geometry — plots and axes are never mirrored. Formatting data
is separate from layout direction.

## 5. Playwright tests (REQ id in each name)

- `test("REQ-001 the report-request composer sends one optimistic message that is not duplicated", ...)`
- `test("REQ-039 a bot-built dashboard renders in the sandboxed viewer and never injects app HTML", ...)`
- `test("REQ-007 the publication card names dataset, version, destination and effect and takes focus", ...)`
- `test("REQ-017 approve and deny answer the publication card exactly once with an attributable confirmation", ...)`
- `test("REQ-011 the activity view groups analytics runs by day and filters by bot and status", ...)`
- `test("REQ-012 every analytics-run status renders a distinct named text-and-icon chip", ...)`
- `test("REQ-036 the report library filters by type and bot and shows the version chain", ...)`
- `test("REQ-037 unknown, revoked and expired links render the identical unavailable document", ...)`
- `test("REQ-038 the request board shows text-and-icon columns and moves a ticket by keyboard", ...)`
- `test("REQ-057 Hebrew RTL mirrors the board and rail and axe reports zero critical or serious findings", ...)`

Behaviour without a REQ yet (dataset chip, high-impact marker, chart equivalent) must get an
owner-assigned id before code — do not invent one in a test name. Goldens under `spec/golden/ui/`
stay byte-exact; a new state means a new `REQ-<id>-<slug>.json` asserting seeded state, not a click path.

## 6. Best first slice

**Change (c), the analytics run monitor** — GUESS, recommended. It is read-only and the smallest
step: no new transport (`runsList`, `runsRetry`, `runsPause`, `runsResume` exist), no new component
(the `ActivityScreen` pattern and installed `table`/`chip`/`Button` suffice), no token re-tune, and
no new REQ id. It directly delivers the spec's "state is never ambiguous" constraint for analytics
pipelines, where a stalled refresh or a failed transform is otherwise invisible in the chat.
Test-first: write `REQ-011 the activity view groups analytics runs by day and filters by bot and
status` and `REQ-012 every analytics-run status renders a distinct named text-and-icon chip`, then ship
only the grouped list, the named text-and-icon chips and retry/pause/resume controls. Defer the trend
visualisation and (a), (b), (d), (e) until the owner assigns any missing REQ ids and approves scope.

Provenance: RPC names and the `CONFLICT` guard CONFIRMED from `apps/web/lib/api/rpc.ts`; component
structure (`activity-screen.tsx` named status + retry/pause/resume, `board-screen.tsx` columns, raw
form fields, focus-trapped dialog, flag-off 404) CONFIRMED by reading both files this session; REQ
ids, tokens and golden names CONFIRMED from `spec/01-requirements.md`, `docs/UI.md`, `spec/08-ui-ux.md`.
The persona, jobs and data-analytics mapping are GUESS (no such sector/REQ in the spec). No product code was modified.
