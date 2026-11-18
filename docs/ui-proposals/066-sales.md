# UI proposal 066 — sales (revenue operations)

Status: PROPOSAL. Sector framing, persona and screen treatments are GUESS (a sector lens, not in
`spec/`); file, component, token and RPC references are CONFIRMED (read this session). No product
code was touched; this is the only file written.

Sources (CONFIRMED): `docs/UI.md`, `spec/08-ui-ux.md`,
`apps/web/components/thread/thread-composer.tsx`, `apps/web/components/thread/message-blocks.tsx`,
`apps/web/lib/api/rpc.ts` (procedure names). Charter (CONFIRMED, `docs/UI.md` + AGENTS.md): fresh
design (ADR-015); the owner-supplied robot-cat head (`assets/brand/boma-bot-head.svg`) is the only
identity — no upstream brand, code, asset or wording; BoardUI semantic tokens only with `cx()`
(no raw palette class, no hex literal), one design system, no hand-built lookalike; self-hosted only
— no SaaS/cloud, no prospect, contact, quote or contract record leaving the operator's machine.

## 1. Persona and jobs

**Persona — "the sales lead"**: the owner-operator of a small B2B sales team who runs a fleet of
specialist bots (prospect research, outreach, quote drafting, follow-up, pipeline reporting) from
one self-hosted console. Desktop-first at a desk, phone between calls; contacts, pricing, discounts
and contracts are sensitive and the trail matters. A quote sent with a stale price or a discount
approved without a named consequence is a real commercial and legal event, so legible state beats a
fast screen. The robot-cat mark sits beside the wordmark on sign-in and in empty states — never a
CRM, e-mail or payment-brand logo, never borrowed copy.

Top jobs:
1. **Qualify** — research an account and keep the pipeline state visible without hunting the rail.
2. **Outreach** — draft and send a personal message with the right account context attached.
3. **Quote** — produce a quote or proposal at the current price with an explicit approval trail.
4. **Follow up** — triage due conversations, snoozed leads and working runs by owner and status.
5. **Report** — hand a customer a clean, attributable proposal and a pipeline summary, with no PII
   or contact data leaking into logs, tool output or another bot's view.

## 2. Five concrete changes

All strings are new keys reached through `useTranslation()` / `t(locale, key)`; no hardcoded copy.
All use BoardUI components under `apps/web/components/`, semantic tokens from `docs/UI.md`, and
`cx()`. Bot- and connector-produced content (quotes, contact notes) is untrusted: render as text,
never HTML.

**(a) Pipeline context strip — `/app/:botId` (thread header).**
A compact, read-only band above the transcript with named chips for "open opportunities", "quotes
due" and "at-risk accounts", each with a `Tooltip` that reveals detail on focus, not hover alone.
Components: `Chip`/`Badge` (`components/base/badges/chip.tsx`), `Tooltip`, `IconButton`; tokens
`bg-background-secondary-default`, `border-separator-border`, `text-text-secondary` /
`text-text-primary`, `text-foreground-icon-secondary`, type `text-caption-1-semibold`. Existing
RPCs: `botsGet`, `threadsHead`, `threadsMessages`. Keep contact names, emails and account ids out
of the DOM where possible; the strip reads as state, not as a data dump.

**(b) Quote and discount approval card — `/app/:botId` (pending card, REQ-007/REQ-017).**
Make the "what will happen" sentence dominant ("Apply a 15% discount to quote Q-104 and send it to
the customer") with an explicit consequence summary and a distinct confirm label ("Send discounted
quote", not just "Approve"). Discounts and contract changes carry `border-border-error-default` +
`text-text-error-primary` **plus** a warning icon and the words "Money action" — never colour alone.
Extend `MessageBlocks`' `CardBlock` (`message-blocks.tsx`); `Button`, `Badge`, `Tooltip`; card
surface `bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`. Existing
RPC: `threadsAnswer`. The card is announced once through the polite live region (spec §8.9); the
transcript is not a live region.

**(c) Account-aware outreach composer — `/app/:botId` (composer, REQ-001).**
Extend `ThreadComposer` (`thread-composer.tsx`) with a visible "account-aware" chip and an
attach-account `IconButton` (`RiContactsLine` via `@remixicon/react`) with a `Tooltip`; keep
Enter-to-send / Shift+Enter-newline exactly as today, and keep send disabled while
`awaitingApproval`. Focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`, 44 px
`.boma-target`, `text-foreground-icon-primary`. Existing RPC: `threadsSend`; the chip and attached
context are local state, so no new transport.

**(d) Proposal library and shared quote — `/app/artifacts`, `/p/:slugToken` (REQ-036/REQ-037/REQ-039).**
In `ArtifactsGrid`, name quotes, proposals, rate cards and contracts by type, filter by type/bot,
search by name, show the version chain and "open in thread". Content is untrusted — render as data,
never inject artifact HTML into the document. Components: `Input`, `Select`, `Chip`, `Button`;
tokens `text-text-primary` / `text-text-secondary` / `text-text-tertiary`,
`bg-background-primary-default`, `border-border-button-default`. Existing RPCs:
`artifactsListSpace`, `artifactsGet`, `artifactsListVersions`. Reuse `SharedPageScreen`'s block
renderer and safe-link/safe-image walls to publish a compiled customer proposal with no session and
no product chrome beyond a small robot-cat footer; unknown, revoked and expired tokens render one
identical "not available" screen — same words, shape and timing. Transport (CONFIRMED):
`GET /api/v1/pages/shared/{token}`; `noindex`/`no-referrer` and its own CSP stay backend concerns.

**(e) Pipeline triage in the rail — `/app` shell left rail (REQ-009/REQ-011/REQ-012).**
Replace the unread count's bare number with `Counter` (`components/base/badges/counter.tsx`) plus a
named `Chip` status ("Needs reply", "Working", "Snoozed till 16:00"), with due-time detail in a
keyboard-focusable `Tooltip`. Tokens: `text-text-primary`, `text-text-secondary`,
`bg-background-tertiary-default`, `animate-number-fade` per `docs/UI.md`. Existing RPCs:
`threadsMarkRead`, `threadsMarkUnread`, `threadsSnooze`, `threadsUnsnooze`, `runsList`.

## 3. Accessibility, i18n and RTL

- New labels go in `lib/i18n.ts` (English source; French/Hebrew translations; a missing key falls
  back to English, never a blank or the key). No string in JSX.
- `DirectionProvider` sets locale/direction; the boot script writes `lang`/`dir`. Use logical
  utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`, `rounded-s`/
  `rounded-e`). Mirror the attach/disclosure arrows via `useDirection()`; keep prices, quote and
  account ids, dates, emails and URLs LTR in `<bdi>` / `dir="auto"`.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets via
  `.boma-target`; visible token-driven focus; full keyboard path (card decision, attach, snooze,
  proposal download); status by text + icon, not colour; 360 px width; `prefers-reduced-motion`
  honoured.
- Money uses `Intl.NumberFormat` with an explicit currency and integer minor units, never a float;
  a discount is sign + icon + the word "discount", not red alone. Contact names, emails and
  contract values never reach a hover-only tooltip, logs or another bot's view. Error copy is one
  sentence of what happened plus one of what to do — no IDs, stack traces or provider text on screen
  (correlation id behind "details", spec §8.9).
- Run-status vocabulary stays exactly `queued | leased | running | waiting_input | waiting_takeover |
  completed | failed | cancelled` (spec §8.9).

## 4. Playwright coverage (REQ id in the test name)

- `test("REQ-001 send creates one optimistic message that is not duplicated", ...)`.
- `test("REQ-007 pending quote card names the exact action, takes focus, and does not block reading", ...)`.
- `test("REQ-017 approve and deny answer the discount card exactly once", ...)`.
- `test("REQ-009 unread divider moves and snooze shelf returns the lead on time", ...)`.
- `test("REQ-011 pipeline triage filters by bot and status and working runs sort first", ...)`.
- `test("REQ-012 every run status renders a distinct named chip", ...)`.
- `test("REQ-036 proposal library shows the version chain and opens the quote in thread", ...)`.
- `test("REQ-037 shared quote renders without chrome; unknown and revoked tokens share one screen", ...)`.
- `test("REQ-057 rtl mirrors the rail, keeps prices LTR, and axe reports zero critical or serious findings", ...)`.
- New behaviour without a REQ yet (context strip, account-aware chip) must be given a REQ id by the
  owner before implementation — do not invent one in a test name (traceability gate).
- Every golden under `spec/golden/ui/` stays byte-exact; a new seeded state means a new
  `REQ-<id>-<slug>.json` golden, never a click path.

## 5. Best first slice

**Change (b), the quote and discount approval card** — GUESS, recommended. It is the smallest,
highest-value step: one screen, no new transport (`threadsAnswer` already exists), no new component
(`Button`/`Badge`/`Tooltip` are installed), and it directly reduces the risk of an unchecked
discount reaching a customer. Test-first: write `REQ-007 ... names the exact action` and extend
`spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant consequence sentence, the
explicit "Send discounted quote" label, and the text+icon money-action marker. Defer (a), (c), (d),
(e) until the owner assigns REQ ids and approves.

**Decision needed from the owner**: confirm the sector framing and assign REQ ids for behaviour the
spec does not already cover; record any externally visible deviation in `docs/STATE.md` /
`docs/PARITY.md` before shipping.
