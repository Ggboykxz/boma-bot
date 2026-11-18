# UI proposal 040 — mining (self-hosted mine operations desk)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #040 — mining. Read-only on
product code; this file is the only write. Grounding (CONFIRMED, read this session): `docs/UI.md`,
`spec/08-ui-ux.md`, RPC names in `apps/web/lib/api/rpc.ts`, and two components — `BoardScreen`
(`board-screen.tsx`) and `ActivityScreen` (`activity-screen.tsx`). REQ ids and golden names checked
against `spec/01-requirements.md`, `docs/UI.md` and `spec/golden/ui/`. No upstream brand, code,
assets or wording consulted or copied.

Charter held (CONFIRMED): Boma Bot only, fresh design (ADR-015), the owner-supplied robot-cat as the
sole identity; no upstream brand/code/assets/copy; BoardUI semantic tokens only — no raw palette
class or hex literal, every class list merged with `cx()`; one design system, no second library or
hand-built lookalike; self-hosted on the mine's own machine — production, geotech, dispatch and
safety records never leave the site, no SaaS/cloud.

## 1. Persona and top jobs-to-be-done

**Persona — "the shift supervisor"**: a control-room supervisor at a surface or underground mine.
She watches production and equipment state, raises maintenance and safety work orders, authorises
high-impact actions (a blast, a confined-space entry, a haulage dispatch, a reagent-dose change),
and takes manual control of plant when a bot needs a human at the levers. Crew, contractors and
regulators work in English, French and Arabic, on a wall display and a dusty handheld. Boma Bot runs
on the site's own network: grade, assay, dispatch and incident data never leave the premises. A
confirmation on a colour cue or against the wrong pit/level is an incident, so legible, named state
matters more than a fast screen. The robot-cat mark is the only identity art.

Top jobs:
1. **Read the shift pulse** — see tonnage, equipment availability and open safety alarms on the thread without hunting the rail.
2. **Authorise safely** — confirm a bot-proposed blast, entry or dispatch knowing the exact pit/level, scope, window and consequence; never approve on colour alone.
3. **Run work orders** — move maintenance, inspection and safety tickets from raised through scheduled and in progress to signed off, keeping the record.
4. **Name run state** — state every automation run out loud using the §8.9 vocabulary; pause/resume queued work, retry only failures.
5. **Prove and share** — keep versioned assay, geotech and incident-report artifacts, open each where it was produced, and publish a compiled production summary by link with no login or leakage.

## 2. Five concrete UI changes

All copy is a new `lib/i18n.ts` key via `t(locale, key)`; no string in JSX. Every colour rides the
`docs/UI.md` token map, type uses composite utilities, every class list goes through `cx()`, and
dark mode flips tokens automatically.

**(a) Shift-pulse strip — `/app/:botId` (thread header, REQ-001, §8.3).** A compact, read-only band
above the transcript with named chips for tonnes moved, equipment availability and open alarms, plus
a small trend using the `chart-1`…`chart-5` tokens and `-active` variants. Components: `Badge`/`Chip`
(`components/base/badges/chip.tsx`), `Tooltip`, `IconButton` (icons from `@remixicon/react` passed as
references). Tokens: surface `bg-background-secondary-default`, hairline `border-separator-border`,
text `text-text-primary`/`text-text-secondary`, icon `text-foreground-icon-secondary`, type
`text-caption-1-semibold`. Existing RPCs: `botsGet`, `threadsHead`, `threadsMessages`. Bot-produced
values are untrusted text, never HTML; keep pit/level and equipment ids out of logs. Chart stays LTR.

**(b) High-impact approval card — `/app/:botId` (pending card, REQ-007/REQ-017, §8.9).** Make "what
will happen" dominant: exact pit/level, scope, window and consequence, with an explicit confirm
label ("Initiate blast, Pit 3 bench 12, 22:00", not just "Approve"). A blast, confined-space entry or
reagent-dose change pairs `border-border-error-default` + `text-text-error-primary` with a leading
icon and the words "High impact" — never colour alone. Components: `Button`, `Badge`, `Tooltip`;
card `bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`, focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`. Existing RPCs: `threadsAnswer`
(exactly-once effect, keeps the `CONFLICT` guard for double answers), `threadsReact`. Reuse the
composer's pending-card behaviour so the transcript stays readable and other threads reachable;
announce once through the polite live region — the transcript is not a live region (§8.9).

**(c) Mine work-order board — `/app/board` (REQ-038, feature-flagged, §8.6).** Reuse `BoardScreen`:
retitle the four flag-gated `COLUMNS` as raised → scheduled → in progress → signed off, each card
showing work-order key, title, assigned trade bot and last comment; the named detail panel shows the
attached runs. Status is text + icon, never colour alone. Prefer installed BoardUI `Input`/`Select`
over the raw fields in `board-screen.tsx`, merged with `cx()`; tokens `bg-background-primary-default`,
`border-border-button-default`, `text-text-primary`/`text-text-secondary`, focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`. RPCs: `boardsGet`, `botsList`,
`ticketsCreate`, `ticketsMove`, `ticketsComment`. Flag off: route absent, not locked.

**(d) Named run triage — activity view (REQ-011/REQ-012/REQ-067, §8.4).** Extend `ActivityScreen` so
every run row names its state out loud and adds a plain mining caption (`queued` → "job queued",
`running` → "crew on shift", `waiting_takeover` → "awaiting plant handover", `failed` → "stopped /
defect") — the canonical §8.9 vocabulary stays, the caption is secondary text. Working/paused rows
keep Pause/Resume, failed rows keep Retry. Components: `Button`, `Badge`, `Chip`, `Tooltip`;
`border-border-button-default`, `bg-background-primary-default`, `text-text-primary`/
`text-text-secondary`, and `border-border-error-default` + `text-text-error-primary` +
`text-foreground-icon-error` on the error row; status via `runStatusKey`/`isRetryable` from
`lib/run-status.ts`. Existing RPCs: `runsList`, `runsPause`, `runsResume`, `runsRetry`, plus
`threadsHead`. Working runs sort to the top and update in place.

**(e) Assay archive and shared production report — `/app/artifacts` and `/p/:slugToken`
(REQ-036/REQ-037, §8.5/§8.8).** Following the `ArtifactsGrid` pattern: name assay sheets, geotech
logs and incident reports by type, filter by type/bot, search by name, show the version chain and
"open in thread". Content is untrusted — render as data, never HTML in the document. Components:
`Input`, `Select`, `Chip`, `Button`; tokens `text-text-primary`/`text-text-secondary`/
`text-text-tertiary`, `bg-background-primary-default`, `border-border-button-default`. Existing
RPCs: `artifactsListSpace`, `artifactsGet`, `artifactsListVersions`. Publish a compiled production
summary to a regulator with no session and no chrome beyond a small footer carrying the robot-cat
mark; unknown, revoked and expired tokens render one identical "not available" screen — same words,
shape and timing. `noindex`/`no-referrer` and the CSP stay backend concerns; transport (CONFIRMED):
`GET /api/v1/pages/shared/{token}`.

## 3. Accessibility

WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; a keyboard-only path
for every primary flow (approve, board moves, pause/resume, retry, filters); visible
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target` on gloved
handheld controls; 360 px width; `prefers-reduced-motion` honoured; no animation on streaming
content. Alarm severity, high-impact markers and run status are text + icon, using the exact §8.9
vocabulary — no colour-only pit alarms. Tables and boards carry accessible names; the board dialog
keeps its focus trap. Focus lands on a pending card; the uniform unavailable screen is announced in
identical text — no leak through the accessibility tree.
Error copy is one sentence of what happened plus one of what to do — no pit ids, equipment ids,
assays, stack traces or provider text on screen (correlation id behind "details" only, §8.9).

## 4. i18n and RTL

English is the source catalogue, fr/he the translations; a missing key falls back to English — never
a blank or the raw key. `DirectionProvider`
(`@/components/foundations/direction/direction`) gets an explicit BCP 47 locale; the boot script
writes `lang` and `dir` together. RTL mirrors the board's columns, the rail and drawer motion via
`useDirection()`; arrows flip. Logical utilities throughout (`ms`/`me`, `ps`/`pe`,
`text-start`/`text-end`, `border-s`/`border-e`). Pit/level and equipment codes, assay values,
timestamps, tonnage, URLs and OTP digits stay LTR in `<bdi>`/`dir="auto"`; chart geometry is not mirrored.

## 5. Playwright tests (REQ id in each name)

```
test("REQ-001 the shift composer sends one optimistic message that is not duplicated", ...)
test("REQ-007 a pending blast card takes focus and does not block reading or other threads", ...)
test("REQ-017 a high-impact approval names pit, scope, window, consequence and answers once", ...)
test("REQ-038 the work-order board moves a ticket and records a comment through the same RPCs", ...)
test("REQ-038 the board route is absent, not locked, when the flag returns 404", ...)
test("REQ-011 the activity view replaces the list and groups mine runs by day", ...)
test("REQ-012 every run status renders a distinct named chip with the right terminal action", ...)
test("REQ-067 pause and resume announce their status through the polite live region", ...)
test("REQ-036 the assay archive filters by type and bot and shows the version chain", ...)
test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)
test("REQ-009 the unread divider moves after a shift thread is read", ...)
test("REQ-057 Hebrew RTL mirrors the board and axe reports zero critical or serious findings", ...)
```

Goldens under `spec/golden/ui/` stay byte-exact; a new state means a new `REQ-<id>-<slug>.json`
asserting seeded state, never a click path. New behaviour without a REQ (pulse strip, high-impact
marker, mining captions) needs an owner-assigned id before code — do not invent one in a test name.

## 6. Best first slice

**Change (b), the high-impact approval card** — GUESS, recommended. It is the smallest, highest-value
step: one screen, no new transport (`threadsAnswer`/`threadsReact` exist), no new component
(BoardUI `Button`/`Badge`/`Tooltip` are installed) and no token re-tune. It directly prevents a blast,
confined-space entry or reagent-dose change being confirmed on a colour cue or against the wrong pit.
Test-first: write `REQ-017 a high-impact approval names pit, scope, window, consequence and answers
once`, extend `spec/golden/ui/REQ-007-pending-card.json`, then ship only the dominant consequence
line, the "Initiate blast…" label and the text+icon marker. Defer the rest until REQ ids and scope are approved.

Provenance: RPC names and the `CONFLICT` guard CONFIRMED from `apps/web/lib/api/rpc.ts`; component
structure (`BoardScreen.COLUMNS`, `ActivityScreen` run rows, `runStatusKey`/`isRetryable`) and REQ
ids/goldens CONFIRMED from the files read this session and `spec/01-requirements.md`. The persona,
jobs and mining mapping are GUESS (no mining sector or REQ exists). No product code was modified.
