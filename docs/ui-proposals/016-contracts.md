# UI proposal 016 — contracts (self-hosted contract lifecycle desk)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #016. Read-only on product code;
only this file is written. Grounding read this session: `docs/UI.md`, `spec/08-ui-ux.md`,
`apps/web/lib/api/rpc.ts` (RPC names), and two components under `apps/web/components/`:
`thread/message-blocks.tsx` (`CardBlock`) and `thread/thread-composer.tsx` (`ThreadComposer`).
Charter held: Boma Bot only; no upstream brand, code, wording or assets; BoardUI semantic tokens
only — no raw palette classes or hex literals — classes merged with `cx()`; one design system, no
second; no SaaS (runs on the organisation's own machine, no contract text leaving the building);
robot-cat identity and product name unchanged.

## 1. Persona and top jobs-to-be-done

**Persona — Amal, an in-house contracts manager at a mid-size cooperative.** She reviews, redlines and
tracks supplier and customer agreements, works a shared workstation, and serves English, French and
Arabic colleagues. Boma Bot runs self-hosted on the organisation's own machine: no cloud, no contract
text or counterparty data leaving the building. A contract decision is legally and financially
binding, so "what will happen" must be explicit and attributable — an ambiguous confirmation is
worse than a slow screen.

Top jobs:
1. **Review safely** — see a bot-proposed clause change or approval and be certain of the exact
   clause, counterparty and effect before committing; never approve by colour alone.
2. **Triage** — find which reviews are waiting on her, which are overdue, and which automated runs
   (obligation checks, renewals) are stalled, without scrolling a transcript.
3. **Draft** — capture a clause note or a redline instruction quickly, without losing the draft to a
   stray Enter.
4. **Track obligations** — let the bot work the repository on-screen, and take over cleanly when it
   needs her hands, with an attributable record.
5. **Archive and report** — keep a versioned, searchable record of executed agreements and open each
   version in the thread where it was produced.

## 2. Five concrete changes

All new copy is catalogued through `t(locale, key)` (`lib/i18n.ts`); nothing is hardcoded in JSX. All
reuse BoardUI components already vendored in `apps/web/components/` and the token map in `docs/UI.md`.

**A. Review-queue context strip — `/app/:botId` thread header.**
A compact, read-only band above the transcript with chips for "awaiting signature", "overdue
renewal" and "in review", each with a `Tooltip` for detail. Components: `Badge`/`Chip`
(`components/base/badges/chip.tsx`), `Tooltip`, `IconButton` (icons from `@remixicon/react` passed as
references). Tokens: surface `bg-background-secondary-default`, hairline `border-separator-border`,
text `text-text-primary`/`text-text-secondary`, icon `text-foreground-icon-secondary`, type
`text-caption-1-semibold`, joined with `cx()`. Existing RPCs: `botsGet`, `threadsHead`, `threadsMessages`.
Bot-produced values are untrusted text, never HTML; counterparty names and dates stay LTR.

**B. Clause-approval card — `/app/:botId` pending card (`components/thread/message-blocks.tsx`,
REQ-007/REQ-017).**
Make the "what will happen" sentence the dominant line: exact clause, counterparty and effect, plus a
distinct confirm label ("Approve clause", not just "Approve"). High-impact changes use
`border-border-error-default` + `text-text-error-primary` **plus** a leading icon and the words
"High impact" — never colour alone. Components: `Button`, `Badge`, `Tooltip`; card surface
`bg-background-secondary-default`, `rounded-3xl`, `border-separator-border`, focus
`focus-visible:ring-border-focus-ring`. RPC: `threadsAnswer` (already present, exactly-once effect,
with the existing `CONFLICT` guard for double answers). Announced once through the polite live region;
the transcript stays readable and other threads reachable (spec §8.9). No new transport.

**C. Clause-note composer — `/app/:botId` composer (`components/thread/thread-composer.tsx`,
REQ-001).**
Extend `ThreadComposer` with a named "attach clause" `IconButton` (`RiFileTextLine`) that offers the
current agreement as a reply-style chip, reusing the existing reply-banner pattern. Keep
Enter-to-send / Shift+Enter-newline exactly as today, and keep send disabled while an approval is
pending (`awaitingApproval`). Component: BoardUI `Button`/`IconButton` + `Tooltip`; focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`, 44 px `.boma-target`, icon
`text-foreground-icon-primary`. RPC `threadsSend`; the attached reference enters the same `value`
state, so no new transport. Agreement numbers and dates stay LTR in `<bdi>`.

**D. Repository takeover panel — `/app/:botId` computer panel (REQ-026/REQ-028/REQ-029).**
Add a named status for each computer state (starting, running, awaiting takeover, released) and a
plain caption above takeover/release controls: "Leaving this screen pauses nothing — the record stays
open." Components: `Button`, `Badge` and the installed spinner/skeleton states; surface
`bg-background-secondary-default`, text `text-text-secondary`, status as text + icon, never colour
only. RPCs: `computerStatus`, `computerTakeover`, `computerRelease`, `computerOpenScreen`. Mirror
drawer motion in RTL; keep physical screen coordinates untouched; no contract text in logs.

**E. Agreement lifecycle on the board and in the archive — `/app/board` (REQ-038) and
`/app/artifacts` (REQ-036).**
Retitle the four flag-gated board columns as requested → in review → approved → executed, each card
showing agreement key, title, assigned bot and last comment; a named detail panel shows the attached
obligation runs. Components: installed `table`/`chip`, `Button`, `input`, `select`, `Badge`; surfaces
`bg-background-primary-default` and `bg-background-secondary-default`, hairline
`border-separator-border`, status text + icon. RPCs: `boardsGet`, `ticketsCreate`, `ticketsMove`,
`ticketsComment`. In the artifact grid/viewer, show executed agreements with version selector,
download and "open in thread" via `artifactsListSpace`, `artifactsGet`, `artifactsListVersions`.
Content is untrusted: never inject artifact/HTML into the document. When the flag is off the board
route is absent, not locked; background runs appear in Activity, not in the transcript.

## 3. Accessibility, i18n and RTL

- All visible strings go through `t(locale, key)` (English source; French/Hebrew translations; a
  missing key falls back to English, never a blank or the key). No string in JSX.
- `DirectionProvider` sets locale/direction; the boot script writes `lang`/`dir` together. Use logical
  utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Mirror takeover and
  lifecycle arrows via `useDirection()`; keep agreement numbers, dates, money and URLs LTR in
  `<bdi>`/`dir="auto"`, formatted with `Intl` (money in integer minor units, never floats).
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets via
  `.boma-target`; token-driven visible focus; full keyboard path (clause decision, attach, takeover,
  board move, version select); run status is text + icon, never colour alone (spec §8.9 vocabulary);
  360 px width; `prefers-reduced-motion`; no animation on streaming content.
- The high-impact marker is text + icon, and the clause decision is announced once via the polite
  live region — the transcript is not a live region, so streaming tokens are not read aloud. Error
  copy is one sentence of what happened plus one of what to do; no counterparty data, ids, stack
  traces or provider text on screen (correlation id behind "details" only). The robot-cat mark stays
  unchanged.

## 4. Playwright tests (REQ ids in the name)

```
test("REQ-001 the clause-note composer sends one optimistic message that is not duplicated", ...)
test("REQ-007 a pending clause card takes focus and does not block reading or other threads", ...)
test("REQ-009 the review-queue divider moves and the snooze shelf returns a review on time", ...)
test("REQ-012 every run status renders as text plus icon in light, dark and RTL", ...)
test("REQ-017 a high-impact clause names the clause, counterparty and effect and answers once", ...)
test("REQ-026 the repository panel names starting, running, takeover and released states", ...)
test("REQ-036 the archive viewer selects a version and opens it in the originating thread", ...)
test("REQ-038 the agreement lifecycle moves a ticket and records a comment through the same RPCs", ...)
test("REQ-057 the desk screens mirror for Hebrew with a clean axe run", ...)
```
Behaviour above without a REQ yet (review-queue strip, high-impact marker, attach-clause chip) must be
assigned a REQ id by the owner before implementation per the traceability gate — do not invent one in
test names.

## 5. Highest-impact first slice

**Change B — the clause-approval card.** It is the cheapest and the only change that directly reduces
legal and financial risk: `threadsAnswer`, `Button` and `Badge` are installed, the semantic tokens
exist, and no new RPC, component install or data model is needed. Test-first: write
`REQ-017 a high-impact clause names the clause, counterparty and effect and answers once` and extend
`spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant effect sentence, the explicit
confirm label, the attribution line, the text+icon high-impact marker, keyboard operability and the
polite announcement. Defer (A), (C), (D), (E) until the owner assigns REQ ids and approves.

Provenance: REQ ids verified against `spec/01-requirements.md`; RPC names and the `CONFLICT` guard
read this session from `apps/web/lib/api/rpc.ts`; component structure read from `message-blocks.tsx`
and `thread-composer.tsx`; design rules from `docs/UI.md` and `spec/08-ui-ux.md`. All are CONFIRMED
readings. The persona and its jobs are a GUESS (proposal only — no contracts sector exists in the
spec). No product code was modified.
