# UI proposal 068 — procurement (sourcing, purchase authorisation, supplier trace)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #068 — procurement.
Read-only on product code; this file is the only write. Grounding (CONFIRMED, read this session):
`docs/UI.md`, `spec/08-ui-ux.md`, `spec/01-requirements.md` (REQ-007/017/018/021/036/037/038),
`spec/03-api-contract.md`, `apps/web/lib/api/rpc.ts`, and two components:
`apps/web/components/thread/message-blocks.tsx` and `apps/web/components/activity/activity-screen.tsx`.
No upstream brand, code, assets or wording consulted or copied.

Charter (CONFIRMED): Boma Bot only, fresh design (ADR-015); the owner-supplied robot-cat is the sole
identity art — no supplier, carrier, payment-brand or regulator logo or copy is borrowed. BoardUI
semantic tokens only — no raw palette class or hex literal, classes merged with `cx()`, dark mode via
tokens. One design system, no second library or hand-built lookalike. **No SaaS** — self-hosted;
hosted checkout/subscription (REQ-051) stays off. Existing RPCs only; no new transport invented.

## 1. Persona and top jobs

**Persona — "the procurement lead"**: a category manager at a mid-size distributor who runs Boma Bot
on the company's own machine. She works in English, French and Arabic, and is measured on accuracy,
not speed: a purchase order released to the wrong supplier or a payment approved on a colour cue is
a real, irreversible loss. Supplier quotes, invoices and spend data never leave the building. The
robot-cat mark is the only identity art on every screen.

Top jobs:
1. **Source** — drive an enquiry from draft to requested to quoted to awarded without reading chat.
2. **Authorise** — review a bot-drafted purchase request and know exactly what will be paid, to whom
   and for what, before committing; never confirm on colour alone.
3. **Contain** — see instantly when a request was refused at the spending cap, with one next action.
4. **Trace** — tie a quote or invoice to its originating run and version, without exposing internals.
5. **Inform** — give a supplier a read-only status page by link, with no login and no leakage of
   unrelated orders or spend.

## 2. Five concrete UI changes

All copy is a new `lib/i18n.ts` key through `useTranslation` / `t(locale, key)` — no string in JSX.
Every class list goes through `cx()`; every colour rides a `docs/UI.md` token.

**(a) Purchase-request card — thread `/app/:botId` (`thread/message-blocks.tsx`, `CardBlock`).**
The `purchase` kind is already branched (CONFIRMED), and answering goes through `threadsAnswer`
with a CONFLICT branch that unlocks the gate honestly (REQ-017 idempotence, CONFIRMED). Make the
"what will happen" sentence dominant and use an explicit confirm label ("Approve Purchase Order",
not "Approve"). High-impact requests pair `border-border-error-default` + `text-text-error-primary`
with `RiAlertLine` and the word "High impact" — never colour alone. Migrate the raw `<button>`/
`<input>` in this block (CONFIRMED at lines 178–218) to installed BoardUI `Button`/`Input`. Surface
`bg-background-secondary-default`, `rounded-3xl`, `border-separator-border`, text
`text-text-primary`/`text-text-secondary`; RPC `threadsAnswer`. REQ-007, REQ-017, REQ-018. **Wire gap
(important)**: the `card` block carries only `cardId` + `kind` today (CONFIRMED `rpc.ts:27`); supplier,
amount and purpose are *not* on the web wire, so naming them needs a contract addition + owner-assigned
REQ id — do not invent them. The dominant line, explicit label, marker and control migration ship now.

**(b) Spending-cap refusal row — thread transcript (`thread/message-blocks.tsx` / `thread-screen.tsx`).**
When a paid request is refused at the cap, show one sentence of what happened and one of what to do
with a single `Button` action. Branch on the typed `RpcError.code` already returned by `call()` —
never parse message strings. Use `border-border-error-default`, `text-text-error-primary`, plus icon
and text; keep the correlation id behind a "Details" `Tooltip` (spec §8.9). RPCs: existing
`threadsSend` / `threadsAnswer` error envelopes; `authCapabilities` carries the billing flag
(CONFIRMED) — no new transport, no checkout UI. REQ-018, REQ-021.

**(c) Sourcing / order board — `/app/board` (feature-flagged, REQ-038).**
Reuse `BoardScreen` (`components/board/board-screen.tsx`): columns `backlog/todo/in_progress/done`
read as draft / requested / quoted / awarded, with ticket key, title, assignee bot and last comment.
Status is text + icon (`Chip`), never colour alone, using the exact §8.9 vocabulary. BoardUI
`Button`, `Input`, `Select`, `Chip`/`Badge`, `Tooltip`; page `bg-background-full`, surfaces
`bg-background-secondary-default`, hairline `border-separator-border`, focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`. RPCs (CONFIRMED): `boardsGet`,
`botsList`, `ticketsCreate`, `ticketsMove`, `ticketsComment`. A 404 keeps the route absent, not locked.

**(d) Quote and invoice shelf — `/app/artifacts` (`artifacts/artifacts-grid.tsx`, artifact viewer).**
Filter the grid to supplier quotes, invoices and purchase orders with a type/bot filter and name
search; show the version chain and "open in thread" so each document traces to its run. Artifact
content is untrusted — render as data, never HTML in the document. Keep the robot-cat empty state.
BoardUI `Input`, `Select`, `Chip`, `Button`; `bg-background-primary-default`,
`border-border-button-default`, `text-text-primary`/`text-text-secondary`/`text-text-tertiary`,
`text-foreground-icon-secondary`. RPCs (CONFIRMED): `artifactsListSpace`, `artifactsGet`,
`artifactsListVersions`. REQ-036.

**(e) Supplier-facing shared status page — `/p/:slugToken` (REQ-037).**
Reuse `SharedPageScreen`: no navigation or product chrome beyond a small footer with the robot-cat
mark. Unknown, revoked and expired tokens render one identical "not available" document — same
words, shape and timing, no leak through the accessibility tree. `noindex`/`no-referrer` and its own
CSP stay backend concerns; never render artifact HTML in the page. Tokens `bg-background-full`,
`text-text-primary`, `bg-background-secondary-default`. Transport (CONFIRMED, `spec/03-api-contract.md`): `GET /api/v1/pages/shared/:token`.

## 3. Accessibility

WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; a keyboard-only path for
every primary flow (approve, cap-refusal action, board move, download); visible
`focus-visible:ring-2 focus-visible:ring-border-focus-ring` / `.boma-focus`; ≥44 px `.boma-target`;
360 px width; `prefers-reduced-motion` honoured; no animation on streaming content. Status and impact
are text + icon, using the §8.9 run-status vocabulary — never colour alone. Focus lands on a pending
card; the approval is blocking for its effect only, transcript readable and other threads reachable;
card arrivals and status changes go through the polite live region, the transcript is not one (§8.9).
Tables carry `<caption>` and `<th scope>`. Keep the `docs/UI.md` contrast re-maps (dark
`text-secondary` → neutral-400; light nav labels `text-primary`) rather than overriding them.

## 4. i18n and RTL

English is the source catalogue, fr/he the translations; missing keys fall back to English — never a
blank or the raw key. `DirectionProvider` (`@/components/foundations/direction/direction`) gets an
explicit BCP 47 locale; the boot script writes `lang` and `dir` together. RTL mirrors the board's
columns, the rail and drawer motion via `useDirection()`; arrows flip. Logical utilities throughout
(`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). PO numbers, supplier ids,
amounts, dates, URLs and OTP digits stay LTR in `<bdi>`/`dir="auto"` and are formatted with
`Intl.NumberFormat`/`Intl.DateTimeFormat` — formatting is separate from layout direction. Error copy
is one sentence of what happened plus one of what to do: no supplier data, id, stack trace or provider
text on screen.

## 5. Playwright test names (REQ id in each name)

```ts
test("REQ-007 the purchase card takes focus and does not block reading or other threads", ...)
test("REQ-018 the purchase card names the consequence and shows an explicit Approve Purchase Order label", ...)
test("REQ-017 approve and deny answer the purchase card exactly once and leave an attributable confirmation", ...)
test("REQ-018 a spending-cap refusal shows a visible reason, one next action and no stack trace", ...)
test("REQ-021 a failed paid run shows a plain-language cause and a working retry", ...)
test("REQ-038 the sourcing board shows text-and-icon status columns and moves a ticket by keyboard", ...)
test("REQ-038 the board route is absent, not locked, when the flag returns 404", ...)
test("REQ-036 supplier quotes and invoices filter by type/bot and show the version chain", ...)
test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)
test("REQ-057 the procurement screens mirror for Hebrew with a clean axe run", ...)
```

Goldens under `spec/golden/ui/` stay byte-exact; a new state means a new `REQ-<id>-<slug>.json`
asserting seeded state, never a click path. New behaviour without a REQ must get an owner-assigned id
before code — do not invent one in a test name.

## 6. Best first slice

**Change (a), the purchase-request card** — GUESS, recommended. Smallest and highest-value: one
component, no new transport (`threadsAnswer` exists), no new install (BoardUI `Button`/`Input` are
vendored), no token re-tune, and it directly prevents a purchase being approved on a colour cue.
Test-first: write `REQ-018 ... names the consequence and shows an explicit Approve Purchase Order
label`, extend `spec/golden/ui/REQ-007-pending-card.json`, then ship only the dominant consequence
line, the explicit label, the text+icon high-impact marker and the raw-control migration. Defer (b)–(e).

**Decisions needed from the owner**: (1) approve adding supplier/amount/purpose fields to the card
block contract (with a REQ id), or accept a generic consequence line; (2) confirm the REQ-038 board
flag is in scope; (3) confirm whether supplier rooms with @mention routing and `REQ-068` reassignment
are in this sector's scope before scheduling them as a later change.

Provenance: CONFIRMED claims were read this session from the files named in the header; the persona,
its jobs and the proposed presentation are a GUESS (proposal only — no procurement sector in spec).
