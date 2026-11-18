# 022 — Wholesale / distribution vertical (UI proposal)

Status: **PROPOSAL / GUESS** (2026-11-18). No product code was changed; only this file was
written. These are adaptations of *existing* screens to a wholesale workflow, expressed
entirely through BoardUI tokens, installed components and RPCs that already exist. They need
owner approval (STATE "Decisions needed") before any implementation. No upstream brand, code,
asset or wording is used; the only mark is the owner-supplied robot-cat head. No new theme and
no second design system; no hosted-account or credit (SaaS) surface.

## Persona and jobs

**Mara — wholesale operations lead** at a regional distributor. She coordinates purchase
orders, supplier shipments and returns across ~40 SKUs with a small team and a handful of
bots. She works on a laptop at a desk and on a phone on the warehouse floor.

| Job | What she needs from the UI |
| --- | --- |
| Approve a purchase order above a value threshold | one blocking, explained decision, no hunting through chat |
| Chase an order exception / damaged shipment | a board of exceptions she can triage and assign to a bot |
| Talk to a supplier's agent without leaving the product | a room per supplier where the right bot answers |
| Find an invoice or packing list and prove which version was used | a searchable document library with versions |
| See what is stuck, retry a failed sync, pause a noisy run | one activity feed with plain-language status |

These are the product's generic primitives (cards, board, rooms, artifacts, runs) viewed through
a wholesale lens — not new features. Each maps to a shipped screen and RPC.

## The five UI changes

### 1. Purchase-order approval inbox — thread + activity
A high-value PO parks a **modal approval card** (spec §8.3, §8.9): the transcript stays readable,
other threads stay reachable, and one click decides. The card states "what will happen" in one
sentence (e.g. "Approve releases the PO to the supplier robot"). Wholesale value: an approval
trail with an explicit decision instead of a buried reply.
- **Components/tokens:** reuse the `card` block in `message-blocks.tsx`; `Button` primary/secondary
  for Approve/Deny; `badge`/`chip` for the "awaiting approval" state; `text-text-primary/secondary`,
  `bg-background-primary-default`, `border-border-button-default`; `cx()` for merges.
- **RPCs (existing):** `threadsMessages` + SSE `threadsSubscribe` to receive the card;
  `threadsAnswer` to record the decision; `runsList` for the linked run.
- **REQ:** REQ-007, REQ-017.

### 2. Order-exception board — `/app/board`
Reuse the feature-flagged board (REQ-038). Columns `backlog → todo → in_progress → done` become
the exception queue; each ticket carries a key, title, assignee bot and last comment. Creating a
ticket and assigning a bot queues the work; the board shows progress, the chat does not.
- **Components/tokens:** `Button`, `chip` (state), `avatar`/bot name pill; existing `rounded-2xl`
  card styling with `border-border-button-default`, `bg-background-primary-hover` on hover,
  `text-text-tertiary` for the key/count; `RiAddLine`, `RiChat1Line` as component references.
- **RPCs (existing):** `boardsGet`, `ticketsCreate`, `ticketsMove`, `ticketsComment`, `botsList`.
- **REQ:** REQ-038, REQ-012.

### 3. Supplier room — `/app/g/:groupId`
One group room per supplier or carrier. Bots act as buyer/expediter agents; `@Name` mentions
route a request to exactly the mentioned bot, and a bot can hand a task to a sibling. Wholesale
value: one durable, attributed conversation per counterparty instead of scattered DMs.
- **Components/tokens:** `group-thread-screen.tsx` member chips, attributed transcript and
  composer; `Avatar`, `Input`; `text-foreground-icon-primary`, `border-separator-border`.
- **RPCs (existing):** `groupsCreate`, `groupsList`, `groupsGet`, `groupsMessages`, `threadsSend`
  (fan-out), `groupsReassign`, `botsList`.
- **REQ:** REQ-010, REQ-068.

### 4. Invoice and packing-list library — `/app/artifacts`
Reuse the artifact grid/viewer (REQ-036/037). Invoices, packing lists and specs become versioned
artifacts with type filters, name search and "open in thread". Images render in a bounded frame,
PDFs/text natively; anything else downloads. Content stays untrusted — never render artifact HTML
in the app document.
- **Components/tokens:** `artifacts-grid.tsx` / `artifact-viewer.tsx` cards, `select` (filter),
  `input` (search), `Button` (download/open); `bg-background-secondary-default` skeletons,
  `border-border-table`, composite type utilities.
- **RPCs (existing):** `artifactsListSpace`, `artifactsGet`, `artifactsListVersions`,
  `artifactsRemove`.
- **REQ:** REQ-036, REQ-037.

### 5. Fulfilment run feed — Activity view
Reuse the activity screen (REQ-011). Each order sync / exception check is a run with a named
status — `queued`, `leased`, `running` → `waiting_input`, `waiting_takeover` → `completed`,
`failed`, `cancelled` — shown as text+icon chips, never colour alone. Only `failed` offers retry;
queued/paused rows offer pause/resume; working runs sort to the top and update in place.
- **Components/tokens:** `activity-screen.tsx` rows, `Button` (retry/pause/resume), `chip` status;
  `animate-row-slide-in`, `prefers-reduced-motion` guarded; `text-text-error-primary` for error
  rows; `border-border-error-default` hairlines.
- **RPCs (existing):** `runsList`, `runsRetry`, `runsPause`, `runsResume`.
- **REQ:** REQ-011, REQ-012, REQ-067.

## Accessibility (WCAG 2.2 AA)

- Zero critical/serious axe findings in light, dark and RTL; keyboard path for all five flows.
- Status and card state always carry a text label plus an icon — colour is a second signal only.
- Approvals are modal but non-blocking for reading: focus moves into the card, Tab is trapped,
  Escape closes on non-modal cards; a polite `aria-live` region announces status/card changes,
  the transcript itself is never a live region (§8.9).
- 44 px touch targets (`.boma-target`) and `focus-visible:ring-2 focus-visible:ring-border-focus-ring`
  (`.boma-focus`); never a raw palette class or hex value.

## i18n and RTL

- Every user-visible string through `t(locale, key)` (`lib/i18n.ts`); keys prefixed `wholesale.*`
  in the English source catalogue, with fr/he translations or English fallback — never a key or a
  blank shown to the user. Plain, factual, second person, no exclamation marks.
- `DirectionProvider` (BCP 47 locale) drives direction and React Aria; logical utilities
  (`ms`/`me`, `ps`/`pe`, `start`/`end`, `text-start/end`) everywhere; rail on the right and drawer
  motion reversed for `he`.
- Order numbers, SKUs, URLs and email addresses stay LTR, isolated with `<bdi>` / `dir="auto"`.
- Proof: store `he`, load `/app/`, assert `dir="rtl"`, a Hebrew nav label and a clean axe run.

## Playwright names (REQ id in each)

- `REQ-007 a high-value purchase-order card is modal, takes focus and one click decides`
- `REQ-017 answering a purchase-order approval is reflected once in the transcript`
- `REQ-038 an order exception can be created, moved and commented on the board`
- `REQ-010 a supplier room routes an @mention to the named bot`
- `REQ-068 reassigning a delegated exception hands it to a sibling bot`
- `REQ-036 an invoice artifact opens with its version chain and bounded viewer`
- `REQ-037 a shared order page renders without chrome`
- `REQ-011 the fulfilment feed shows newest runs first with named statuses`
- `REQ-012 every run status renders a distinct text+icon chip; only failed offers retry`
- `REQ-067 queued runs offer pause and paused runs offer resume`
- `REQ-057 wholesale strings switch locale and the layout mirrors to RTL`

## Best first slice

**#1 — Purchase-order approval inbox.** It is the smallest, most wholesale-specific change and
it is already half-built: `REQ-007-pending-card` golden, `threadsAnswer`, the modal-card pattern
and the polite live region all exist. One card + one decision + one a11y/RTL test proves the whole
vertical without a new component or RPC, and gives the owner a concrete demo before #2–#5 are
approved. Fields above the value threshold would be a future, separately approved rule, not part
of this slice.
