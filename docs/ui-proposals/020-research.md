# UI proposal 020 — research (self-hosted evidence and synthesis desk)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #020. Read-only on product code;
only this file is written. Grounding read this session: `docs/UI.md`, `spec/08-ui-ux.md`,
`spec/01-requirements.md` (REQ ids), `apps/web/lib/api/rpc.ts` (RPC names) and two components under
`apps/web/components/`: `artifacts/artifact-viewer.tsx` and `thread/message-blocks.tsx`
(`CardBlock`/`MessageBlocks`). Charter held: Boma Bot only — no upstream brand, code, wording or
assets; BoardUI semantic tokens only, no raw palette classes or hex literals, classes merged with
`cx()`; one design system, no second; no SaaS (runs on the institute's own machine, no source text
or notes leaving the building); robot-cat identity and product name unchanged.

## 1. Persona and top jobs-to-be-done

**Persona — Dr. Layla, a systematic-review researcher at a self-hosted research institute.** She
scopes questions, screens sources, verifies bot-proposed findings and writes syntheses, often on a
shared workstation, serving English, French and Arabic colleagues. Boma Bot runs on the institute's
own machine: no cloud, no manuscript or source text leaving the building. A published claim is
attributable, so "what will happen" must be explicit — an ambiguous confirmation is worse than a
slow screen.

Top jobs:
1. **Scope and collect** — attach the current source to a prompt without losing the draft to a stray
   Enter, and keep DOIs/URLs readable.
2. **Triage** — see which sources are new, which results are unread and which questions are open,
   without scrolling a transcript.
3. **Verify** — review a bot-proposed finding (a citation, a synthesis, a publish action) and be
   certain of the source, the citation and the effect before approving; never approve by colour alone.
4. **Monitor** — track scheduled literature-watch runs, know when one stalls or fails, and retry or
   pause it in place with a link back to the originating message.
5. **Archive and share** — keep a versioned, searchable source library, open each version in the
   thread that produced it, and move screening work through a pipeline the chat does not show.

## 2. Five concrete changes

All new copy is catalogued through `t(locale, key)` (`lib/i18n.ts`); nothing is hardcoded in JSX.
All reuse BoardUI components already vendored in `apps/web/components/` and the token map in
`docs/UI.md`. Bot-produced values are untrusted text, never HTML (per `artifact-viewer.tsx`).

**A. Evidence-triage context strip — `/app/:botId` thread header.**
A compact, read-only band above the transcript with chips for "new sources", "unread results" and
"open questions", each with a `Tooltip` for detail. Components: `Badge`/`Chip`
(`components/base/badges/chip.tsx`), `Tooltip`, `IconButton` (icons from `@remixicon/react` passed as
references). Tokens: surface `bg-background-secondary-default`, hairline `border-separator-border`,
text `text-text-primary`/`text-text-secondary`, icon `text-foreground-icon-secondary`, type
`text-caption-1-semibold`, joined with `cx()`. Existing RPCs: `botsGet`, `threadsHead`, `threadsMessages`.

**B. Finding-approval card — `/app/:botId` pending card (`components/thread/message-blocks.tsx`,
REQ-007/REQ-017).**
Make the "what will happen" sentence the dominant line: exact citation, source and effect, plus a
distinct confirm label ("Publish finding", not just "Approve"). High-impact changes use
`border-border-error-default` + `text-text-error-primary` **plus** a leading icon and the words
"High impact" — never colour alone. Components: BoardUI `Button`, `Badge`, `Tooltip`; card surface
`bg-background-secondary-default`, `rounded-3xl`, `border-separator-border`, focus
`focus-visible:ring-border-focus-ring`. RPC: `threadsAnswer` (already present, exactly-once effect,
with the `CONFLICT` guard that `CardBlock` already handles for double answers). Announced once
through the polite live region; the transcript stays readable and other threads reachable (spec §8.9).
No new transport.

**C. Cite/source composer — `/app/:botId` composer (`components/thread/thread-composer.tsx`, REQ-001).**
Extend the composer with a named "attach source" `IconButton` (`RiFileTextLine`) offering the current
artifact as a reply-style chip, reusing the existing reply-banner pattern. Keep Enter-to-send /
Shift+Enter-newline exactly as today, and keep send disabled while an approval is pending. Component:
BoardUI `IconButton` + `Tooltip`; focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`,
44 px `.boma-target`, icon `text-foreground-icon-primary`. RPC `threadsSend`; the attached reference
enters the same `value` state, so no new transport. DOIs, URLs and dates stay LTR in `<bdi>`.

**D. Literature-watch status — `/app` Activity view (REQ-011/REQ-012/REQ-064).**
In the run-centric Activity list, group scheduled watch runs by day with prompt snippet, bot name, a
named status chip, duration and a link back to the starting message. Working runs sort to the top and
update in place; failures get a plain cause and a retry, never a decorative spinner. Components:
installed `table`/`chip`, `Button`, `IconButton`, `Tooltip`; surfaces `bg-background-primary-default`
and `bg-background-secondary-default`, hairline `border-separator-border`, status as text + icon.
RPCs: `runsList`, `runsRetry`, `runsPause`, `runsResume`, `notificationsStatus`, `notificationsSubscribe`,
`threadsHead`. Background (screening) runs appear here, not in the transcript.

**E. Screening pipeline and source library — `/app/board` (REQ-038) and `/app/artifacts` (REQ-036).**
Retitle the four flag-gated board columns as to-screen → screening → included → published, each card
showing source key, title, assigned bot and last comment; a named detail panel shows the attached
watch runs. Components: installed `table`/`chip`, `Button`, `input`, `select`, `Badge`; surfaces
`bg-background-primary-default`/`bg-background-secondary-default`, `border-separator-border`, status
text + icon. RPCs: `boardsGet`, `ticketsCreate`, `ticketsMove`, `ticketsComment`. In the library/viewer,
show sources with version selector, download and "open in thread" via `artifactsListSpace`,
`artifactsGet`, `artifactsListVersions` (and `artifactsRemove`), reusing the bounded frame and
untrusted-byte rules in `artifact-viewer.tsx`. When the flag is off the board route is absent, not locked.

## 3. Accessibility, i18n and RTL

- All visible strings go through `t(locale, key)` (English source; French/Hebrew translations; a
  missing key falls back to English, never a blank or the key). No string in JSX.
- `DirectionProvider` sets locale/direction; the boot script writes `lang`/`dir` together. Use logical
  utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Mirror the attach
  and pipeline arrows via `useDirection()`; keep DOIs, URLs, dates and numbers LTR in `<bdi>`/`dir="auto"`,
  formatted with `Intl` (no floats for any counts).
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets via
  `.boma-target`; token-driven visible focus; full keyboard path (finding decision, attach, retry/pause,
  board move, version select); run status is text + icon, never colour alone (spec §8.9 vocabulary);
  360 px width; `prefers-reduced-motion`; no animation on streaming content.
- The high-impact marker is text + icon, and the finding decision is announced once via the polite
  live region — the transcript is not a live region, so streaming tokens are not read aloud. Error copy
  is one sentence of what happened plus one of what to do; no source ids, stack traces or provider text
  on screen (correlation id behind "details" only). The robot-cat mark stays unchanged.

## 4. Playwright tests (REQ ids in the name)

```
test("REQ-001 the cite composer sends one optimistic message that is not duplicated", ...)
test("REQ-007 a pending finding card takes focus and does not block reading or other threads", ...)
test("REQ-011 the activity view lists working watch runs and links back to the starting message", ...)
test("REQ-012 every run status renders as text plus icon in light, dark and RTL", ...)
test("REQ-017 a finding names the source, citation and effect and answers exactly once", ...)
test("REQ-021 a failed watch run shows a plain cause and a working retry action", ...)
test("REQ-036 the source viewer selects a version and opens it in the originating thread", ...)
test("REQ-038 the screening pipeline moves a ticket and records a comment through the same RPCs", ...)
test("REQ-057 the research screens mirror for Hebrew with a clean axe run", ...)
```
Behaviour above without a REQ yet (evidence-triage strip, attach-source chip, high-impact marker)
must be assigned a REQ id by the owner before implementation per the traceability gate — do not invent
one in test names.

## 5. Highest-impact first slice

**Change B — the finding-approval card.** It is the cheapest and the only change that directly reduces
the risk of an unattributed or ambiguous published claim: `threadsAnswer`, `Button` and `Badge` are
installed, the semantic tokens exist, and no new RPC, component install or data model is needed
(`CardBlock` already owns the `CONFLICT` guard). Test-first: write
`REQ-017 a finding names the source, citation and effect and answers exactly once` and extend
`spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant effect sentence, the explicit
confirm label, the attribution line, the text+icon high-impact marker, keyboard operability and the
polite announcement. Defer (A), (C), (D), (E) until the owner assigns REQ ids and approves.

Provenance: REQ ids verified against `spec/01-requirements.md`; RPC names read this session from
`apps/web/lib/api/rpc.ts`; component structure read from `artifact-viewer.tsx` and
`message-blocks.tsx`; design rules from `docs/UI.md` and `spec/08-ui-ux.md`. All are CONFIRMED
readings. The persona and its jobs are a GUESS (proposal only — no research sector exists in the
spec). No product code was modified.
