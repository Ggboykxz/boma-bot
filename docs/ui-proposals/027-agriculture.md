# UI proposal 027 — agriculture (self-hosted field-operations desk)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #027. Read-only on product code;
only this file is written. Grounding read this session: `docs/UI.md`, `spec/08-ui-ux.md`,
`spec/01-requirements.md` (REQ ids), `apps/web/lib/api/rpc.ts` (RPC names) and two components under
`apps/web/components/`: `thread/thread-composer.tsx` and `board/board-screen.tsx`. Charter held: Boma
Bot only — no upstream brand, code, wording or assets; BoardUI semantic tokens only, no raw palette
classes or hex literals, classes merged with `cx()`; one design system, no second; no SaaS (runs on the
cooperative's own machine, no plot, sensor or spray record leaving it); robot-cat identity unchanged.

## 1. Persona and top jobs-to-be-done

**Persona — Mr. Karim, an agronomist managing several plots for a farming cooperative.** He scouts
fields, schedules irrigation and treatment runs, verifies bot-proposed interventions and keeps
traceable treatment/yield records, often from a tablet in the field with a poor connection. He serves
French, English and Arabic colleagues. Boma Bot runs on the cooperative's own machine: no cloud, no
field data leaving the site. An intervention released to the wrong plot, or approved on a colour cue
alone, is a real harm — ambiguity is worse than latency.

Top jobs:
1. **Monitor** — see which scouting/irrigation runs are working, stalled or failed, without scrolling a
   transcript; retry or pause one in place with a link back to the message that started it.
2. **Approve** — review a bot-proposed spray or irrigation and be certain of the plot, dose and effect
   before approving; never decide by colour alone.
3. **Command** — ask the bot from the field and attach a field note or photo without losing the draft
   to a stray Enter; keep plot codes and dates readable.
4. **Plan** — move field tasks (planting, irrigation, treatment, harvest) through a board, each card
   naming its plot, assignee bot and last comment.
5. **Record and share** — keep a versioned treatment/yield record library and publish a report to the
   cooperative without giving every member a login.

## 2. Five concrete changes

All new copy is catalogued through `t(locale, key)` (`lib/i18n.ts`); nothing is hardcoded in JSX. All
reuse BoardUI components already vendored in `apps/web/components/` and the token map in `docs/UI.md`.
Bot-produced values are untrusted text, never HTML.

**A. Field-context strip — `/app/:botId` thread header.** A compact, read-only band above the
transcript with chips for "plot", "crop", "growth stage" and "open alerts", each with a `Tooltip` for
detail. Components: `Chip`/`Badge` (`components/base/badges/chip.tsx`), `Tooltip`, `IconButton` (icons
from `@remixicon/react` passed as references). Tokens: surface `bg-background-secondary-default`,
hairline `border-separator-border`, text `text-text-primary`/`text-text-secondary`, icon
`text-foreground-icon-secondary`, type `text-caption-1-semibold`, joined with `cx()`. Existing RPCs:
`botsGet`, `threadsHead`, `threadsMessages`.

**B. Intervention-approval card — `/app/:botId` pending card (`REQ-007`/`REQ-017`).** Make the "what
will happen" sentence dominant: exact plot, product/dose and effect, plus a distinct confirm label
("Spray Plot 4" or "Start irrigation", not just "Approve"). High-impact treatments use
`border-border-error-default` + `text-text-error-primary` **plus** a leading icon and the words
"High impact" — never colour alone. Components: BoardUI `Button`, `Badge`, `Tooltip`; card surface
`bg-background-secondary-default`, `rounded-3xl`, `border-separator-border`, focus
`focus-visible:ring-border-focus-ring`. RPC: `threadsAnswer` (already present, exactly-once effect,
with the `CONFLICT` guard `message-blocks.tsx` already handles for double answers). Announced once via
the polite live region; the transcript stays readable and other threads reachable (spec §8.9).

**C. Field-command composer — `/app/:botId` composer (`components/thread/thread-composer.tsx`,
`REQ-001`).** Extend the composer with a named "attach field note" `IconButton` (`RiFileTextLine`)
offering a photo/note as a reply-style chip, reusing the existing `ReplyContext` banner pattern and the
installed `file-upload` component. Keep Enter-to-send / Shift+Enter-newline exactly as today and keep
send disabled while an approval is pending (`awaitingApproval`). Component: BoardUI `IconButton` +
`Tooltip`; focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`, 44 px `.boma-target`,
icon `text-foreground-icon-primary`. RPC `threadsSend`; the attachment enters the same `value` state,
so no new transport. Plot codes, dates and doses stay LTR in `<bdi>`.

**D. Field-ops run monitor — `/app` Activity view (`REQ-011`/`REQ-012`).** In the run-centric Activity
list, group scheduled scouting and irrigation runs by day with prompt snippet, bot name, a named
status chip, duration and a link back to the starting message. Working runs sort to the top and update
in place; failures get a plain cause and a retry, never a decorative spinner. Components: installed
`table`/`chip`, `Button`, `IconButton`, `Tooltip`; surfaces `bg-background-primary-default` and
`bg-background-secondary-default`, hairline `border-separator-border`, status as text + icon. RPCs:
`runsList`, `runsRetry`, `runsPause`, `runsResume`, `threadsHead`. Background (scheduled) runs appear
here, not in the transcript.

**E. Field task board and record library — `/app/board` (`REQ-038`) and `/app/artifacts` (`REQ-036`).**
Retitle the four flag-gated columns of `board-screen.tsx` as to-sow → in progress → treated → harvested;
each card shows plot key, title, assignee bot and last comment, and the detail dialog shows attached
runs. Replace the raw `<input>`/`<select>` there with the installed BoardUI `Input`/`Select` to honour
the one-system rule. Components: installed `table`/`chip`, `Button`, `Input`, `Select`, `Badge`;
surfaces `bg-background-primary-default`/`bg-background-secondary-default`, `border-separator-border`,
status text + icon. RPCs: `boardsGet`, `ticketsCreate`, `ticketsMove`, `ticketsComment`, `botsList`.
In the library/viewer, show treatment and yield records with a version selector, download and "open in
thread" via `artifactsListSpace`, `artifactsGet`, `artifactsListVersions` (and `artifactsRemove`),
reusing the bounded frame and untrusted-byte rules of `artifact-viewer.tsx`. When the flag is off the
board route is absent, not locked. A compiled report can be published through `/p/:slugToken`
(`REQ-037`) with no session or chrome beyond a small robot-cat footer.

## 3. Accessibility, i18n and RTL

- All visible strings go through `t(locale, key)` (English source; French/Hebrew translations; a
  missing key falls back to English, never a blank or the key). No string in JSX.
- `DirectionProvider` sets locale/direction; the boot script writes `lang`/`dir` together. Use logical
  utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Mirror the attach
  and board-column arrows via `useDirection()`; keep plot codes, URLs, dates and numbers LTR in
  `<bdi>`/`dir="auto"`, formatted with `Intl` (no floats for areas or doses).
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets via
  `.boma-target`; token-driven visible focus; full keyboard path (approval decision, attach, retry/
  pause, board move via the existing focus-trapped dialog, version select); run status is text + icon,
  never colour alone (spec §8.9 vocabulary); 360 px width; `prefers-reduced-motion`; no animation on
  streaming content.
- The high-impact marker is text + icon, and the intervention decision is announced once via the
  polite live region — the transcript is not a live region, so streaming tokens are not read aloud.
  Error copy is one sentence of what happened plus one of what to do; no plot ids, stack traces or
  provider text on screen (correlation id behind "details" only). The robot-cat mark stays unchanged.

## 4. Playwright tests (REQ ids in the name)

```
test("REQ-001 the field-command composer sends one optimistic message that is not duplicated", ...)
test("REQ-007 a pending intervention card names plot, dose and effect and does not block reading", ...)
test("REQ-011 the activity view lists working field runs and links back to the starting message", ...)
test("REQ-012 every run status renders as text plus icon in light, dark and RTL", ...)
test("REQ-017 answering the intervention card reopens the composer and answers exactly once", ...)
test("REQ-021 a failed irrigation run shows a plain cause and a working retry action", ...)
test("REQ-036 the records viewer selects a version and opens it in the originating thread", ...)
test("REQ-038 the field task board moves a ticket by keyboard and hides when the flag is off", ...)
test("REQ-057 the agriculture screens mirror for Hebrew with a clean axe run", ...)
```
Behaviour above without a REQ yet (field-context strip, attach-note chip, high-impact marker) must be
assigned a REQ id by the owner before implementation per the traceability gate — do not invent one in
test names.

## 5. Highest-impact first slice

**Change B — the intervention-approval card.** It is the cheapest and the only change that directly
reduces the risk of a treatment released to the wrong plot through an ambiguous confirmation:
`threadsAnswer`, `Button` and `Badge` are installed, the semantic tokens exist, and no new RPC,
component install or data model is needed (`message-blocks.tsx` already owns the `CONFLICT` guard).
Test-first: write `REQ-017 answering the intervention card reopens the composer and answers exactly
once` and extend `spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant effect
sentence, the explicit confirm label, the plot/dose attribution, the text+icon high-impact marker,
keyboard operability and the polite announcement. Defer (A), (C), (D), (E) until the owner assigns REQ
ids and approves.

Provenance: REQ ids verified against `spec/01-requirements.md`; RPC names read this session from
`apps/web/lib/api/rpc.ts`; component structure read from `thread-composer.tsx` and `board-screen.tsx`;
design rules from `docs/UI.md` and `spec/08-ui-ux.md`. All are CONFIRMED readings. The persona and its
jobs are a GUESS (proposal only — no agriculture sector exists in the spec). No product code was
modified.
