# UI proposal 021 — retail e-commerce (store operations)

Status: PROPOSAL. Sector framing, persona and screen treatments are GUESS (a sector lens, not in
`spec/`); file, component, token and RPC references are CONFIRMED (read this session). No product
code was touched; this is the only file written.

Sources (CONFIRMED): `docs/UI.md`, `spec/08-ui-ux.md`,
`apps/web/components/thread/thread-composer.tsx`, `apps/web/components/shell/bot-rail.tsx`,
`apps/web/lib/api/rpc.ts` (procedure names). Charter (CONFIRMED, `docs/UI.md` + AGENTS.md): fresh
design (ADR-015), robot-cat identity (`assets/brand/boma-bot-head.svg`), BoardUI semantic tokens
only with `cx()` (no raw palette/hex), one design system, self-host only (no SaaS), and no upstream
brand/code/assets/wording.

## 1. Persona and jobs

**Persona — "the store lead"**: the owner-operator of a single-brand online shop who runs a fleet of
specialist bots (merchandising, order exceptions, restock, customer care) from one self-hosted
console. Desktop-first at a back-office desk, tablet on the shop floor; orders, refunds and customer
data are sensitive and the trail matters. The robot-cat mark is the only identity: it sits beside the
wordmark on sign-in and in empty states — never a marketplace or payment-brand logo, never borrowed
copy.

Top jobs:
1. **Merchandise** — write and refresh product copy and keep the catalogue consistent across locales
   without hunting through the rail.
2. **Fulfil** — catch stuck or payment-failed orders and act on exceptions before the customer notices.
3. **Restock** — watch low-stock signals and have the bot draft a purchase order with a clear rationale.
4. **Serve** — answer order and return questions with a durable, attributable record; never approve a
   refund or price change by colour alone.
5. **Report** — produce a daily sales/inventory summary for the owner with an evidence trail and no PII
   leaking into logs or another bot's view.

## 2. Five concrete changes

All strings are new keys in `lib/i18n.ts` through `t(locale, key)` (here `useTranslation()`); no
hardcoded copy. All use BoardUI components under `apps/web/components/` and the semantic tokens in
`docs/UI.md`, merged with `cx()`. Bot- and catalog-produced content is untrusted: render as text,
never HTML.

**(a) Merchandising context strip — `/app/:botId` (thread header).**
A compact, read-only band above the transcript: chips for "low stock", "open orders", "drafts due",
each with a `Tooltip` (`components/base/tooltip/tooltip.tsx`) that expands detail on focus, not hover
alone. BoardUI `Badge`/`Chip` (`components/base/badges/chip.tsx`), `Tooltip`, `IconButton`
(`components/base/buttons/icon-button.tsx`); surfaces `bg-background-secondary-default`, hairline
`border-separator-border`, text `text-text-secondary`/`text-text-primary`, icons
`text-foreground-icon-secondary`; type `text-caption-1-semibold`. Existing RPCs: `botsGet`,
`threadsHead`, `threadsMessages`. Keep SKUs and customer identifiers out of the DOM where possible.

**(b) Money-action approval card — `/app/:botId` (pending card, REQ-007/REQ-017).**
Make the "what will happen" sentence dominant ("Refund order … to the customer") with an explicit
consequence summary and a distinct confirm label ("Refund order", not just "Approve"). Refunds and
price changes use `border-border-error-default` + `text-text-error-primary` **plus** a warning icon
and the words " Money action" — never colour alone. BoardUI `Button`, `Badge`, `Tooltip`; card
surface `bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`. Existing
RPCs: `threadsAnswer`, `threadsReact`. The card is announced once through the polite live region
(spec §8.9); the transcript is not a live region.

**(c) Order-aware composer — `/app/:botId` (composer, REQ-001).**
Extend `ThreadComposer` with a visible "order-aware" chip and an attach-order `IconButton`
(`RiShoppingBagLine` via `@remixicon/react`) with a `Tooltip`; keep Enter-to-send / Shift+Enter-newline
exactly as today, and keep send disabled while `awaitingApproval`. Focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`, 44 px `.boma-target`,
`text-foreground-icon-primary`. Existing RPC: `threadsSend`; the chip and attached-order context are
local state, so no new transport.

**(d) Fulfilment handover banner — `/app/g/:groupId` (group thread, REQ-010/REQ-067/REQ-068).**
Add a dismissal-free banner naming outgoing and incoming bot with a named status chip, reusing the
existing reassign row (`components/shell/bot-rail.tsx` pattern). BoardUI `Button`, `Badge`, `Avatar`
(`components/base/avatar/avatar.tsx`); tokens `bg-background-secondary-default`,
`border-separator-border`, `text-text-secondary`, `text-text-error-primary` for a failed handover
plus text + icon. Existing RPCs: `groupsGet`, `groupsMessages`, `groupsReassign`
(`runsPause`/`runsResume` if a run must pause). Every message keeps its bot attribution.

**(e) Order triage in the rail — `/app` shell left rail (REQ-009/REQ-011/REQ-012).**
Replace the unread count's bare number with `Counter` (`components/base/badges/counter.tsx`) plus a
named `Chip` status ("Needs review", "Working", "Snoozed till 16:00"), and expose due-time detail via
a keyboard-focusable `Tooltip`. Tokens: `text-text-primary`, `text-text-secondary`,
`bg-background-tertiary-default`, `animate-number-fade` per `docs/UI.md`. Existing RPCs:
`threadsMarkRead`, `threadsMarkUnread`, `threadsSnooze`, `threadsUnsnooze`, `runsList`.

## 3. Accessibility, i18n and RTL

- New labels go in `lib/i18n.ts` (English source; French/Hebrew translations; missing key falls back
  to English, never a blank or the key). No string in JSX.
- `DirectionProvider` sets locale/direction; the boot script writes `lang`/`dir`. Use logical utilities
  (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`, `rounded-s`/`rounded-e`).
  Mirror the reassign/back arrows via `useDirection()`; keep order numbers, SKUs, prices, dates and
  URLs LTR in `<bdi>`/`dir="auto"`.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets via
  `.boma-target`; visible token-driven focus; full keyboard path (card decisions, reassign, snooze);
  status conveyed by text + icon, not colour; 360 px width; `prefers-reduced-motion` honoured.
- Money uses `Intl.NumberFormat` with an explicit currency and integer minor units, never a float; a
  negative value is sign + icon, not red alone. Customer names, emails and addresses never reach a
  hover-only tooltip, logs or another bot's view. Error copy is one sentence of what happened plus one
  of what to do — no IDs, stack traces or provider text on screen (correlation id behind "details",
  spec §8.9).
- Run-status vocabulary stays exactly `queued | leased | running | waiting_input | waiting_takeover |
  completed | failed | cancelled` (spec §8.9).

## 4. Playwright coverage (REQ id in the test name)

- `test("REQ-001 send creates one optimistic message that is not duplicated", ...)`.
- `test("REQ-007 pending money-action card names the exact action, takes focus, and does not block reading", ...)`.
- `test("REQ-017 approve and deny answer the refund card exactly once", ...)`.
- `test("REQ-009 unread divider moves and snooze shelf returns the bot on time", ...)`.
- `test("REQ-011 order triage filters by bot and status and working runs sort first", ...)`.
- `test("REQ-012 every run status renders a distinct named chip", ...)`.
- `test("REQ-010 group thread attributes each message and reassigns by keyboard alone", ...)`.
- `test("REQ-057 rtl mirrors the rail, keeps prices LTR, and axe reports zero critical or serious findings", ...)`.
- New behaviour without a REQ yet (context strip, order-aware chip) must be given a REQ id by the
  owner before implementation — do not invent one in a test name (traceability gate).
- Every golden under `spec/golden/ui/` stays byte-exact; a new seeded state means a new
  `REQ-<id>-<slug>.json` golden, never a click path.

## 5. Best first slice

**Change (b), the money-action approval card** — GUESS, recommended. It is the smallest, highest-value
step: one screen, no new transport (`threadsAnswer` already exists), no new component (BoardUI
`Button`/`Badge`/`Tooltip` are installed), and it directly reduces the risk of an unchecked refund or
price change reaching a customer. Test-first: write `REQ-007 ... names the exact action` and extend
`spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant consequence sentence, the
explicit "Refund order" label, and the text+icon money-action marker. Defer (a), (c), (d), (e) until
the owner assigns REQ ids and approves.

**Decision needed from the owner**: confirm the sector framing and assign REQ ids for behaviour the
spec does not already cover; record any externally visible deviation in `docs/STATE.md` /
`docs/PARITY.md` before shipping.
