# UI proposal 099 — water supply and treatment

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #099. Read-only on product code;
only this file was written. Grounding read this session: `docs/UI.md`, `spec/08-ui-ux.md`,
`spec/01-requirements.md` (REQ ids), `apps/web/lib/api/rpc.ts` (RPC names) and two components under
`apps/web/components/`: `activity/activity-screen.tsx` and `thread/message-blocks.tsx`. Charter held:
Boma Bot only — no upstream brand, code, wording or assets; BoardUI semantic tokens only, never raw
palette classes or hex literals, classes merged with `cx()`; one design system, no second; no SaaS
(self-hosted on the utility's own machine — no reservoir level, valve state, dose or water-quality
record leaves the site); robot-cat identity unchanged. Water-only (reservoirs, zones, dosing) — not #037's electricity/gas scope.

## 1. Persona and top jobs-to-be-done

**Persona — Salma, a water-network and water-quality duty manager at a municipal water utility.**
She works a multilingual control room (French, English, Arabic), alternates between a console and a
gloved tablet at a valve chamber on a poor link, and verifies bot-proposed actions against pressure
and quality readings. Boma Bot runs on the utility's own server: zone topology, reservoir levels,
dose logs and quality records never leave the premises. A chlorine dose released at the wrong rate,
or a valve closed on the wrong main, is a public-health or supply event — ambiguity is worse than
latency.

Top jobs:
1. **Monitor** — see which level-check, pump, leak-detection and restoration runs are working,
   stalled or failed without hunting through a transcript; retry only failures.
2. **Control** — review a bot-proposed valve operation, pump switch or chlorine/assay dose and be
   certain of the exact zone, effect and sequence before approving; never decide by colour alone.
3. **Report** — log a field reading (turbidity, residual chlorine, pressure) or a leak photo from the
   tablet, attaching a note without losing the draft to a stray Enter.
4. **Dispatch** — move restoration jobs (reported → dispatched → in progress → restored) on a board,
   each naming its job key, assignee bot and last comment.
5. **Record and share** — keep versioned water-quality logs, dose records and meter reads, and
   publish a regulator or consumer summary without giving every reader a login.

## 2. Five concrete changes

All new copy is catalogued through `t(locale, key)` (`lib/i18n.ts`); nothing is hardcoded in JSX. All
reuse BoardUI components already vendored in `apps/web/components/` and the token map in `docs/UI.md`.
Bot-produced values are untrusted text, never HTML. No new RPC, component install or data model.

**A. Zone and level context strip — `/app/:botId` thread header.** A compact, read-only band above
the transcript with chips for "pressure zone", "reservoir level", "line pressure" and "active leaks",
each with a `Tooltip` for detail; trend values use `chart-1`…`chart-5` tokens **plus** a printed
number, never colour alone. Components: `Chip`/`Badge` (`components/base/badges/chip.tsx`),
`Tooltip`, `IconButton` (icons from `@remixicon/react` passed as references). Tokens: surface
`bg-background-secondary-default`, hairline `border-separator-border`, text `text-text-primary`/
`text-text-secondary`, icon `text-foreground-icon-secondary`, type `text-caption-1-semibold`,
joined with `cx()`. Existing RPCs: `botsGet`, `threadsHead`, `threadsMessages`.

**B. Dose and valve approval card — `/app/:botId` pending card (`REQ-007`/`REQ-017`).** Make the
"what will happen" sentence dominant: exact valve/pump, dose rate and unit, the effect and the
sequence, plus a distinct confirm label ("Open valve V-12", "Apply 1.2 mg/L" — not just "Approve").
Public-health and supply-critical operations use `border-border-error-default` +
`text-text-error-primary` **plus** a leading icon and the words "High impact" — never colour alone.
Components: BoardUI `Button`, `Badge`, `Tooltip`; card surface `bg-background-secondary-default`,
`rounded-3xl`, `border-separator-border`, focus `focus-visible:ring-border-focus-ring`. RPC
`threadsAnswer` (already present, exactly-once effect, with the `CONFLICT` guard
`message-blocks.tsx` already handles for double answers). Announced once via the polite live region;
the transcript stays readable and other threads reachable (spec §8.9).

**C. Field-reading composer — `/app/:botId` composer (`components/thread/thread-composer.tsx`,
`REQ-001`).** Extend the composer with an "attach reading/photo" `IconButton` (`RiFileTextLine`)
offering a turbidity, residual-chlorine or pressure reading as a reply-style chip, reusing the
existing `ReplyContext` banner and the installed `file-upload` component. Keep Enter-to-send /
Shift+Enter-newline exactly as today and keep send disabled while an approval is pending
(`awaitingApproval`). Component: BoardUI `IconButton` + `Tooltip`; focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`, 44 px `.boma-target`, icon
`text-foreground-icon-primary`. RPC `threadsSend`; the attachment enters the same `value` state, so
no new transport. Zone codes and meter ids stay LTR in `<bdi>`; readings are formatted with `Intl`
and held as integers, never floats.

**D. Leak-repair triage — `/app` Activity view (`REQ-011`/`REQ-012`/`REQ-067`).** Extend
`ActivityScreen` with day grouping and named status chips (`queued`, `leased`, `running`,
`waiting_input`, `waiting_takeover`, `completed`, `failed`, `cancelled`) using `runStatusKey` as text
+ icon; pause/resume queued runs, retry only `failed`. Working runs sort to the top and update in
place; no decorative spinner. BoardUI `Button`, `Chip`; surfaces `bg-background-primary-default` and
`bg-background-secondary-default`, hairline `border-separator-border`, error row
`border-border-error-default` + `text-text-error-primary` + `text-foreground-icon-error`. RPCs:
`runsList`, `runsPause`, `runsResume`, `runsRetry`, `threadsHead`. Scheduled (background) runs appear
here, not in the transcript.

**E. Repair board, records library and shared quality report — `/app/board` (`REQ-038`),
`/app/artifacts` (`REQ-036`) and `/p/:slugToken` (`REQ-037`).** Retitle the four flag-gated columns of
`board-screen.tsx` as reported → dispatched → in progress → restored; each card shows job key, title,
assignee bot and last comment, and the detail dialog shows attached runs. Replace raw `<input>`/
`<select>` there with the installed BoardUI `Input`/`Select` to honour the one-system rule.
Components: installed `Chip`/`Badge`, `Button`, `Input`, `Select`; surfaces
`bg-background-primary-default`/`bg-background-secondary-default`, `border-separator-border`, status
text + icon. RPCs: `boardsGet`, `ticketsCreate`, `ticketsMove`, `ticketsComment`, `botsList`. In the
library, show water-quality logs, dose records and meter reads with a version selector, download and
"open in thread" via `artifactsListSpace`, `artifactsGet`, `artifactsListVersions`, reusing the bounded
frame and untrusted-byte rules of `artifact-viewer.tsx`; keep the robot-cat empty state. When the flag
is off the board route is absent, not locked. A compiled regulator/consumer quality report can be
published through `/p/:slugToken` with no session or product chrome beyond a small robot-cat footer;
unknown, revoked and expired tokens show one identical "not available" screen — same wording and
timing — with `noindex`, `no-referrer` and its own CSP.

## 3. Accessibility, i18n and RTL

- All visible strings go through `t(locale, key)` (English source; French/Hebrew translations; a
  missing key falls back to English, never a blank or the key). No string in JSX.
- `DirectionProvider` sets locale/direction; the boot script writes `lang`/`dir` together. Use logical
  utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Mirror the attach
  affordance, board columns and drawer motion via `useDirection()`; keep zone codes, meter ids, dose
  units, URLs, dates and numbers LTR in `<bdi>`/`dir="auto"`, formatted with `Intl` (integer units for
  readings, no floats).
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets via
  `.boma-target` for gloved field use; token-driven visible focus; full keyboard path (approval
  decision, attach, retry/pause/resume, board moves via the focus-trapped dialog, version select);
  run status is text + icon, never colour alone (spec §8.9 vocabulary); 360 px width;
  `prefers-reduced-motion`; no animation on streaming content.
- The high-impact marker is text + icon, and the valve/dose decision is announced once via the polite
  live region — the transcript is not a live region, so streaming tokens are not read aloud. Level and
  pressure charts carry a printed value, never colour as the sole signal. Error copy is one sentence
  of what happened plus one of what to do; no valve ids, meter ids, stack traces or provider text on
  screen (correlation id behind "details" only). The robot-cat mark stays unchanged.

## 4. Playwright tests (REQ ids in the name)

```
test("REQ-001 the field-reading composer sends one optimistic message that is not duplicated", ...)
test("REQ-007 a pending dose card names valve, rate, effect and sequence and does not block reading", ...)
test("REQ-011 the activity view groups utility runs by day and links back to the starting message", ...)
test("REQ-012 every run status renders as text plus icon in light, dark and RTL", ...)
test("REQ-017 answering the dose card reopens the composer and answers exactly once", ...)
test("REQ-021 a failed leak-detection run shows a plain cause and a working retry action", ...)
test("REQ-036 the records library filters by type and bot and shows the version chain", ...)
test("REQ-038 the restoration board moves a ticket by keyboard and hides when the flag is off", ...)
test("REQ-037 the shared quality report renders without chrome; an invalid token shows the uniform screen", ...)
test("REQ-057 the water screens mirror for Hebrew with a clean axe run", ...)
```
Behaviour without a REQ yet (zone strip, attach-reading chip, high-impact marker, day grouping) needs an
owner-assigned id before implementation — do not invent one in test names.

## 5. Highest-impact first slice

**Change B — the dose and valve approval card.** It is the cheapest and the only change that directly
reduces the risk of a dose or valve action released to the wrong zone through an ambiguous
confirmation: `threadsAnswer`, `Button` and `Badge` are installed, the semantic tokens exist, and no
new RPC, component install or data model is needed (`message-blocks.tsx` already owns the `CONFLICT`
guard). Test-first: write `REQ-007 a pending dose card names valve, rate, effect and sequence` and
extend `spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant effect sentence, the
explicit confirm label, the zone attribution, the text+icon high-impact marker, keyboard operability
and the polite announcement. Defer (A), (C), (D), (E) until the owner assigns REQ ids and approves.

Provenance: REQ ids verified against `spec/01-requirements.md`; RPC names read this session from
`apps/web/lib/api/rpc.ts`; component structure read from `activity-screen.tsx` and
`message-blocks.tsx`; design rules from `docs/UI.md` and `spec/08-ui-ux.md`. All are CONFIRMED
readings. The persona and its jobs are a GUESS (proposal only — no water sector exists in the spec).
No product code was modified.
