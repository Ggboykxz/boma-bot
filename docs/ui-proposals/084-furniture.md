# UI proposal 084 — furniture (made-to-order workshop, showroom and catalog)

Status: PROPOSAL. Sector framing and priorities are GUESS (the spec is industry-neutral); every file,
component, token and RPC reference below is CONFIRMED read this session. Read-only on product code;
this file is the only write. Grounding: `docs/UI.md`, `spec/08-ui-ux.md`, and two product components —
`apps/web/components/board/board-screen.tsx` and `apps/web/components/artifacts/artifacts-grid.tsx`.
RPC names checked against `apps/web/lib/api/rpc.ts`; status vocabulary against `apps/web/lib/run-status.ts`.

Charter (CONFIRMED): Boma Bot only; no upstream brand, code, wording or asset; fresh design (ADR-015)
with the owner-supplied robot-cat as the sole identity art; BoardUI semantic tokens only — no raw
palette class or hex literal — class lists merged with `cx()`; one design system, no second library or
hand-built lookalike of an installed component; self-hosted on the workshop's own machine — no SaaS
and no customer order, design, supplier or material data leaving it.

## 1. Persona and top jobs-to-be-done

**Persona — the workshop/showroom operations lead** at a furniture maker doing bespoke and small-batch
pieces (chairs, cabinets, tables). She works in French, English and Arabic with clients and suppliers,
often from a dusty tablet at the bench with gloves on. The bot runs on the shop's own server: cut
lists, CAD, fabric/hardware inventory, quotes and customer details never touch a third-party cloud. A
wrong click that raises a purchase order for rare hardwood, releases a material hold or sends a quote
with the wrong finish is a real financial and trust loss, so an ambiguous state is worse than latency.
The robot-cat mark is the only identity — no furniture brand, catalogue or supplier logo is borrowed.

Top jobs:
1. **Quote** — draft a bot-made quote/spec for a bespoke piece (dimensions, material, finish, quantity) and know exactly what will be sent to the client before it goes.
2. **Track** — name the state of every order run out loud (working / paused / terminal from `lib/run-status.ts`), never by colour alone.
3. **Gate** — approve a bot-proposed material purchase or supplier PO only after seeing exactly what will be bought, from whom, and for how much (REQ-007/REQ-017).
4. **Handle exceptions** — spot a failed material/supplier sync, read a plain-language cause, then retry only the failure; pause or resume queued work (REQ-021, REQ-012).
5. **Publish and trace** — keep cut lists, CAD, finish specs and photos versioned per order (REQ-036), and share a client-facing spec sheet by link with no login and no leakage of other clients' work (REQ-037).

## 2. Five concrete UI changes

All new strings are `lib/i18n.ts` keys through `useTranslation` / `t(locale, key)`; nothing hardcoded in
JSX. Every colour rides the `docs/UI.md` token map; every class list goes through `cx()`.

**(a) Quote / purchase-order approval card — thread `/app/:botId` (REQ-007, REQ-017).**
Make the "what will happen" sentence dominant; add material, dimensions, finish, quantity, supplier and
total, and an explicit confirm label ("Approve Purchase Order", not just "Approve"). High-impact buys
(rare timber, large spend) pair `border-border-error-default` + `text-text-error-primary` with an icon and
the words "High impact" — never colour alone. Reuse installed BoardUI `Button`, `Badge` (chip), `Tooltip`;
card `bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`, text
`text-text-primary`/`text-text-secondary`, focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`.
RPCs (CONFIRMED): `threadsAnswer`, `threadsReact`, `threadsSubscribe`. Focus lands on the card; the
transcript stays readable and other threads reachable; announce once via the polite live region
(spec §8.9) — the transcript is not a live region. The answered state replaces controls with an
attributable confirmation line.

**(b) Workshop order board — `/app/board` (REQ-038, feature-flagged).**
Reuse `BoardScreen` (`components/board/board-screen.tsx`): its four columns `backlog/todo/in_progress/done`
read as to-quote / to-build / in-the-workshop / delivered, with order key, title, assignee bot and last
comment. Replacing the raw `<input>`, `<textarea>` and `<select>` in the create form with installed
BoardUI `Input`, `Select`, `Button` keeps focus trapped in the detail dialog and Escape-to-close (the
dialog code is already there). Status is text + icon (`Chip`/`Badge`), never colour alone. Tokens: page
`bg-background-full`, surface `bg-background-primary-default`, hairline `border-separator-border`, text
`text-text-primary`/`text-text-secondary`/`text-text-tertiary`. RPCs (CONFIRMED): `boardsGet`, `botsList`,
`ticketsCreate`, `ticketsMove`, `ticketsComment`. A 404 keeps the route absent, not locked (§8.6).

**(c) Material-status context strip — thread `/app/:botId` header (new behaviour, needs a REQ id).**
A compact read-only band above the transcript: chips for "current order", "material on hold", and
"stock to reorder", each with a `Tooltip` for detail. Reuse `Chip`/`Badge` (`components/base/badges/chip.tsx`),
`Tooltip`, `IconButton`; surface `bg-background-secondary-default`, hairline `border-separator-border`,
text `text-text-secondary`/`text-text-primary`, icons `text-foreground-icon-secondary`, type
`text-caption-1-semibold`. RPCs (CONFIRMED): `botsGet`, `threadsHead`, `threadsMessages`. Bot-produced
values are untrusted — render as text, never HTML; keep SKU codes out of logs.

**(d) Exception and retry/reassign row — activity view + thread (REQ-021, REQ-012, REQ-068).**
Follow the activity pattern: a failed material/supplier sync shows one sentence of plain cause and one
of what to do, a working Retry gated on `isRetryable` (`lib/run-status.ts`), a Pause/Resume for queued
work, and Reassign for a delegated sourcing task. Correlation id behind a "details" `Tooltip` — no stack
trace, id or provider text on the surface (§8.9). BoardUI `Button`, `Chip`, `Tooltip`;
`text-text-error-primary` **plus** icon and the word "Failed", `border-border-error-default`,
`bg-background-primary-default`, 44 px `.boma-target` for gloved touch. RPCs (CONFIRMED): `runsList`,
`runsRetry`, `runsPause`, `runsResume`, `groupsReassign`.

**(e) Cut-list library and client spec sheet — `/app/artifacts`, `/app/artifacts/:artifactId`, `/p/:slugToken`
(REQ-036, REQ-037).**
Reuse `ArtifactsGrid` (`components/artifacts/artifacts-grid.tsx`): name cut lists, CAD, finish specs,
photos and quotes by type/bot, filter and search by name, and show the version chain in the viewer.
Replace that file's raw search `<input>` and two `<select>` with installed BoardUI `Input` and `Select`.
Tokens: `text-text-primary`/`text-text-secondary`/`text-text-tertiary`, `text-foreground-icon-secondary`,
`bg-background-primary-default`, `border-border-button-default`. RPCs (CONFIRMED): `artifactsListSpace`,
`artifactsGet`, `artifactsListVersions`, `artifactsRemove`, `botsList`. Publish a compiled spec sheet to a
client with no session and no chrome beyond a small footer with the robot-cat mark; unknown, revoked and
expired tokens show one identical "not available" document — same words, shape and timing; `noindex`,
`no-referrer`, its own CSP. Transport (CONFIRMED): `GET /api/v1/pages/shared/{token}`. Never render
artifact HTML in the app document; keep the robot-cat empty state when nothing is filed.

## 3. Accessibility, i18n and RTL

- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; a keyboard-only path for every primary flow (approve, retry, reassign, board move, version switch); visible `focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target` for gloved use; 360 px width; `prefers-reduced-motion`; no animation on streaming content. Status and impact are text + icon using the exact nine-status `lib/run-status.ts` vocabulary. Board columns carry `aria-label`, the grid lists `<caption>`/`<th scope>` where tabular, and the version `Select` a programmatic label.
- English is the source catalogue, fr/he the translations; a missing key falls back to English, never a blank or the raw key (`docs/UI.md`). `DirectionProvider`
  (`@/components/foundations/direction/direction`) gets an explicit BCP 47 locale; the boot script writes `lang`/`dir` together. RTL mirrors the board columns, the rail and drawer motion via `useDirection()`; arrows flip, measured screen coordinates stay physical. Logical utilities throughout (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Keep SKUs, dimensions, currency/prices, dates, URLs and OTP digits LTR in `<bdi>`/`dir="auto"`. Error copy is one sentence of what happened plus one of what to do — no order id, price, machine id, stack trace or provider text on screen.

## 4. Playwright tests (REQ id in each name)

- `test("REQ-007 the PO card names material, dimensions, finish, supplier, total and takes focus", ...)`
- `test("REQ-017 approve and deny answer the card exactly once and leave an attributable line", ...)`
- `test("REQ-038 the workshop board shows text-and-icon columns and moves an order by keyboard", ...)`
- `test("REQ-038 the board route is absent, not locked, when the flag returns 404", ...)`
- `test("REQ-021 a failed material sync shows a plain cause, one retry, and the id behind details", ...)`
- `test("REQ-068 a delegated sourcing task is reassigned and the new owner is shown", ...)`
- `test("REQ-036 cut lists and finish specs filter by type and bot and show the version chain", ...)`
- `test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)`
- `test("REQ-012 every run status renders a distinct named chip with the right terminal action", ...)`
- `test("REQ-057 Hebrew RTL mirrors the board and axe reports zero critical or serious findings", ...)`
- Goldens under `spec/golden/ui/` stay byte-exact; extend `REQ-007-pending-card.json` for (a). New behaviour
  without a REQ (the material-status strip in change c) gets no test until the owner assigns an id.

## 5. Best first slice

**Change (a), the quote / purchase-order approval card — GUESS, recommended.** Smallest, highest-value
step: one screen, no new transport (`threadsAnswer`/`threadsReact`/`threadsSubscribe` exist), no new
component (BoardUI `Button`/`Badge`/`Tooltip` are installed), no token re-tune. It directly prevents an
expensive material order or client quote being approved on a colour cue. Test-first: write
`REQ-007 the PO card names material, dimensions, finish, supplier, total and takes focus`, extend
`spec/golden/ui/REQ-007-pending-card.json`, then ship only the dominant consequence line, the explicit
"Approve Purchase Order" label and the text+icon high-impact marker. Defer (b)–(e) until the owner
assigns a REQ id for (c) and approves scope; record any externally visible deviation in `docs/STATE.md` /
`docs/PARITY.md` first.
