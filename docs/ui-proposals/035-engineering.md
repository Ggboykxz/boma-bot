# UI proposal 035 — engineering (software delivery)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #035. Read-only on product
code; this file is the only write.
Grounding: `docs/UI.md`, `spec/08-ui-ux.md`, `apps/web/components/base/badges/chip.tsx` and
`apps/web/components/thread/thread-composer.tsx` (the two components read in full), plus
`apps/web/lib/api/rpc.ts` for the exact client names.
Charter: Boma Bot only — no upstream/OpenBot brand, code, assets or wording; BoardUI semantic
tokens only (no raw palette classes or hex literals), classes merged with `cx()`; one design
system; self-hosted, no SaaS surface; keep the owner-supplied robot-cat identity everywhere.

## 1. Persona and top jobs-to-be-done

**Persona — Sam, a backend engineer on a small self-hosted team.** Sam delegates bounded tasks to
bots (fix a red CI job, review a diff, bump a dependency) and judges trust by a named state, not by
a colourful dashboard. Logs, stack traces and code cross the composer all day; a wrong approval on
a merge or deploy is worse than a slow answer. The deployment stays on the team's own machine; no
hosted account, credits or third-party service is involved.

Top jobs:
1. **Delegate and watch** — hand a task to a bot and see the current phase without tailing logs.
2. **Triage failures** — find the failed run, read a plain-language cause, retry or reassign it.
3. **Approve safely** — know exactly what a merge/deploy/dependency effect will do before approving.
4. **Paste evidence** — drop logs, stack traces and code into the composer without losing the draft.
5. **Hand off** — move a stuck run to another bot or teammate with an attributable, keyboard-only
   record that survives a reload.

## 2. Five concrete changes

All strings are new keys in `lib/i18n.ts` through `useTranslation()`/`t(locale, key)`; no hardcoded
JSX copy. Every colour rides a BoardUI token; class lists are merged with `cx()`.

**(a) Build-phase status strip — screen `/app/:botId` (thread header, REQ-012).**
Place a named run-state chip and a short phase label beside the model badge in the header. Reuse
BoardUI `Chip` (`components/base/badges/chip.tsx` — the `CHIP_COLORS` status pairs are semantic
tokens), `Tooltip` and `IconButton`. Tokens: `bg-background-secondary-default`,
`border-separator-border`, `text-text-secondary`, `text-foreground-icon-secondary`; type
`text-caption-1-semibold`. The vocabulary is exactly spec §8.9 (`queued`, `leased`, `running`,
`waiting_input`, `waiting_takeover`, `completed`, `failed`, `cancelled`) — text plus icon, never
colour alone. Existing RPCs: `botsGet`, `threadsHead`, `runsList`; live phase comes from the
`threads.subscribe` SSE frames already written into the query cache, no new transport.

**(b) Failure-first triage row — screen `/app` Activity (REQ-011, REQ-021, REQ-012).**
Sort working and failed runs to the top; each failed row carries a one-sentence cause, a named
status chip and a Retry action, with status/bot filters. BoardUI `Chip`, `Button`, `IconButton`,
`Tooltip`; surface `bg-background-primary-default` with `border-border-button-default` and
`rounded-3xl`, cause in `text-text-secondary`, failure marker `text-text-error-primary` +
`border-border-error-default` **plus** the word "Failed" and an icon. Error copy shows no id,
stack trace or provider text (correlation id behind "details", §8.9). Existing RPCs: `runsList`,
`runsRetry`, `runsPause`, `runsResume`, `botsList`.

**(c) High-impact approval card — screen `/app/:botId` pending card (REQ-007, REQ-017).**
Make the "what will happen" sentence the dominant line of a merge/deploy/dependency card and give
the confirm button an explicit verb ("Approve deploy to staging"). High-impact cards pair text,
icon and `border-border-error-default`/`text-text-error-primary` — never colour alone. BoardUI
`Button`, `Chip`, `Tooltip`; card surface `bg-background-primary-default`, `border-border-button-default`,
`rounded-3xl`. Existing RPCs: `threadsAnswer`, `threadsReact`; keep idempotency on the card answer.
Announce once through the polite live region (§8.9); the transcript is not a live region. The card
takes focus without blocking other threads; the composer stays disabled while it is unanswered, as
`thread-composer.tsx` already does.

**(d) Evidence-aware composer — screen `/app/:botId` composer (REQ-001, REQ-061).**
Extend `ThreadComposer` with a small "attach last error" `IconButton` (`RiClipboardLine`) that
inserts a redacted snippet into the draft, and a one-line monospaced preview. Keep Enter-to-send,
Shift+Enter-newline and the disabled-while-approval rule exactly as in `thread-composer.tsx`.
BoardUI `IconButton`/`Tooltip`/`Chip`; tokens `text-text-primary`, `placeholder:text-text-placeholder`,
`bg-background-primary-default`, `text-foreground-icon-primary`, focus `focus-visible:ring-2
focus-visible:ring-border-focus-ring`, 44 px `.boma-target`. Existing RPC: `threadsSend` with
`clientNonce`, so a resend never duplicates a row. Code and stack traces stay text in `<bdi
dir="auto">` inside the React Aria `TextArea` — never rendered HTML. The attach chip is new
behaviour; the owner must assign it a REQ id before implementation.

**(e) Handoff controls in the group room — screen `/app/g/:groupId` (REQ-010, REQ-067, REQ-068).**
Show a named "Working now" banner with the current bot, a status chip, and Pause / Resume / Reassign
actions (BoardUI `Button`, `Select`, `Chip`, `Avatar`). Tokens: `bg-background-secondary-default`,
`border-separator-border`, `text-text-secondary`, `bg-background-secondary-hover`, semantic status
pairs. Existing RPCs: `groupsGet`, `groupsMessages`, `groupsReassign`, `runsPause`, `runsResume`.
Every message keeps its bot attribution so the handover stays accountable; the control is reachable
by keyboard alone and its result is announced politely.

## 3. Accessibility, i18n and RTL

- New labels live in `lib/i18n.ts` (English source, fr/he translations, missing key falls back to
  English, never a blank or the key). No string in JSX.
- `DirectionProvider` drives locale/direction; the boot script writes `lang`/`dir`. Use logical
  utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`); mirror the
  reassign/back arrow via `useDirection()`. Keep commit hashes, URLs, code and stack traces LTR in
  `<bdi>`/`dir="auto"` — translating labels is separate from layout direction.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets via
  `.boma-target`; token-driven visible focus; a full keyboard path for triage, approval and handoff;
  status by text + icon, not colour; 360 px width; `prefers-reduced-motion` honoured (120–180 ms,
  no animation on streaming content).
- Latency honesty: skeletons while a run list loads, never a blank panel; no decorative spinner that
  outlives its work (§8.1).

## 4. Playwright tests (names carry REQ ids)

- `REQ-012 every run phase renders a distinct named chip with text and icon`
- `REQ-011 activity view sorts failures first and filters by status and bot`
- `REQ-021 a failed run shows a one-sentence cause and a retry action with no stack trace`
- `REQ-007 pending deploy approval names the exact effect, takes focus, and does not block other threads`
- `REQ-001 a pasted log sends once with clientNonce and keeps code LTR`
- `REQ-010 group room attributes each message and reassigns by keyboard alone`
- `REQ-067 pause and resume announce their status through the polite live region`
- `REQ-057 Hebrew RTL loads dir=rtl with a clean axe run`
- New behaviour without a REQ (the attach-last-error chip) gets an owner-assigned REQ id before code;
  do not invent one in a test name.

## 5. Highest-impact first slice

**Change (c), the high-impact approval card.** It is the smallest safe step: one screen, no new
transport (`threadsAnswer` exists), no new component (BoardUI `Button`/`Chip`/`Tooltip` are
installed), and it directly removes an ambiguous, high-consequence decision. Test first: write
`REQ-007 ... names the exact effect` and extend `spec/golden/ui/REQ-007-pending-card.json`, then
ship only the dominant consequence sentence, the explicit confirm label and the text+icon
high-impact marker. Defer (a), (b), (d), (e) — and the attach chip's REQ id — to owner approval.
