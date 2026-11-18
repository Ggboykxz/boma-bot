# UI proposal 098 — waste management (self-hosted collection and disposal desk)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #098. Read-only on product
code; only this file was written. Grounding read this session: `docs/UI.md`, `spec/08-ui-ux.md`,
`spec/01-requirements.md` (REQ ids), `apps/web/lib/api/rpc.ts` (RPC names) and two components under
`apps/web/components/`: `activity/activity-screen.tsx` and `thread/message-blocks.tsx`. Charter held:
Boma Bot only — no upstream brand, code, wording or assets; BoardUI semantic tokens only, never raw
palette classes or hex literals, classes merged with `cx()`; one design system, no second; no SaaS
(self-hosted on the operator's machine, no route, manifest or customer record leaving the site);
robot-cat identity unchanged.

## 1. Persona and top jobs-to-be-done

**Persona — Amal, fleet and disposal supervisor at a regional waste authority.** She coordinates
collection routes, weighbridge handoffs and licensed-disposal runs across crews that speak Arabic,
French and English, alternating between a depot console and a gloved cab tablet on a poor link.
Boma Bot runs on the authority's own server: route plans, weigh tickets and hazardous-waste manifests
never leave the premises. A load released to the wrong facility, or a manifest approved on a colour
cue alone, is a compliance and safety event — legible state and an explicit "what will happen"
matter more than a fast screen.

Top jobs:
1. **Monitor** — see which collection, transfer and disposal runs are working, paused or failed
   without hunting through a transcript; retry only failures.
2. **Verify before releasing** — confirm a bot-proposed disposal knowing the exact waste stream,
   vehicle, destination facility and manifest id; never approve on colour alone.
3. **Report from the field** — log a missed pickup, contamination or overflow with a photo/note
   without losing the draft to a stray Enter.
4. **Supervise the weighbridge handoff** — take over a legacy scale/routing tool when it needs
   manual entry, and leave an attributable handoff record.
5. **Prove and share** — keep versioned weigh tickets, disposal manifests and recycling reports,
   open each in the thread where it was produced, and publish a regulator or public summary without
   giving every reader a login.

## 2. Five concrete changes

All new copy is catalogued through `t(locale, key)` (`lib/i18n.ts`); nothing is hardcoded in JSX. All reuse BoardUI components vendored in `apps/web/components/` and the token map in `docs/UI.md`; bot values are untrusted text, never HTML.

**A. Route-context strip — `/app/:botId` thread header.** A compact, read-only band above the
transcript with chips for "on route", "at facility" and "exceptions", each with a `Tooltip` carrying
detail (vehicle count, next pickup). Components: `Chip`/`Badge` (`components/base/badges/chip.tsx`),
`Tooltip`, `IconButton` (icons from `@remixicon/react` passed as references). Tokens: surface
`bg-background-secondary-default`, hairline `border-separator-border`, text
`text-text-primary`/`text-text-secondary`, icon `text-foreground-icon-secondary`, type
`text-caption-1-semibold`, joined with `cx()`. Existing RPCs: `botsGet`, `threadsHead`,
`threadsMessages`. Vehicle ids and plates stay LTR in `<bdi>`.

**B. Disposal-approval card — `/app/:botId` pending card (`REQ-007`/`REQ-017`).** Make "what will
happen" dominant: exact waste stream, vehicle, destination facility and manifest id, with an
explicit confirm label ("Release load 4 to North Transfer, manifest M-118", not just "Approve").
Hazardous loads pair `border-border-error-default` + `text-text-error-primary` with a leading icon
and the words "Hazardous" — never colour alone. Components: installed BoardUI `Button`, `Badge`,
`Tooltip`; card surface `bg-background-secondary-default`, `rounded-3xl`, `border-separator-border`,
focus `focus-visible:ring-border-focus-ring`. RPC `threadsAnswer` (exactly-once effect, keeps the
`CONFLICT` guard `message-blocks.tsx` already handles for double answers). Announced once via the
polite live region; the transcript stays readable and other threads reachable (spec §8.9).

**C. Field exception composer — `/app/:botId` composer (`REQ-001`).** Extend the composer with an
"attach photo/note" `IconButton` (`RiFileTextLine`) offering a missed-pickup or contamination note
as a reply-style chip, reusing the existing `ReplyContext` banner and the installed `file-upload`
component. Keep Enter-to-send / Shift+Enter-newline unchanged and keep send disabled while an
approval is pending (`awaitingApproval`). Component: BoardUI `IconButton` + `Tooltip`; focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`, 44 px `.boma-target`, icon
`text-foreground-icon-primary`. RPC `threadsSend`; the attachment enters the same `value` state, so
no new transport. Manifest ids, plates and times stay LTR in `<bdi>`.

**D. Collection-run triage — `/app` Activity view (`activity/activity-screen.tsx`, `REQ-011`/
`REQ-012`/`REQ-067`).** Extend `ActivityScreen` with day grouping and plain captions on the named
canonical chips (`queued` → "collection queued", `running` → "on route", `waiting_takeover` →
"awaiting weighbridge handoff", `failed` → "exception"); the §8.9 vocabulary stays, the caption is
secondary text, never a colour-only chip. Working runs sort to top and update in place; pause/resume
queued runs and retry only `failed`. Components: `Button`, `Chip`; surfaces
`bg-background-primary-default`/`bg-background-secondary-default`, `border-separator-border`, error
row `border-border-error-default` + `text-text-error-primary` + `text-foreground-icon-error`.
Existing RPCs: `runsList`, `runsPause`, `runsResume`, `runsRetry`, `threadsHead`. Scheduled
(background) runs appear here, not in the transcript (spec §8.4).

**E. Route board, record library and shared report — `/app/board` (`REQ-038`), `/app/artifacts`
(`REQ-036`), `/p/:slugToken` (`REQ-037`).** Retitle the four flag-gated columns of `board-screen.tsx`
as reported → routed → collecting → disposed; each card shows load key, title, assignee bot and last
comment, and the detail dialog shows attached runs. Replace any raw `<input>`/`<select>` there with
installed BoardUI `Input`/`Select` to honour the one-system rule. Components: installed `table`/
`chip`, `Button`, `Input`, `Select`, `Badge`; surfaces `bg-background-primary-default`/
`bg-background-secondary-default`, `border-separator-border`, status text + icon. RPCs:
`boardsGet`, `ticketsCreate`, `ticketsMove`, `ticketsComment`. In the library, show weigh tickets,
manifests and recycling reports with a version selector, download and "open in thread" via
`artifactsListSpace`, `artifactsGet`, `artifactsListVersions`, reusing the bounded frame and
untrusted-byte rules of `artifact-viewer.tsx`. With the flag off the board route is absent, not
locked. A compiled regulator/public report can be published through `/p/:slugToken` with no session
or chrome beyond a small robot-cat footer; unknown, revoked and expired tokens show one identical
"not available" screen — same wording and timing — with `noindex`, `no-referrer` and its own CSP.

## 3. Accessibility, i18n and RTL

- All visible strings go through `t(locale, key)` (English source, French/Hebrew translations; a
  missing key falls back to English, never blank or the raw key). No string in JSX.
- `DirectionProvider` takes the explicit BCP 47 locale; the boot script writes `lang`/`dir` together.
  Use logical utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`) and
  `useDirection()` to mirror the strip, board columns and drawer motion in RTL. Load keys, manifest
  ids, plates, dates and numbers stay LTR in `<bdi>`/`dir="auto"`, formatted with `Intl` (weights and
  money in integer minor units, never floats).
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px `.boma-target`
  for gloved cab use; token-driven visible focus; a keyboard path for every decision (release, field
  report, pause/resume/retry, board move, version select); run status and the hazardous marker are
  text + icon, never colour alone (spec §8.9); 360 px width; `prefers-reduced-motion`; no animation on
  streaming content.
- Error copy is one sentence of what happened plus one of what to do — no manifest ids, plates,
  stack traces or provider text on screen (correlation id behind "details" only). The hazardous
  marker is text + icon and the release decision is announced once via the polite live region; the
  transcript is not a live region, so streaming tokens are not read aloud. The robot-cat mark stays
  unchanged.

## 4. Playwright tests (REQ ids in the names)

```
test("REQ-001 the field exception composer sends one optimistic message that is not duplicated", ...)
test("REQ-007 a pending disposal card takes focus and does not block reading or other threads", ...)
test("REQ-009 the exceptions divider moves and a snoozed pickup returns on time", ...)
test("REQ-011 the activity view groups collection runs by day and links to the starting message", ...)
test("REQ-012 every run status renders as text plus icon in light, dark and RTL", ...)
test("REQ-017 a hazardous disposal names stream, vehicle, facility and manifest, answers once", ...)
test("REQ-021 a failed disposal run shows a plain cause and a working retry action", ...)
test("REQ-026 the weighbridge panel names starting, running, takeover and released states", ...)
test("REQ-036 the records library filters weigh tickets and manifests and shows the version chain", ...)
test("REQ-038 the route board moves a load and records a comment through the same RPCs", ...)
test("REQ-037 the shared report renders without chrome; an invalid token shows the uniform screen", ...)
test("REQ-057 the waste screens mirror for Hebrew with dir=rtl and a clean axe run", ...)
```

Behaviour above without a REQ yet (route strip, attach-note chip, hazardous marker, day grouping,
collection captions) must be assigned a REQ id by the owner before implementation per the
traceability gate — do not invent one in a test name.

## 5. Highest-impact first slice

**Change B — the disposal-approval card.** It is the cheapest and the only change that directly
reduces the risk of a load released to the wrong facility through an ambiguous confirmation:
`threadsAnswer`, `Button` and `Badge` are installed, the semantic tokens exist, and no new RPC,
component install, data model or token re-tune is needed (`message-blocks.tsx` already owns the
`CONFLICT` guard). Test-first: write `REQ-017 a hazardous disposal names stream, vehicle, facility
and manifest, answers once` and extend `spec/golden/ui/REQ-007-pending-card.json` first; ship only
the dominant consequence sentence, the explicit confirm label, the manifest attribution, the
text+icon hazardous marker, keyboard operability and the polite announcement. Defer (A), (C), (D),
(E) until the owner assigns REQ ids and approves.

Provenance: REQ ids verified against `spec/01-requirements.md`; RPC names from `apps/web/lib/api/rpc.ts`;
component structure from `activity-screen.tsx` and `message-blocks.tsx`; design rules from `docs/UI.md`
and `spec/08-ui-ux.md`. All are CONFIRMED readings. The persona, jobs and waste mapping are GUESS
(proposal only — no waste sector exists in the spec). No product code was modified.
