# UI proposal 060 — ai-research (self-hosted model-evaluation and experiment desk)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #060 — ai-research. Read-only on
product code; this file is the only write. Grounding (CONFIRMED, read this session): `docs/UI.md`,
`spec/08-ui-ux.md`, `spec/01-requirements.md` (REQ ids), `apps/web/lib/api/rpc.ts` (RPC names) and two
components: `apps/web/components/thread/message-blocks.tsx` (`CardBlock` — `threadsAnswer` with its
`CONFLICT` guard, tool-call disclosure, bot text as data not markup) and
`apps/web/components/artifacts/artifact-viewer.tsx` (bounded frame `max-h-[60vh] max-w-full object-contain`,
version selector, "open in thread", untrusted-byte walls, raw `<select>`). No upstream brand, code,
wording, asset or capture consulted or copied.

Charter (CONFIRMED): Boma Bot only, fresh design (ADR-015), owner-supplied robot-cat as sole identity;
no upstream brand/code/assets/copy; BoardUI semantic tokens only — no raw palette class or hex literal,
composite type utilities, every class list merged with `cx()`; one design system, no second library or
lookalike; self-hosted on the lab's machine — no SaaS/cloud, no dataset, prompt, weight or unpublished
result leaving the premises.

## 1. Persona and top jobs-to-be-done

**Persona — "the evaluation lead"**: an ML researcher at a self-hosted lab who surveys papers, designs
evals, runs benchmarks, compares checkpoints and writes up findings, in English, French and Arabic,
between a GPU workstation and a phone. Datasets, prompts, weights and draft results never leave the
building. A published claim is attributable: approving it against the wrong dataset, checkpoint or
metric — or on a colour cue — is a real loss; ambiguity is worse than latency. Plots are user content.

Top jobs:
1. **Scope and collect** — hand a paper, dataset or benchmark to a bot and see which sources are new,
   which results unread and which questions open, without scrolling the transcript.
2. **Verify a finding** — review a bot-proposed claim, synthesis or publish action knowing the exact
   metric, dataset and model version; never approve on colour alone.
3. **Name run state** — say out loud which watch, eval, benchmark and training runs are working, paused
   or terminal, with the §8.9 vocabulary; retry only failures, pause/resume queued work.
4. **Compare and version** — keep datasets, result tables and plots as an artifact version chain and
   open each version in the thread and run that produced it.
5. **Publish safely** — move an experiment from proposed to evaluated to published and share a reviewed
   result page by link with no login and no leak of other experiments.

## 2. Five concrete UI changes

All copy is a new `lib/i18n.ts` key via `useTranslation()` / `t(locale, key)` (English source, fr/he
translations; missing key falls back to English). Colour rides the `docs/UI.md` token map, type uses
composite utilities, every class list goes through `cx()`. Bot-produced values are untrusted text/bytes,
never HTML (per `artifact-viewer.tsx`).

**(A) Research-status strip — thread `/app/:botId` header.** A compact read-only band above the
transcript: named chips for "new papers", "unread results", "open questions", each with a `Tooltip`.
BoardUI `Chip`/`Badge` (`components/base/badges/chip.tsx`), `Tooltip`, `IconButton` (icons from
`@remixicon/react` as references). Tokens: `bg-background-secondary-default`, `border-separator-border`,
`text-text-primary`/`text-text-secondary`, `text-foreground-icon-secondary`, `text-caption-1-semibold`,
joined with `cx()`; an optional trend uses `chart-1`…`chart-5`. RPCs (CONFIRMED): `botsGet`,
`threadsHead`, `threadsMessages`, `runsList`. Chart geometry stays LTR; ids and URLs stay out of logs.

**(B) Finding/publication approval card — thread pending card (REQ-007/REQ-017).** Extend `CardBlock`
so the "what will happen" sentence dominates: exact claim, metric, dataset and model/checkpoint version,
with a distinct label ("Publish finding", not just "Approve"). High-impact actions (publish, retrain,
overwrite) pair `border-border-error-default` + `text-text-error-primary` with a leading icon and the
words "High impact" — never colour alone. BoardUI `Button`, `Badge`, `Tooltip`; card
`bg-background-secondary-default`, `rounded-3xl`, `border-separator-border`, focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`. RPC (CONFIRMED): `threadsAnswer`, keeping
the `CONFLICT` guard `CardBlock` already owns. Transcript stays readable, other threads reachable,
focus lands on the card, decision announced once via the polite live region (§8.9); no new transport.

**(C) Cite/paper composer — thread `/app/:botId` composer (REQ-001).** Add an "attach source"
`IconButton` (`RiFileTextLine`) offering the current paper/dataset as a reply-style chip, reusing the
reply-banner pattern. Keep Enter-to-send / Shift+Enter-newline and send-disabled-while-approval-pending.
BoardUI `IconButton` + `Tooltip`; `focus-visible:ring-2 focus-visible:ring-border-focus-ring`, 44 px
`.boma-target`, `text-foreground-icon-primary`. RPC (CONFIRMED): `threadsSend`; the reference enters the
same `value` state, so no new transport. DOIs, arXiv ids, URLs and dates stay LTR in `<bdi>`.

**(D) Experiment-run triage — `/app` Activity view (REQ-011/REQ-012/REQ-067).** Follow `ActivityScreen`:
group watch, eval, benchmark and training runs by day with prompt snippet, bot name, a named status
chip, duration and a link to the starting message. Working runs sort to the top and update in place;
failures get a plain cause and a retry, never a decorative spinner. Installed `table`/`chip`, `Button`,
`IconButton`, `Tooltip`; surfaces `bg-background-primary-default`/`bg-background-secondary-default`,
`border-separator-border`, status text + icon in the exact §8.9 vocabulary. RPCs (CONFIRMED):
`runsList`, `runsRetry`, `runsPause`, `runsResume`, `notificationsStatus`. Background eval runs appear
here, not the transcript; arrivals use `notifications.*` policies (REQ-064).

**(E) Experiment registry and result library — `/app/board`, `/app/artifacts`, `/p/:slugToken`
(REQ-038/REQ-036/REQ-037/REQ-039).** Retitle the four flag-gated board columns proposed → evaluating →
analyzed → published, each card with key, title, assignee bot and last comment; a detail panel shows
attached eval runs. Prefer installed BoardUI `Input`/`Select`/`Chip` over raw fields (notably the raw
`<select>` in `artifact-viewer.tsx`), merged with `cx()`; `bg-background-primary-default`,
`border-separator-border`, `text-text-primary`/`text-text-secondary`. RPCs (CONFIRMED): `boardsGet`,
`botsList`, `ticketsCreate`, `ticketsMove`, `ticketsComment`. In the library name datasets, result tables
and plots by experiment, filter by type/bot, search by name, show the version chain and "open in thread"
via `artifactsListSpace`, `artifactsGet`, `artifactsListVersions`, `artifactsRemove`, keeping the bounded
frame and untrusted-byte rules. A 404 keeps the board route absent, not locked (§8.6). Reuse
`SharedPageScreen` to publish a reviewed result page with no session and no chrome beyond a small
robot-cat footer; unknown, revoked and expired tokens render one identical "not available" screen —
same words, shape and timing. Transport (CONFIRMED): `GET /api/v1/pages/shared/{token}`; `noindex`/
`no-referrer` and its own CSP stay backend concerns.

## 3. Accessibility

WCAG 2.2 AA: zero critical/serious axe findings in light, dark and RTL; a keyboard-only path for every
primary flow (finding decision, attach, board moves, pause/resume, retry, version select); visible
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target`; 360 px width;
`prefers-reduced-motion`; no animation on streaming content. Status uses the §8.9 vocabulary and impact
is text + icon — never colour alone; plots carry a text alternative, never colour-only encoding, never
autoplay. A pending card takes focus; the uniform unavailable screen is announced in identical text — no
leak through the accessibility tree. Error copy is one sentence of what happened plus one of what to do —
no metric, dataset, checkpoint hash, stack trace or provider text (correlation id behind "details", §8.9).
Tables carry `<caption>` and `<th scope>`.

## 4. i18n and RTL

English is the source catalogue, fr/he translations; a missing key falls back to English, never blank or
the raw key. `DirectionProvider` (`@/components/foundations/direction/direction`) gets an explicit BCP 47
locale; the boot script writes `lang` and `dir` together. RTL mirrors the strip, board columns, rail and
drawer motion via `useDirection()`; arrows flip. Logical utilities throughout (`ms`/`me`, `ps`/`pe`,
`text-start`/`text-end`, `border-s`/`border-e`). DOIs, arXiv ids, URLs, metric symbols, versions, dates
and numbers stay LTR in `<bdi>`/`dir="auto"`, formatted with `Intl` (no floats for counts). Preserve
physical coordinates for plots — a chart is never mirrored, only its labels move.

## 5. Playwright tests (REQ id in each name)

- `test("REQ-001 the cite composer sends one optimistic message that is not duplicated", ...)`
- `test("REQ-007 a pending finding card takes focus and does not block reading or other threads", ...)`
- `test("REQ-011 the activity view groups eval runs by day and links to the source", ...)`
- `test("REQ-012 every run status renders as a distinct named text-and-icon chip", ...)`
- `test("REQ-017 a finding names the metric, dataset and model version and answers exactly once", ...)`
- `test("REQ-036 the result library filters by type and bot and shows the version chain", ...)`
- `test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)`
- `test("REQ-038 the experiment board shows text-and-icon columns and moves a card by keyboard", ...)`
- `test("REQ-057 Hebrew RTL mirrors the board and axe reports zero critical or serious findings", ...)`
- `test("REQ-067 pause and resume a queued eval announce their status through the polite live region", ...)`

Behaviour without a REQ yet (status strip, attach-source chip, high-impact marker) needs an owner-assigned
id before code — do not invent one in a test name. Goldens stay byte-exact; a new state means a new
`REQ-<id>-<slug>.json` asserting seeded state, not a click path.

## 6. Best first slice

**Change (B), the finding/publication approval card** — GUESS, recommended. Smallest, highest-value step:
one screen, no new transport (`threadsAnswer` with its `CONFLICT` guard), no new component
(`Button`/`Badge`/`Tooltip` installed), no token re-tune. It directly prevents publishing a claim against
the wrong dataset or checkpoint, or confirming on a colour cue. Test-first: write
`REQ-017 a finding names the metric, dataset and model version and answers exactly once...`, extend
`spec/golden/ui/REQ-007-pending-card.json`, then ship the dominant consequence line, the explicit
"Publish finding" label, the attribution line and the text+icon high-impact marker. Defer (A), (C), (D),
(E) until the owner assigns any missing REQ ids and approves scope.

**Decision needed from the owner**: confirm the AI-research framing and whether REQ-038's board and the
shared-result path (REQ-039) are in scope for this deployment before change (E) ships.

Provenance (CONFIRMED this session): REQ ids and goldens from `spec/01-requirements.md` and `spec/golden/ui/`; RPC names and the `CONFLICT` guard from `rpc.ts`; component structure from `message-blocks.tsx` and `artifact-viewer.tsx`; design rules from `docs/UI.md` and `spec/08-ui-ux.md`. The persona, jobs and AI-research mapping are GUESS (no such sector or REQ exists). No product code was modified.
