# UI proposal 044 — telecom (network operations centre)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #044 — telecom. Read-only on
product code; only this file was written. Grounding read this session: `docs/UI.md`,
`spec/08-ui-ux.md`, `spec/01-requirements.md` (REQ ids), `apps/web/lib/api/rpc.ts` (RPC names), and
two components under `apps/web/components/`: `thread/computer-panel.tsx` and
`activity/activity-screen.tsx`. Charter held: Boma Bot only — no upstream brand, code, assets or
wording; BoardUI semantic tokens only — no raw palette class or hex literal, classes merged with
`cx()`; one design system, no second; no SaaS (self-hosted at the operator — subscriber records,
topology and alarms never leave the premises); robot-cat identity unchanged.

## 1. Persona and top jobs-to-be-done

**Persona — Nadia, a duty engineer in a regional telecom operator's NOC.** She watches alarms and
provisioning runs across mobile, fixed-line and fibre, alternates between a wall console and a
laptop on a poor link, and verifies bot-proposed network changes before release. Teams work in
French, English and Arabic. Boma Bot runs on the operator's own server: subscriber data, topology
and alarm history never leave the building. A change pushed to the wrong network element, or
approved on a colour cue alone, is a customer-facing outage — legible state and an explicit
"what will happen" matter more than a fast screen.

Top jobs:
1. **Monitor** — see which alarm-correlation, provisioning and test runs are working, stalled or
   failed without hunting through a transcript; retry only failures.
2. **Verify before committing** — confirm a bot-proposed config push or subscriber change knowing
   the exact element, effect and sequence; never approve on colour alone.
3. **Intervene** — take exclusive control of a router/switch management console for a manual step
   and hand it back with an attributable record.
4. **Triage** — find alarms, exceptions and unread items, tell working runs from stalled ones, and
   snooze non-urgent alerts with a named due time (REQ-067 pause/resume).
5. **Prove and share** — keep versioned change/incident records, open each in the thread where it
   was produced, and publish a regulator or customer summary without giving every reader a login.

## 2. Five concrete changes

All new copy is catalogued through `t(locale, key)` (`lib/i18n.ts`; English source, French/Arabic
translations, a missing key falls back to English — never blank or the raw key). Every colour rides
the `docs/UI.md` token map, type uses composite utilities, class lists go through `cx()`. Icons come
from `@remixicon/react` as component references. Bot-produced values are untrusted text, never HTML.

**A. Network-context strip — `/app/:botId` thread header.** A compact read-only band above the
transcript with chips for "active alarms", "open incidents" and "pending changes", each with a
`Tooltip` carrying detail (element, count, severity). Components: `Chip`/`Badge`
(`components/base/badges/chip.tsx`), `Tooltip`, `IconButton`. Tokens: `bg-background-secondary-default`,
`border-separator-border`, `text-text-primary`/`text-text-secondary`,
`text-foreground-icon-secondary`, `text-caption-1-semibold`, joined with `cx()`. Existing RPCs:
`botsGet`, `threadsHead`, `threadsMessages`. Element ids and timestamps stay LTR in `<bdi>`.

**B. Network-change approval card — `/app/:botId` pending card (`REQ-007`/`REQ-017`).** Make "what
will happen" dominant: exact element, effect and sequence, with an explicit confirm label ("Push
config to element RTR-04", not just "Approve"). High-impact changes pair
`border-border-error-default` + `text-text-error-primary` with a leading icon and the words
"High impact" — never colour alone. Components: BoardUI `Button`, `Badge`, `Tooltip`; card surface
`bg-background-secondary-default`, `rounded-3xl`, `border-border-button-default`, focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`. Existing RPC `threadsAnswer`
(exactly-once effect, keeps the `CONFLICT` guard). Announced once via the polite live region; the
transcript stays readable and other threads reachable (spec §8.9).

**C. Console-takeover panel — `/app/:botId` right panel (`REQ-026`/`REQ-028`/`REQ-029`).** Extend
`computer-panel.tsx`: give each state a name (starting, running, awaiting takeover, released) plus
a plain caption — "Leaving this screen pauses nothing — the handoff record stays open." Takeover
and release stay explicit, attributable actions. Components already used there: `Button`, status
`role="status"`, `text-text-error-primary` error `role="alert"`; surface
`bg-background-primary-default`, hairline `border-border-button-default`, `text-caption-1-semibold`.
Existing RPCs: `computerStatus`, `computerOpenScreen`, `computerOpenScreenFrame`, `computerTakeover`,
`computerRelease`. Mirror drawer motion in RTL; leave physical screen coordinates untouched; no
subscriber data in logs.

**D. Alarm-run triage — `/app` Activity view (`REQ-011`/`REQ-012`/`REQ-021`/`REQ-067`).** Extend
`ActivityScreen` so each run row names its state out loud and adds a telecom caption (`queued` →
"change queued", `running` → "in progress", `waiting_takeover` → "awaiting console handoff", `failed`
→ "incident") — the canonical §8.9 vocabulary stays, the caption is secondary text. Working/paused
rows keep Pause/Resume (REQ-067), failed rows keep Retry (REQ-021); working runs sort to the top and
update in place, no decorative spinner. Components: `Button`, `Chip`, `Badge`; surfaces
`bg-background-primary-default`/`bg-background-secondary-default`, `border-separator-border`,
error row `border-border-error-default` + `text-foreground-icon-error`; status via `runStatusKey`/
`isRetryable`. Existing RPCs: `runsList`, `runsPause`, `runsResume`, `runsRetry`, `threadsHead`.
Background test runs appear here, not the transcript (spec §8.4).

**E. Service-order board, change-record library and shared report — `/app/board` (`REQ-038`),
`/app/artifacts` (`REQ-036`), `/p/:slugToken` (`REQ-037`).** In `board-screen.tsx`, retitle the four
flag-gated columns as reported → assigned → in progress → resolved; each card shows order key,
title, assignee bot and last comment, with attached test runs in the detail panel. Replace raw
`<input>`/`<select>` there with installed BoardUI `Input`/`Select`. RPCs: `boardsGet`,
`ticketsCreate`, `ticketsMove`, `ticketsComment`. In the library, show change/incident records with
a version selector, download and "open in thread" via `artifactsListSpace`, `artifactsGet`,
`artifactsListVersions`; never inject artifact HTML into the document. With the flag off the board
route is absent, not locked. A compiled regulator/customer report can be published through
`/p/:slugToken` with no session or product chrome beyond a small robot-cat footer; unknown,
revoked and expired tokens show one identical "not available" screen — same wording and timing —
with `noindex`, `no-referrer` and its own CSP.

## 3. Accessibility, i18n and RTL

- All visible strings go through `t(locale, key)`, no string hardcoded in JSX. `DirectionProvider`
  takes the explicit BCP 47 locale; the boot script writes `lang`/`dir` together. Use logical
  utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`) and `useDirection()`
  to mirror the attach affordance, board columns and drawer motion in RTL (REQ-057). Element ids,
  URLs, dates, numbers and tariff money stay LTR in `<bdi>`/`dir="auto"`, money in integer minor
  units via `Intl` (ADR-023).
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px `.boma-target`
  on touch controls; token-driven visible focus; a keyboard path for every decision (approval,
  takeover/release, pause/resume/retry, board move, version select); run status and high-impact
  markers are text + icon, never colour alone (spec §8.9); 360 px width; `prefers-reduced-motion`
  honoured; no animation on streaming content.
- Error copy is one sentence of what happened plus one of what to do — no element ids, subscriber
  data, stack traces or provider text (correlation id behind "details" only, spec §8.9). The
  approval decision is announced once via the polite live region; the transcript is not a live
  region, so streaming tokens are not read aloud. The robot-cat mark stays unchanged.

## 4. Playwright tests (REQ ids in the names)

```
test("REQ-001 the network-change composer sends one optimistic message that is not duplicated", ...)
test("REQ-007 a pending change card names element, effect and sequence and does not block reading", ...)
test("REQ-017 a high-impact config push answers once and executes its effect once", ...)
test("REQ-011 the activity view groups alarm and provisioning runs by day and links back", ...)
test("REQ-012 every run status renders as text plus icon in light, dark and RTL", ...)
test("REQ-021 a failed run shows a plain cause and a working retry action", ...)
test("REQ-026 the console panel names starting, running, takeover and released states", ...)
test("REQ-028 observe and act on the console through the existing RPCs", ...)
test("REQ-029 takeover and release announce their status and pause the bot", ...)
test("REQ-036 the change-record library selects a version and opens it in its thread", ...)
test("REQ-038 the service-order board moves a ticket by keyboard and hides when the flag is off", ...)
test("REQ-037 the shared report renders without chrome; an invalid token shows the uniform screen", ...)
test("REQ-057 the telecom screens mirror for Hebrew/Arabic with dir=rtl and a clean axe run", ...)
```

Behaviour without a REQ yet (network strip, high-impact marker, telecom captions on canonical chips)
must be assigned a REQ id by the owner before implementation — do not invent one in test names.

## 5. Highest-impact first slice

**Change B — the network-change approval card.** It is the cheapest and the only change that directly
reduces the risk of a config push to the wrong element through an ambiguous confirmation:
`threadsAnswer`, `Button` and `Badge` are installed, the semantic tokens exist, and no new RPC,
component install, data model or token re-tune is needed. Test-first: write `REQ-017 a high-impact
config push answers once and executes its effect once` and extend
`spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant effect sentence, the
explicit confirm label, element attribution, the text+icon high-impact marker, keyboard operability
and the polite announcement. Defer (A), (C), (D), (E) until the owner assigns REQ ids and approves.

Provenance: REQ ids CONFIRMED from `spec/01-requirements.md`; RPC names CONFIRMED from
`apps/web/lib/api/rpc.ts`; component structure (`ComputerPanel` status/takeover/release,
`ActivityScreen` `runStatusKey`/`isRetryable`) CONFIRMED by reading the two files this session; design
rules CONFIRMED from `docs/UI.md` and `spec/08-ui-ux.md`. The persona, its jobs and the telecom
mapping are GUESS (proposal only — no telecom sector or REQ exists in the spec). No product code was
modified.
