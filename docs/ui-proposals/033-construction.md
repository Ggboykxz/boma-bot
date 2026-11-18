# UI proposal 033 — construction (self-hosted site and project desk)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #033 — construction. Read-only on
product code; only this file is written. Grounding read this session: `docs/UI.md`,
`spec/08-ui-ux.md`, `apps/web/lib/api/rpc.ts` (RPC names CONFIRMED), and three components —
`apps/web/components/board/board-screen.tsx` (`BoardScreen`),
`apps/web/components/thread/computer-panel.tsx` (`ComputerPanel`) and
`apps/web/components/artifacts/artifacts-grid.tsx` (`ArtifactsGrid`). REQ ids checked against
`spec/01-requirements.md`, `docs/UI.md` and the `spec/golden/ui/` table.

Charter held: Boma Bot only; no upstream brand, code, assets or wording; BoardUI semantic tokens
only — no raw palette class or hex literal, classes merged with `cx()`; one design system, no second;
no SaaS (self-hosted on the contractor's machine — drawings, costs, submittals and safety records
never leave the premises); robot-cat identity and product name unchanged.

## 1. Persona and top jobs-to-be-done

**Persona — Amal, site engineer on a mid-size building contractor.** She coordinates trades, raises
and answers RFIs and submittals, tracks work packages, watches safety and quality exceptions, and
hands over site plant (a drone, a scanner terminal) when a bot needs a human at the controls. Crew,
subcontractors and the client work in English, French and Arabic. Boma Bot runs on the firm's own
machine: drawing sets, rates and safety records never leave the site office. A vague approval or a
missed stop-work notice costs a rework visit or an incident, so legible state and an explicit "what
will happen" matter more than a fast screen.

Top jobs:
1. **Run the site pulse** — see which work packages are queued, active, at risk or blocked, and which
   automation runs are still working, without scrolling a transcript.
2. **Verify before committing** — confirm a bot-proposed submittal or RFI answer knowing the exact
   scope, trade, cost and deadline; never approve on colour alone.
3. **Triage exceptions** — find stop-work notices, defects and unread items, tell working runs from
   stalled ones, and snooze non-urgent notices with a named due time.
4. **Supervise plant handoff** — take over site equipment when it needs manual control, and leave an
   attributable handoff record.
5. **Prove and archive** — keep versioned drawing, spec and method-statement artifacts, open each in
   the thread where it was produced, and never render untrusted content in the app document.

## 2. Five concrete changes

All new copy is catalogued in `lib/i18n.ts` through `t(locale, key)` (English source, French/Hebrew
translations, a missing key falls back to English — never blank or the raw key). Every colour rides
the `docs/UI.md` token map, type uses composite utilities, and every class list goes through `cx()`.

**(a) Site-pulse strip — screen `/app/:botId` (thread header).** A compact, read-only band above the
transcript with chips for "on schedule", "at risk" and "blocked", each with a `Tooltip` carrying the
detail (work-package count, next milestone). Components: `Badge`/`Chip`
(`components/base/badges/chip.tsx`), `Tooltip`, `IconButton` (icons from `@remixicon/react` passed as
references). Tokens: surface `bg-background-secondary-default`, hairline `border-separator-border`,
text `text-text-primary`/`text-text-secondary`, icon `text-foreground-icon-secondary`, type
`text-caption-1-semibold`. Existing RPCs: `botsGet`, `threadsHead`, `runsList`. Bot-produced values
are untrusted text, never HTML; dates, quantities and drawing numbers stay LTR in `<bdi>`.

**(b) Submittal / RFI approval card — screen `/app/:botId` (pending card, REQ-007/REQ-017).** Make
"what will happen" the dominant line: exact scope, trade, cost and deadline, with an explicit confirm
label ("Submit Rev C structural drawings to the client, 16:00 Friday", not just "Approve"). A
stop-work or high-cost action pairs `border-border-error-default` + `text-text-error-primary` with a
leading icon and the words "Stop work" / "High cost" — never colour alone. Components: `Button`,
`Badge`, `Tooltip`; card surface `bg-background-secondary-default`, `rounded-3xl`,
`border-separator-border`, focus `focus-visible:ring-border-focus-ring`. Existing RPCs:
`threadsAnswer` (exactly-once effect, keeps the `CONFLICT` guard for double answers), `threadsReact`.
Reuse `ThreadComposer`'s `awaitingApproval` prop so the transcript stays readable and other threads
reachable; the card is announced once through the polite live region, not the transcript (§8.9).

**(c) Named site-exception ledger — screen `/app` activity (REQ-011/REQ-012).** Extend
`ActivityScreen` so every construction run row names its state out loud and adds a plain-language
caption (`queued` → "work package queued", `running` → "crew on site", `waiting_takeover` → "awaiting
plant handoff", `failed` → "stop-work / defect") — the canonical §8.9 vocabulary stays, the caption
is secondary text, never a colour-only chip. Working/paused rows keep Pause/Resume and failed rows
keep Retry. Components: `Button`, `Badge`, `Chip`, `Tooltip`; surface `bg-background-primary-default`,
text `text-text-primary`/`text-text-secondary`, status via `runStatusKey`/`isRetryable` from
`lib/run-status.ts`, type `text-caption-1-semibold`. Existing RPCs already used there: `runsList`,
`runsPause`, `runsResume`, `runsRetry`, plus `threadsHead`. Background (ticket) runs appear here,
not in the transcript (§8.4).

**(d) Plant-handoff takeover panel — screen `/app/:botId` right panel (REQ-026/REQ-028/REQ-029).**
Give each computer state a named status (starting, running, awaiting takeover, released) and a plain
caption above the controls: "Leaving this screen pauses nothing — the handoff record stays open."
Takeover and release are explicit, attributable actions. Components: `Button`, `Badge` and the
installed spinner/skeleton states; surface `bg-background-secondary-default`, text
`text-text-secondary`, status as text + icon. Existing RPCs: `computerStatus`, `computerTakeover`,
`computerRelease`, `computerOpenScreen`. Mirror drawer motion in RTL; leave physical screen
coordinates untouched; no site data in logs.

**(e) Work-package board and drawing/spec archive — screens `/app/board` (REQ-038) and
`/app/artifacts` (REQ-036).** In `BoardScreen`, retitle the four flag-gated `COLUMNS` as ready →
 mobilise → on site → signed off, each card showing work-package key, title, assigned trade bot and
last comment; the named detail panel shows attached runs. Components already used there: `Button`,
`input`, `select`, installed `table`/`chip`; surface `bg-background-primary-default`, hairline
`border-separator-border`, status text + icon. RPCs: `boardsGet`, `ticketsCreate`, `ticketsMove`,
`ticketsComment`. In `ArtifactsGrid`, show drawings/specs/method statements with a version selector,
download and "open in thread" via `artifactsListSpace`, `artifactsGet`, `artifactsListVersions`.
Content is untrusted — never inject artifact HTML into the document. With the flag off the board
route is absent, not locked (REQ-038).

## 3. Accessibility, i18n and RTL

- All visible strings go through `t(locale, key)` (English source, French/Hebrew translations, a
  missing key falls back to English, never blank or key). No string hardcoded in JSX.
- `DirectionProvider` takes the explicit BCP 47 locale; the boot script writes `lang`/`dir` together.
  Use logical utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`) and
  `useDirection()` to mirror takeover arrows and drawer motion in RTL. Drawing numbers, dates,
  quantities, money and URLs stay LTR in `<bdi>`/`dir="auto"`, money in integer minor units via
  `Intl` (ADR-023).
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px `.boma-target`
  on touch controls; token-driven visible focus; a keyboard path for every decision (submit, snooze,
  takeover, board move, version select); run status and stop-work markers are text + icon, never
  colour alone (§8.9); 360 px width; `prefers-reduced-motion` honoured; no animation on streaming
  content.
- Error copy is one sentence of what happened plus one of what to do — no drawing ids, rates, costs,
  stack traces or provider text on screen (correlation id behind "details" only, §8.9). The
  stop-work marker is text + icon and the submit decision is announced once via the polite live
  region; the transcript is not a live region, so streaming tokens are not read aloud.

## 4. Playwright tests (REQ ids in the names)

```
test("REQ-001 the site composer sends one optimistic message that is not duplicated", ...)
test("REQ-007 a pending submittal card takes focus and does not block reading or other threads", ...)
test("REQ-009 the exception divider moves and the snooze shelf returns a notice on time", ...)
test("REQ-011 the site ledger names each construction run state with a plain caption", ...)
test("REQ-012 every run status renders a distinct named chip with the right terminal action", ...)
test("REQ-017 a stop-work approval names scope, trade, cost and answers once", ...)
test("REQ-026 the plant-handoff panel names starting, running, takeover and released states", ...)
test("REQ-028 takeover and release announce their status through the polite live region", ...)
test("REQ-036 the drawing viewer selects a version and opens it in its thread", ...)
test("REQ-038 the work-package board moves a ticket and records a comment through the same RPCs", ...)
test("REQ-057 the construction screens mirror for Hebrew with dir=rtl and a clean axe run", ...)
```

New behaviour without a REQ yet (site-pulse strip, stop-work/high-cost markers, handoff caption,
construction captions on canonical chips) must be assigned a REQ id by the owner before code — do not
invent one in a test name per the traceability gate.

## 5. Highest-impact first slice

**Change (b), the submittal / RFI approval card.** It is the cheapest and the only change that
directly reduces safety and financial risk: `threadsAnswer`, `Button` and `Badge` are installed, the
semantic tokens exist, and no new RPC, component install, data model or token re-tune is needed.
Test-first: write `REQ-017 a stop-work approval names scope, trade, cost and answers once` and extend
`spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant consequence sentence, the
explicit confirm label, the text+icon stop-work/high-cost marker, keyboard operability and the polite
announcement. Defer (a), (c), (d), (e) until the owner assigns REQ ids and approves.

Provenance: RPC names and the `CONFLICT` guard CONFIRMED from `apps/web/lib/api/rpc.ts`; component
structure (`BoardScreen.COLUMNS`, `ComputerPanel` statuses/takeover, `ArtifactsGrid` filters)
CONFIRMED by reading the files this session; REQ ids and golden names CONFIRMED from
`spec/01-requirements.md`, `docs/UI.md` and `spec/golden/ui/`. The persona, its jobs and the
construction mapping are GUESS (proposal only — no construction sector or REQ exists in the spec).
No product code was modified.
