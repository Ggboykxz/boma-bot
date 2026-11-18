# UI proposal 038 — oil and gas (production, HSE and field-operations desk)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #038 — oil and gas. Read-only on
product code; only this file is written. Grounding read this session (CONFIRMED): `docs/UI.md`,
`spec/08-ui-ux.md`, `apps/web/lib/api/rpc.ts` (RPC names, `CONFLICT` guard), and two components —
`apps/web/components/activity/activity-screen.tsx` (`ActivityScreen`, `runStatusKey`, `isRetryable`)
and `apps/web/components/thread/computer-panel.tsx` (`ComputerPanel`, takeover/release). REQ ids and
golden names checked against `spec/01-requirements.md`, `docs/UI.md` and `spec/golden/ui/`.

Charter (CONFIRMED): Boma Bot only, fresh design (ADR-015), the owner-supplied robot-cat as the sole
identity; no upstream brand, code, assets or wording; BoardUI semantic tokens only — no raw palette
class or hex literal, classes merged with `cx()`; one design system, no second library or lookalike;
self-hosted only (no SaaS — well logs, rates, SCADA reads, P&IDs and HSE records never leave the
operator's own machine). Product name and robot-cat unchanged.

## 1. Persona and top jobs-to-be-done

**Persona — "the production and HSE engineer"**: a production engineer at an independent operator who
runs Boma Bot on the company's own hardware. She watches wells and assets, chases pressure/temperature
alarms, reviews bot-proposed permits, chemical-injection changes and work orders, and keeps well logs,
P&IDs, inspection certificates and management-of-change (MOC) records audit-ready. Crew and contractors
work in English, French and Arabic. An approval on a colour cue or a wrong dosage/valve is a trip, a
spill or an incident, so legible state and an explicit "what will happen" beat a fast screen. No
operator, regulator or certification logo is borrowed — the robot-cat is the only art.

Top jobs:
1. **Run the production pulse** — name every asset/work order out loud (working / paused / terminal
   vocabulary, §8.9), never by colour alone, and tell working runs from stalled ones.
2. **Authorise safely** — approve a bot-drafted permit, injection change or work order knowing the
   exact well, tag, volume and effect, before the side effect takes place (REQ-017).
3. **Triage alarms** — find a pressure/HSE excursion or unread item, read a plain-language cause, then
   retry, reassign or take over the legacy SCADA terminal when manual control is needed.
4. **Supervise handoff** — take over field equipment or the control-room screen when it needs manual
   control, and leave an attributable record (REQ-029).
5. **Prove and archive** — keep versioned well logs, P&IDs, inspection certificates and MOC records,
   open each in the thread where it was produced, and never render untrusted content in the app document.

## 2. Five concrete UI changes

All copy is a new `lib/i18n.ts` key via `useTranslation` / `t(locale, key)`; no string in JSX. Every
class list goes through `cx()`; every colour rides the `docs/UI.md` token map; type uses composite
utilities. Bot-produced values are untrusted text, never HTML.

**(a) Production-pulse strip — `/app/:botId` thread header.** A read-only band above the transcript
with chips for "nominal", "watch" and "alarm", each with a `Tooltip` carrying the well, current rate
and alarm setpoint. Components: `Badge`/`Chip`, `Tooltip`, `IconButton` (icons from `@remixicon/react`
as references). Tokens: `bg-background-secondary-default`, `border-separator-border`,
`text-text-primary`/`text-text-secondary`, `text-foreground-icon-secondary`, `text-caption-1-semibold`.
RPCs (CONFIRMED): `botsGet`, `threadsHead`, `runsList`. Well ids, dates and rates stay LTR in `<bdi>`.

**(b) Permit / work-order approval card — `/app/:botId` pending card (REQ-007, REQ-017).** Make the
"what will happen" sentence dominant: exact well, tag, volume and effect, with an explicit confirm
label ("Inject 40 L corrosion inhibitor at Well-7, 16:00", not just "Approve"). High-pressure or
HSE-critical changes pair `border-border-error-default` + `text-text-error-primary` with an icon and
the words "Critical" / "HSE hold" — never colour alone. Components: BoardUI `Button`, `Badge`,
`Tooltip`; card `bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`, focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`. RPCs (CONFIRMED): `threadsAnswer`
(exact-once, keeps the `CONFLICT` guard), `threadsReact`. Reuse `ThreadComposer`'s `awaitingApproval`
prop so the transcript stays readable and other threads reachable; announce the card once through the
polite live region (§8.9), not the transcript.

**(c) Named alarm / excursion ledger — `/app` activity (REQ-011, REQ-012, REQ-021).** Follow
`ActivityScreen`: every row names its state out loud with a plain caption (`queued` → "work order
queued", `running` → "crew on site", `waiting_takeover` → "awaiting SCADA handoff", `failed` → "alarm /
trip"); the canonical §8.9 vocabulary stays, the caption is secondary text, never a colour-only chip.
A failed row gives one sentence of cause plus one of what to do and a working `runsRetry`; working rows
keep `runsPause`/`runsResume`; the correlation id sits behind a "details" `Tooltip`, never a stack trace
or provider text. Components: `Button`, `Badge`, `Chip`, `Tooltip`; status via `runStatusKey` and
`isRetryable` from `lib/run-status.ts`. RPCs (CONFIRMED): `runsList`, `runsRetry`, `runsPause`,
`runsResume`, `threadsHead`. Background (ticket) runs appear here, not in the transcript (§8.4).

**(d) SCADA / control-room takeover panel — `/app/:botId` right panel (REQ-026, REQ-028, REQ-029).**
Follow `ComputerPanel`: name each state (starting, running, awaiting takeover, released) and add a
plain caption above the controls: "Leaving this screen pauses nothing — the handoff record stays open."
Takeover and release are explicit, attributable actions. Components: `Button`, `Badge`, installed
spinner/skeleton; status text + icon. RPCs (CONFIRMED): `computerStatus`, `computerTakeover`,
`computerRelease`, `computerOpenScreen`, `computerOpenScreenFrame`. Mirror drawer motion in RTL; leave
physical screen coordinates untouched; no field data in logs.

**(e) Work-order board and engineering-document archive — `/app/board` (REQ-038) and `/app/artifacts`
(REQ-036).** In `BoardScreen`, retitle the flag-gated `COLUMNS` as planned → permitted → in progress →
signed off, each card showing work-order key, title, assignee bot and last comment; the named detail
panel shows attached runs. With the flag off the route is absent, not locked (§8.6). Components
already used there: `Button`, `Input`, `Select`, installed `table`/`chip`; surface
`bg-background-primary-default`, hairline `border-separator-border`, status text + icon. RPCs
(CONFIRMED): `boardsGet`, `ticketsCreate`, `ticketsMove`, `ticketsComment`. In `ArtifactsGrid`, show
well logs, P&IDs, inspection certificates and MOC records with type/bot filters, a version selector,
download and "open in thread" via `artifactsListSpace`, `artifactsGet`, `artifactsListVersions`.
Artifact content is untrusted — never inject its HTML into the document; keep the robot-cat empty state.

## 3. Accessibility, i18n and RTL

- All visible strings go through `t(locale, key)` (English source, fr/he translations, a missing key
  falls back to English — never blank or the raw key). No string hardcoded in JSX.
- `DirectionProvider` (`@/components/foundations/direction/direction`) takes an explicit BCP 47 locale;
  the boot script writes `lang`/`dir` together. Use logical utilities (`ms`/`me`, `ps`/`pe`,
  `text-start`/`text-end`, `border-s`/`border-e`) and `useDirection()` to mirror the rail, board columns
  and drawer/takeover motion in RTL; navigation arrows flip. Well ids, tag numbers, rates, dates, money
  and URLs stay LTR in `<bdi>`/`dir="auto"`; money in integer minor units via `Intl` (ADR-023).
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px `.boma-target` on
  touch controls; token-driven visible focus; a keyboard path for every decision (authorise, retry,
  takeover, board move, version select); status and critical/HSE markers are text + icon using the exact
  §8.9 vocabulary, never colour alone; 360 px width; `prefers-reduced-motion` honoured; no animation on
  streaming content.
- Error copy is one sentence of what happened plus one of what to do — no well id, rate, stack trace or
  provider text on screen (correlation id behind "details" only). Tables carry `<caption>` and
  `<th scope>`; a pending card takes focus; the uniform unavailable screen is announced in identical
  text; the transcript is not a live region, so streaming tokens are not read aloud.

## 4. Playwright tests (REQ id in each name)

```
test("REQ-001 the field composer sends one optimistic message that is not duplicated", ...)
test("REQ-007 a pending permit card takes focus and does not block reading or other threads", ...)
test("REQ-009 the alarm divider moves and the snooze shelf returns a notice on time", ...)
test("REQ-011 the activity ledger names each field-run state with a plain caption", ...)
test("REQ-012 every run status renders a distinct named chip with the right terminal action", ...)
test("REQ-017 a critical permit names well, tag, volume, consequence and answers once", ...)
test("REQ-021 an HSE excursion shows a plain-language cause and a working retry", ...)
test("REQ-026 the SCADA handoff panel names starting, running, takeover and released states", ...)
test("REQ-028 takeover and release announce their status through the polite live region", ...)
test("REQ-036 the engineering-document viewer selects a version and opens it in its thread", ...)
test("REQ-038 the work-order board moves a ticket and records a comment through the same RPCs", ...)
test("REQ-057 Hebrew RTL mirrors the board and axe reports zero critical or serious findings", ...)
```

New behaviour without a REQ yet (production-pulse strip, critical/HSE marker, alarm caption) must be
assigned an id by the owner before code — do not invent one in a test name per the traceability gate.

## 5. Best first slice

**Change (b), the permit / work-order approval card** — GUESS, recommended. Smallest and highest-value:
one screen, no new transport (`threadsAnswer`/`threadsReact` exist), no new component (BoardUI
`Button`/`Badge`/`Tooltip` are installed) and no token re-tune. It directly prevents a high-pressure or
HSE-critical action being approved on a colour cue. Test-first: write `REQ-017 a critical permit names
well, tag, volume, consequence and answers once` and extend `spec/golden/ui/REQ-007-pending-card.json`
first; ship only the dominant consequence line, the explicit confirm label, the text+icon critical
marker, keyboard operability and the polite announcement. Defer (a), (c), (d), (e) until the owner
assigns any missing REQ ids and approves.

Provenance: RPC names and the `CONFLICT` guard CONFIRMED from `apps/web/lib/api/rpc.ts`; component
structure (`ActivityScreen` status/retry, `ComputerPanel` takeover/release) CONFIRMED by reading the
files this session; REQ ids and golden names CONFIRMED from `spec/01-requirements.md`, `docs/UI.md` and
`spec/golden/ui/`. The persona, its jobs and the oil-and-gas mapping are GUESS (proposal only — no such
sector or REQ exists in the spec). No product code was modified.
