# UI proposal 024 — supply chain (order and shipment operations)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #024 — supply chain.
Read-only on product code; this file is the only write. Grounding (CONFIRMED, read this session):
`docs/UI.md`, `spec/08-ui-ux.md`, `apps/web/components/board/board-screen.tsx`,
`apps/web/components/activity/activity-screen.tsx`, and RPC names in `apps/web/lib/api/rpc.ts`.
No upstream brand, code, assets or wording consulted or copied.

Charter (CONFIRMED): Boma Bot only, fresh design (ADR-015), the owner-supplied robot-cat as the sole
identity; no upstream brand/code/assets/copy; BoardUI semantic tokens only — no raw palette class or
hex literal, classes merged with `cx()`; one design system, no second library or hand-built lookalike;
self-hosted only (no SaaS, no order or supplier data leaving the machine).

## 1. Persona and top jobs-to-be-done

**Persona — "the operations coordinator"**: a logistics/supply-chain manager at a distributor who runs
Boma Bot on the company's own machine. She coordinates purchase orders, carriers and supplier
documents for multilingual staff (English, French, Arabic) and is measured on accuracy, not speed:
an order released to the wrong supplier or a payment approved on a colour cue is a real loss. Data
stays in the building; no third-party cloud. The robot-cat mark is the only identity art — no carrier,
supplier or regulator logo is borrowed.

Top jobs:
1. **Track** — name the state of every order/shipment run out loud (working / paused / terminal
   vocabulary from spec §8.9), without colour-only cues.
2. **Authorise** — review a bot-drafted purchase order or supplier payment and know exactly what will
   be sent and to whom before approving.
3. **Trace** — tie an artifact (invoice, bill of lading, packing list) to its originating run and
   version, without exposing internals.
4. **Handle exceptions** — spot a delayed/failed shipment, read a plain-language cause, then retry or
   reassign the delegated task.
5. **Inform** — publish a supplier-facing status page by link, with no login and no leakage of
   unrelated orders.

## 2. Five concrete UI changes

All copy is a new `lib/i18n.ts` key via `useTranslation` / `t(locale, key)`; no string in JSX. Every
class list goes through `cx()`; every colour rides a `docs/UI.md` token.

**(a) Order/shipment status board — `/app/board` (REQ-038, feature-flagged).**
Reuse `BoardScreen` (`components/board/board-screen.tsx`): columns `backlog/todo/in_progress/done`
read as open / allocated / in transit / received, with ticket key, title, assignee bot and last
comment. Status is text + icon (`Chip`), never colour alone. BoardUI `Button`, `Input`, `Select`,
`Chip`/`Badge`, `Tooltip`; page `bg-background-full`, surfaces `bg-background-secondary-default`,
hairline `border-separator-border`, text `text-text-primary`/`text-text-secondary`, focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`. RPCs (CONFIRMED): `boardsGet`,
`botsList`, `ticketsCreate`, `ticketsMove`, `ticketsComment`. A 404 keeps the route absent, not
locked (spec §8.6).

**(b) Purchase-approval card — thread `/app/:botId` (REQ-007, REQ-017).**
Make the "what will happen" sentence dominant; add supplier and amount context and a distinct confirm
label ("Approve Purchase Order", not just "Approve"). High-impact spends pair
`border-border-error-default` + `text-text-error-primary` with an icon and the word "High impact" —
never colour alone. BoardUI `Button`, `Badge`, `Tooltip`; card `bg-background-primary-default`,
`rounded-3xl`, `border-border-button-default`. RPCs (CONFIRMED): `threadsAnswer`, `threadsReact`,
`threadsSubscribe`. Transcript stays readable, other threads reachable, focus lands on the card;
announce once via the polite live region (§8.9).

**(c) Exception and retry/reassign row — activity + thread (REQ-021, REQ-012, REQ-068).**
Follow `ActivityScreen` (`components/activity/activity-screen.tsx`): a delayed/failed shipment shows
one sentence of cause plus one of what to do, a working Retry `Button`, and a Reassign action for a
delegated task. Correlation id behind a "details" `Tooltip` — no stack trace, id or provider text on
the surface. BoardUI `Button`, `Badge`, `Tooltip`; `text-text-error-primary`,
`border-border-error-default`, `bg-background-primary-default`, 44 px `.boma-target`. RPCs
(CONFIRMED): `runsList`, `runsRetry`, `runsPause`, `runsResume`, `groupsReassign`.

**(d) Shipping-documents library — `/app/artifacts` (REQ-036).**
Reuse `ArtifactsGrid`: invoices, bills of lading and packing lists with a type/bot filter and name
search; show the version chain and "open in thread". Artifact content is untrusted — render as data,
never HTML in the document. Keep the robot-cat empty state when no documents exist. BoardUI `Input`,
`Select`, `Chip`, `Button`; `bg-background-primary-default`, `border-border-button-default`,
`text-text-primary`/`text-text-secondary`/`text-text-tertiary`, `text-foreground-icon-secondary`.
RPCs (CONFIRMED): `artifactsListSpace`, `artifactsGet`, `artifactsListVersions`.

**(e) Supplier-facing shared status page — `/p/:slugToken` (REQ-037).**
Reuse `SharedPageScreen`: no navigation or product chrome beyond a small footer with the robot-cat
mark. Unknown, revoked and expired tokens render one identical "not available" document — same words,
shape and timing. `noindex`/`no-referrer` and its own CSP stay backend concerns; never render
artifact HTML in the page. Tokens `bg-background-full`, `text-text-primary`,
`bg-background-secondary-default`. Transport (CONFIRMED): `GET /api/v1/pages/shared/{token}`.

## 3. Accessibility

WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; keyboard-only path for
every primary flow (approve, retry, reassign, board move); visible
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target`; 360 px width;
`prefers-reduced-motion` honoured; no animation on streaming content. Order status and impact are
text + icon, using the exact run-status vocabulary in §8.9 (queued/leased/running, waiting_input/
waiting_takeover, completed/failed/cancelled). Tables carry `<caption>` and `<th scope>`. Focus lands
on a pending card; the uniform unavailable screen is announced in identical text — no leak through
the accessibility tree.

## 4. i18n and RTL

English is the source catalogue, fr/he the translations, missing keys fall back to English — never a
blank or the raw key. `DirectionProvider` (`@/components/foundations/direction/direction`) gets an
explicit BCP 47 locale; the boot script writes `lang` and `dir` together. RTL mirrors the board's
columns, the rail and the drawer motion via `useDirection()`; arrows flip. Logical utilities
throughout (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Order keys, tracking
numbers, URLs, amounts, dates and OTP digits stay LTR in `<bdi>`/`dir="auto"`. Error copy is one
sentence of what happened plus one of what to do — no supplier data, id or provider text on screen.

## 5. Playwright tests (REQ id in each name)

- `test("REQ-038 the order board shows text-and-icon status columns and moves a ticket by keyboard", ...)`.
- `test("REQ-038 the board route is absent, not locked, when the flag returns 404", ...)`.
- `test("REQ-007 the purchase card names the supplier, amount, consequence and takes focus", ...)`.
- `test("REQ-017 approve and deny answer the purchase card exactly once and leave an attributable confirmation", ...)`.
- `test("REQ-021 a delayed shipment shows a plain-language cause and a working retry", ...)`.
- `test("REQ-068 a delegated task is reassigned and the new owner is shown", ...)`.
- `test("REQ-036 shipping documents filter by type and bot and show the version chain", ...)`.
- `test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)`.
- `test("REQ-012 every run status renders a distinct named chip with the right terminal action", ...)`.
- `test("REQ-057 Hebrew RTL mirrors the board and axe reports zero critical or serious findings", ...)`.

Goldens under `spec/golden/ui/` stay byte-exact; a new state means a new `REQ-<id>-<slug>.json`
asserting seeded state, never a click path. New behaviour without a REQ must get an owner-assigned id
before code — do not invent one in a test name.

## 6. Best first slice

**Change (b), the purchase-approval card** — GUESS, recommended. It is the smallest, highest-value
step: one screen, no new transport (`threadsAnswer`/`threadsReact` exist), no new component (BoardUI
`Button`/`Badge`/`Tooltip` are installed) and no token re-tune. It directly prevents a purchase order
or payment being approved on a colour cue. Test-first: write `REQ-007 ... names the supplier, amount,
consequence and takes focus`, extend `spec/golden/ui/REQ-007-pending-card.json`, then ship only the
dominant consequence line, the explicit "Approve Purchase Order" label and the text+icon high-impact
marker. Defer (a), (c), (d), (e) until the owner assigns any missing REQ ids and approves scope.

**Decision needed from the owner**: confirm the sector framing, and confirm whether REQ-038's
feature flag is in scope for this deployment before change (a) is scheduled.
