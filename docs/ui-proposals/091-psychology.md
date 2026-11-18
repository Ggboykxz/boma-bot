# UI proposal 091 — psychology (evidence-based practice)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #091. Read-only on product code.
Grounding: `docs/UI.md`, `spec/08-ui-ux.md` (§8.3, §8.4, §8.5, §8.9), and two components under
`apps/web/components/`: `thread/message-blocks.tsx`, `thread/thread-composer.tsx`. RPC names
cross-checked against `apps/web/lib/api/rpc.ts` (CONFIRMED exports).
Charter: Boma Bot only; no upstream brand, code, assets or wording; BoardUI tokens only, no raw
palette/hex, `cx()` to merge; one design system; self-hosted, no SaaS; keep the robot-cat identity.

## 1. Persona and top jobs-to-be-done

**Persona — Dr. Nora, a cognitive-behavioural psychologist in a small self-hosted clinic.** She
runs Boma Bot on the clinic's own machine: one bot for structured patient work, one private bot for
her own case notes. Patients answer in English, French, Arabic and Hebrew, usually on a phone.
Nora is a clinician, not a technologist: a wrong score, a diagnosis the bot invents, or a leaked
note is a harm, not a nuisance. Nothing patient-related leaves the clinic or reaches a log.

Top jobs:
1. **Administer a scale** — collect a standardized item (e.g. a 0–3 severity rating) and show a
   deterministic total, never a generative "diagnosis".
2. **Capture a thought record** — guide situation → automatic thought → emotion → evidence →
   alternative thought without a long form or a lost draft.
3. **Frame the boundary** — make plain the tool is a screening and reflection aid, not a clinician.
4. **Track progress** — see scored administrations over time, with a path back to the source message.
5. **Keep case notes private** — control what is stored, exported or shown on a shared device.

## 2. Five concrete changes

All strings are new keys in `lib/i18n.ts` via `t(locale, key)` / `useTranslation()` — none in JSX.
All colours ride the semantic tokens in `docs/UI.md`; classes merge with `cx()` from `@/utils/cx`.

**(a) Standardized-scale item card — screen `/app/:botId`, pending card (REQ-007 pattern).**
When the bot emits a question card, reuse `CardBlock` in `message-blocks.tsx`: one item per card,
a 0–3 Likert row of BoardUI `Button`s (or `Select` for 360 px), a named "Record answer" action and a
read-only "what this score means" sentence. Scoring is **local deterministic logic**, not bot text;
a total is shown only after every item is answered and is labelled "not a diagnosis". Urgency uses
`border-border-error-default` + `text-text-error-primary` **plus** icon (`RiAlertLine`) and the
word "Urgent" — never colour alone. It scrolls into view and takes focus but does not block reading
or other threads (§8.3). Surface `bg-background-secondary-default`, `rounded-3xl`,
`border-separator-border`, `text-body-medium`. RPCs: `threadsAnswer`, `threadsSubscribe`,
`threadsMessages`. Scoring itself has no REQ yet — assign one before implementation.

**(b) Thought-record scaffold — screen `/app/:botId`, composer (REQ-001 pattern).**
Extend `ThreadComposer` with an `IconButton` + `Tooltip` that inserts a catalogue-authored
"situation / thought / feeling / evidence / alternative" scaffold into the draft via `onChange`;
the person edits it and sends through the existing `onSend` path — no new transport. Keep
Enter-to-send / Shift+Enter-newline and the `awaitingApproval` gate exactly as today. Icon
`text-foreground-icon-primary`, focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`,
44 px `.boma-target`, surface `bg-background-primary-default`, `border-border-button-default`.
RPC: `threadsSend`. The button has a visible text label and an `aria-label` from the catalogue.

**(c) Scope and consent strip — screen `/app/:botId`, thread header (new REQ).**
A read-only band above the transcript: chips "Screening aid — not a diagnosis", "Locally
configured crisis resources" and "Session goal", each with a `Tooltip` and one plain sentence.
BoardUI `Badge`/`Chip`, `Tooltip`, `IconButton`; `bg-background-secondary-default`,
`border-separator-border`, `text-text-secondary`, type `text-caption-1-semibold`. RPCs: `botsGet`,
`threadsHead`. Resources are local copy, never a client-side call to a third party. Bot-produced
content is untrusted — render as text, never HTML; the robot-cat mark is the only identity art.

**(d) Progress-over-time table — screen `/app/:botId` right panel, or `/app/artifacts` (§8.5).**
A BoardUI `Table` of administrations: date, scale name, total, status chip, "open in thread".
Tokens `border-border-table`, `text-text-primary`/`text-text-secondary`; use `chart-1…chart-5`
**only** on a BoardUI chart primitive, installed through the MCP if absent — never a hand-drawn
SVG or a second system. RPCs: `artifactsListSpace`, `artifactsGet`, `artifactsListVersions`
(score sheets as artifacts), plus `threadsHead` for the link back. Content is untrusted: show
numbers and text, never artifact HTML; images stay in a bounded frame. Needs a new REQ.

**(e) Session timeline — Activity view reached from the rail (REQ-011, REQ-012).**
Reuse `ActivityScreen`: one row per session run, newest first, grouped by day, status chip via
`runStatusKey`, duration and a link to the message that started it; filter by bot and status, with
working runs sorted to the top and updated in place. Chips use text + icon, never colour alone
(§8.9). RPCs: `runsList`, `runsPause`, `runsResume`, `runsRetry`, `threadsMessages`. Note the
filter controls are a gap in the current screen, not something to fake with the existing row list.

## 3. Accessibility, i18n and RTL

- Every string in `lib/i18n.ts` (English source; French/Arabic/Hebrew translations; a missing key
  falls back to English, never a blank or the raw key). No string in JSX.
- `DirectionProvider` + boot script set `lang`/`dir` together; logical utilities (`ms`/`me`,
  `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Mirror the Likert row and the
  disclosure arrow via `useDirection()`; keep item numbers, scores, dates, hotline numbers, URLs
  and emails LTR in `<bdi>`/`dir="auto"`.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px `.boma-target`;
  token-driven visible focus; full keyboard path (card answers, scaffold button, scope chips,
  table, timeline); status by text + icon, not colour; 360 px width; `prefers-reduced-motion`
  honoured; no animation on streaming content (§8.9).
- Announce a card arrival, a computed score and a failed submission once through the polite live
  region; the transcript itself is not a live region (§8.9).
- Privacy and honesty: error copy is one sentence of what happened plus one of what to do — no
  names, ids, stack traces or provider text on screen (correlation id behind "details" only).
  No patient content in logs; no diagnosis claim; scores are deterministic, not model output.

## 4. Playwright tests (REQ ids where they exist)

- `REQ-007 pending scale item card names the item, takes focus, and does not block reading`
- `REQ-001 thought-record scaffold inserts one message that is replaced, not duplicated`
- `REQ-011 session timeline groups runs by day with a link back to the source message`
- `REQ-012 every run status renders a distinct named chip with the right terminal action`
- `REQ-057 Hebrew RTL loads dir=rtl with a clean axe run`
- Scale scoring, the scaffold button, the scope strip and the progress table have no REQ yet: the
  owner must assign ids before implementation; do not invent them in test names (traceability gate).

## 5. Highest-impact first slice

**Change (a), the standardized-scale item card.** One screen, no new transport
(`threadsAnswer`/`threadsSubscribe` already exist), no new component (`CardBlock`, `Button`,
`Badge` are installed), and it directly reduces the worst harm in this sector — an invented
diagnosis or an ambiguous score. Test-first: write
`REQ-007 pending scale item card names the item, takes focus, and does not block reading` and
extend `spec/golden/ui/REQ-007-pending-card.json` first; ship only the named item, the explicit
"Record answer" label, the deterministic local total and the text+icon marker. Defer (b)–(e)
until the owner assigns REQ ids, confirms the locally configured crisis resources and approves.
