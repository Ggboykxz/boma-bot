# UI proposal 002 — Hospitals (inpatient care coordination)

Sector specialist #002, read-only on product code. Scope: propose changes to the existing Boma Bot
screens for a hospital ward; nothing here is implemented. Charter held: no upstream/OpenBot brand,
code, wording or assets; BoardUI tokens only (no raw palette/hex), `cx()`, dark mode via tokens; one
design system; self-host only; robot-cat identity and product name unchanged. PHI never leaves the
operator's machine and is treated as untrusted data, never as markup.

## 1. Persona and top jobs

**Ward care coordinator** (charge nurse / coordinator on an inpatient unit), self-hosted deployment,
often on a shared ward workstation at 360–1280 px, mid-shift and between handoffs. She uses bots to
draft handoff summaries, chase labs/imaging/pharmacy, and prepare discharge paperwork. Top jobs:

1. Read a patient's current state and what is still open in under ten seconds at a shift boundary.
2. Catch and safely confirm/decline a time-critical bot action (order draft, discharge summary).
3. Triage which automated tasks are stuck, failed or waiting on her, without scrolling chat.
4. Keep an auditable, legible trail of what each bot did for the next shift and for review.
5. Coordinate the care team in one room and get only the alerts that matter (no alarm fatigue).

## 2. Five concrete changes

**A. Handoff banner on the thread — `/app/:botId` (`components/thread/thread-screen.tsx`).**
Pin a collapsible summary above the transcript: named context (bot/patient label), last activity,
open-run count, unread count. Change: expand the existing header into a `<details>` disclosure like
the tool block in `message-blocks.tsx`, with `text-title-3-medium`/`text-body-medium`, surface
`bg-background-secondary-default`, hairline `border-separator-border`, text
`text-text-primary`/`text-text-secondary`, classes joined with `cx()`. Components: `Button`,
BoardUI `badge` (install via MCP if absent — never hand-build a lookalike), `ScrollFade` for the
pinned region. RPCs: `botsGet`, `threadsHead`, `runsList({ botId })`, `threadsMarkRead`. REQ-001,
REQ-009, REQ-011. CONFIRMED these RPCs are already wired in the screen.

**B. Safety-first approval card — `components/thread/message-blocks.tsx` (`CardBlock`).**
An approval/purchase card must state the named effect ("what will happen"), the acting bot and the
target before the decision, using text plus a leading icon from `@remixicon/react` — never colour
alone. Change: extend the current generic `thread.card.approval.explain` copy to a structured,
catalogued effect sentence; keep the card blocking only the effect, not the transcript (spec §8.9);
keep Approve/Decline as `boma-target` buttons with `focus-visible:ring-border-focus-ring`. Components:
`Button`, `Badge`. Tokens: `bg-background-secondary-default`, `border-separator-border`,
`text-text-primary`, `text-text-error-primary`/`border-border-error-default` for the failure row.
RPC: `threadsAnswer` (already present). REQ-007, REQ-017. No new RPC. CONFIRMED.

**C. "Open work" triage on Activity — activity view (`components/activity/activity-screen.tsx`).**
Adapt the single run list into a shift-handoff queue: group by named status (working, waiting,
failed, terminal), sort failing/waiting to the top, add status filter tabs. Change: reuse
`Tabs` (`components/base/tabs`) as the filter and `Badge`/`chip` for text+icon status; keep the
existing pause/resume/retry actions. Components: `Tabs`, `Button`, `Badge`; tokens
`bg-background-primary-default`, `border-border-button-default`, `text-text-secondary`. RPCs:
`runsList`, `runsPause`, `runsResume`, `runsRetry`. REQ-011, REQ-012, REQ-067. CONFIRMED.

**D. Round tracker on the board — `/app/board` (`components/board/board-screen.tsx`).**
Reuse the four columns as a ward task list (assignee bot, comment count, last comment) for chasing
tasks between rounds. Change: migrate the raw `<input>`/`<select>` in the create form to the already
installed BoardUI `input` and `select` components (design rule: extend installed form components,
no raw controls), and keep the modal detail dialog's focus trap and Escape close. Components: `Input`,
`Select`, `Button`; tokens `text-text-primary`, `text-text-secondary`, `border-border-button-default`,
`bg-background-primary-default`, `text-start`. RPCs: `boardsGet`, `ticketsCreate`, `ticketsMove`,
`ticketsComment`. REQ-038. CONFIRMED the raw controls are present today.

**E. Care-team room + alert policy — `/app/g/:groupId` (`components/group/`) and
`components/thread/notifications-panel.tsx`.** One group room per patient/handoff with `@bot`
mentions, plus a per-bot policy panel that turns on only `waiting`/`completed` alerts with a daily
cap. Change: expose the policy with installed `Switch` and `Button` controls and a catalogued
"what reaches the team" sentence; keep it self-hosted (no SaaS destination by default). Components:
`Switch`, `Button`, `Avatar`; tokens `bg-background-secondary-default`, `border-separator-border`,
`text-text-secondary`. RPCs: `groupsCreate`, `groupsGet`, `groupsMessages`, `threadsSend`,
`notificationsPolicyGet`, `notificationsPolicySet`. REQ-010, REQ-057, REQ-064. CONFIRMED RPCs exist.

## 3. Accessibility and i18n / RTL

- WCAG 2.2 AA gate: zero critical/serious axe, keyboard path for every primary flow, 44 px targets
  (`.boma-target`), token focus ring, 360 px width, light and dark, `prefers-reduced-motion`.
- Clinical status (urgent/routine, failed/waiting) is always text + icon, never colour alone; the
  approval effect sentence is announced via the existing polite live region — the transcript itself
  is not a live region (spec §8.9), so streaming tokens are not read aloud mid-round.
- PHI is data, not translation keys: patient identifiers, MRNs, times and order numbers stay in
  `<bdi dir="auto">` / explicit LTR regions and are formatted with `Intl`; artifact and message
  content is never injected as HTML into the document (spec §8.5).
- Every user-visible string goes through `t(locale, key)`; no hardcoded copy, English fallback.
- RTL via `DirectionProvider` and `dir="rtl"` on `<html>`; logical styles `ms/me`, `ps/pe`,
  `text-start`, mirrored rail/drawer motion; numbers, code and URLs LTR (spec §8.9, REQ-057).

## 4. Playwright test names (REQ ids in the name)

```
test("REQ-001 the ward handoff banner shows named context and stays readable while a run streams", ...)
test("REQ-007 a pending clinical approval states its effect, takes focus and does not block other threads", ...)
test("REQ-011 activity sorts failed and waiting runs to the top for shift handoff", ...)
test("REQ-012 every run status renders as text plus icon in light, dark and RTL", ...)
test("REQ-038 the round tracker moves a task and records a comment through the same RPCs", ...)
test("REQ-057 the ward screens mirror for Hebrew with a clean axe run", ...)
test("REQ-064 the care-team policy enables only waiting and completed alerts with a daily cap", ...)
```

## 5. Highest-impact first slice

**Change B — the safety-first approval card.** It is the only proposal that reduces direct patient-
safety risk, and it is the cheapest: `threadsAnswer`, `Button`, `Badge` and the semantic tokens
already exist, so no new RPC, component install or data model is needed. Ship one catalogued
"what will happen" effect sentence, text+icon status, keyboard operability and the polite live
announcement, and pin it with `REQ-007-pending-card` plus the `REQ-007`/`REQ-012` test names above.
It also reuses the M34 check-draw confirmation, so the slice is small and fully verifiable.

Provenance: all claims marked CONFIRMED were read this session from `docs/UI.md`,
`docs/IMPROVEMENTS.md`, `spec/08-ui-ux.md`, `apps/web/lib/api/rpc.ts` and the named components under
`apps/web/components/`; the persona and its jobs are a GUESS (proposal only, no spec sector exists).
