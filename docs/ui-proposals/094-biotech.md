# UI proposal 094 — biotech (self-hosted research lab bench desk)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #094. Read-only on product code;
this file is the only write. Grounding (CONFIRMED, read this session): `docs/UI.md`,
`spec/08-ui-ux.md`, `spec/01-requirements.md` (REQ ids), `apps/web/lib/api/rpc.ts` (RPC names) and two
components: `apps/web/components/thread/message-blocks.tsx` (`CardBlock` — `threadsAnswer` with its
`CONFLICT` guard, bot text as data not markup) and `apps/web/components/artifacts/artifact-viewer.tsx`
(bounded frame `max-h-[60vh] max-w-full object-contain`, version selector, untrusted-byte walls, raw
`<select>`). No upstream brand, code, wording, asset or capture consulted or copied.

Charter (CONFIRMED): Boma Bot only; no upstream brand/code/assets/copy; fresh design (ADR-015) with the
owner-supplied robot-cat as sole identity; BoardUI semantic tokens only — no raw palette class or hex
literal, composite type utilities, every class list merged with `cx()`; one design system, no second
library or lookalike; self-hosted on the lab's own machine — no SaaS/cloud, no sample, sequence,
protocol or unpublished result leaving the premises.

## 1. Persona and top jobs-to-be-done

**Persona — "the bench lead"**: a lab scientist in a self-hosted biotech lab who designs experiments,
tracks samples and chain-of-custody, reviews bot-proposed protocol steps, and writes up results, in
English, French and Arabic, between the bench workstation and a phone. Samples, sequences, protocols
and draft results never leave the building. A released protocol or sample transfer is attributable:
approving the wrong reagent, sample or revision — or on a colour cue — is a real loss; ambiguity is
worse than latency. Instrument plots and micrographs are user content.

Top jobs:
1. **Scope and collect** — hand a protocol, sample set or assay to a bot and see what is new, unread
   or awaiting a decision, without scrolling the transcript.
2. **Release safely** — review a bot-proposed protocol step or sample transfer knowing the exact
   sample id, revision and destination; never confirm on colour alone.
3. **Name run state** — say out loud which assay, batch and analysis runs are working, paused or
   terminal, using the §8.9 vocabulary; retry only failures, pause/resume queued work.
4. **Trace and version** — keep sequences, result tables and micrographs as an artifact version chain
   and open each version in the run that produced it.
5. **Report safely** — move a result from proposed to reviewed to shared and publish a reviewed data
   page by link with no login and no leak of other experiments.

## 2. Five concrete UI changes

All copy is a new `lib/i18n.ts` key via `useTranslation()` / `t(locale, key)` (English source, fr/he
translations; missing key falls back to English). Colour rides the `docs/UI.md` token map, type uses
composite utilities, every class list goes through `cx()`. Bot-produced values are untrusted text/bytes,
never HTML (per `message-blocks.tsx` and `artifact-viewer.tsx`).

**(A) Experiment-status strip — thread `/app/:botId` header.** A compact read-only band above the
transcript: named chips for "samples pending", "unread results", "open questions", each with a
keyboard-focusable `Tooltip`. BoardUI `Chip`/`Badge` (`components/base/badges/chip.tsx`), `Tooltip`,
`IconButton` (icons from `@remixicon/react` as references). Tokens: `bg-background-secondary-default`,
`border-separator-border`, `text-text-primary`/`text-text-secondary`, `text-foreground-icon-secondary`,
`text-caption-1-semibold`, joined with `cx()`. RPCs (CONFIRMED): `botsGet`, `threadsHead`,
`threadsMessages`, `runsList`. Sample ids and accession numbers stay LTR in `<bdi>`; out of logs.

**(B) Protocol/sample-release approval card — thread pending card (REQ-007/REQ-017).** Extend
`CardBlock` so the "what will happen" sentence dominates: exact sample id, protocol revision and
destination, with a distinct label ("Release sample", not just "Approve"). High-impact actions
(release, dispose, publish) pair `border-border-error-default` + `text-text-error-primary` with a
leading icon and the words "High impact" — never colour alone. BoardUI `Button`, `Badge`, `Tooltip`;
card `bg-background-secondary-default`, `rounded-3xl`, `border-separator-border`, focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`. RPC (CONFIRMED): `threadsAnswer`, keeping
the `CONFLICT` guard `CardBlock` already owns. Transcript stays readable, other threads reachable,
focus lands on the card, the decision is announced once via the polite live region (§8.9); no new
transport, robot-cat mark unchanged.

**(C) Notebook-entry composer — thread `/app/:botId` composer (REQ-001).** Add an "attach sample"
`IconButton` (`RiFlaskLine`) offering the current sample/protocol as a reply-style chip, reusing the
reply-banner pattern. Keep Enter-to-send / Shift+Enter-newline and send-disabled-while-approval-pending.
BoardUI `IconButton` + `Tooltip`; `focus-visible:ring-2 focus-visible:ring-border-focus-ring`, 44 px
`.boma-target`, `text-foreground-icon-primary`. RPC (CONFIRMED): `threadsSend`; the reference enters
the same `value` state, so no new transport. Accession numbers, reagent lots and dates stay LTR.

**(D) Batch-run triage — `/app` Activity view (REQ-011/REQ-012/REQ-067).** Follow `ActivityScreen`:
group assay, batch and analysis runs by day with prompt snippet, bot name, a named status chip,
duration and a link to the starting message. Working runs sort to the top and update in place;
failures get a plain cause and a retry, never a decorative spinner. Installed `table`/`chip`, `Button`,
`IconButton`, `Tooltip`; surfaces `bg-background-primary-default`/`bg-background-secondary-default`,
`border-separator-border`, status text + icon in the exact §8.9 vocabulary. RPCs (CONFIRMED):
`runsList`, `runsRetry`, `runsPause`, `runsResume`, `notificationsStatus`. Background batch runs appear
here, not the transcript; arrivals use `notifications.*` policies (REQ-064).

**(E) Sample/result library and shared report — `/app/artifacts`, `/p/:slugToken`
(REQ-036/REQ-037/REQ-039).** In the library name sequences, result tables and micrographs by sample,
filter by type/bot, search by name, show the version chain and "open in thread" via
`artifactsListSpace`, `artifactsGet`, `artifactsListVersions`, `artifactsRemove`, keeping the bounded
frame and untrusted-byte rules. Prefer installed BoardUI `Select` over the raw `<select>` in
`artifact-viewer.tsx`, merged with `cx()`; `bg-background-primary-default`, `border-separator-border`,
`text-text-primary`/`text-text-secondary`. Reuse `SharedPageScreen` to publish a reviewed data page with
no session and no chrome beyond a small robot-cat footer; unknown, revoked and expired tokens render
one identical "not available" screen — same words, shape and timing. Transport (CONFIRMED):
`GET /api/v1/pages/shared/{token}`; `noindex`/`no-referrer` and its own CSP stay backend concerns.

## 3. Accessibility

WCAG 2.2 AA: zero critical/serious axe findings in light, dark and RTL; a keyboard-only path for every
primary flow (release decision, attach, board moves, pause/resume, retry, version select); visible
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target`; 360 px width;
`prefers-reduced-motion`; no animation on streaming content. Status uses the §8.9 vocabulary and impact
is text + icon — never colour alone; micrographs and plots carry a text alternative, never colour-only
encoding, never autoplay. A pending card takes focus; the transcript is not a live region; the uniform
unavailable screen is announced in identical text — no leak through the accessibility tree. Error copy
is one sentence of what happened plus one of what to do — no sample id, sequence, stack trace or provider
text (correlation id behind "details", §8.9). Tables carry `<caption>` and `<th scope>`.

## 4. i18n and RTL

English is the source catalogue, fr/he translations; a missing key falls back to English, never blank or
the raw key. `DirectionProvider` (`@/components/foundations/direction/direction`) gets an explicit BCP 47
locale; the boot script writes `lang` and `dir` together. RTL mirrors the strip, rail and drawer motion
via `useDirection()`; arrows flip. Logical utilities throughout (`ms`/`me`, `ps`/`pe`,
`text-start`/`text-end`, `border-s`/`border-e`). Sample ids, accession numbers, sequences, reagent lots,
URLs, dates and numbers stay LTR in `<bdi>`/`dir="auto"`, formatted with `Intl`. Preserve physical
coordinates for micrographs and plots — a chart is never mirrored, only its labels move.

## 5. Playwright tests (REQ id in each name)

- `test("REQ-001 the notebook composer sends one optimistic message that is not duplicated", ...)`
- `test("REQ-007 a pending sample-release card takes focus and does not block reading or other threads", ...)`
- `test("REQ-011 the activity view groups batch runs by day and links to the source", ...)`
- `test("REQ-012 every run status renders as a distinct named text-and-icon chip", ...)`
- `test("REQ-017 a release names the sample, revision and destination and answers exactly once", ...)`
- `test("REQ-036 the sample library filters by type and bot and shows the version chain", ...)`
- `test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)`
- `test("REQ-057 Hebrew RTL mirrors the rail and axe reports zero critical or serious findings", ...)`
- `test("REQ-067 pause and resume a queued batch announce their status through the polite live region", ...)`

Behaviour without a REQ yet (status strip, attach-sample chip, high-impact marker) needs an owner-assigned
id before code — do not invent one in a test name. Goldens stay byte-exact; a new state means a new
`REQ-<id>-<slug>.json` asserting seeded state, not a click path.

## 6. Best first slice

**Change (B), the protocol/sample-release approval card** — GUESS, recommended. Smallest, highest-value
step: one screen, no new transport (`threadsAnswer` with its `CONFLICT` guard), no new component
(`Button`/`Badge`/`Tooltip` installed), no token re-tune. It directly prevents releasing the wrong sample
or confirming on a colour cue. Test-first: write
`REQ-017 a release names the sample, revision and destination and answers exactly once...`, extend
`spec/golden/ui/REQ-007-pending-card.json`, then ship the dominant consequence line, the explicit
"Release sample" label, the attribution line and the text+icon high-impact marker. Defer (A), (C), (D),
(E) until the owner assigns any missing REQ ids and approves scope.

Provenance (CONFIRMED this session): REQ ids and goldens from `spec/01-requirements.md` and
`spec/golden/ui/`; RPC names and the `CONFLICT` guard from `rpc.ts`; component structure from
`message-blocks.tsx` and `artifact-viewer.tsx`; design rules from `docs/UI.md` and `spec/08-ui-ux.md`.
The persona, jobs and biotech mapping are GUESS (no such sector or REQ exists). No product code modified.
