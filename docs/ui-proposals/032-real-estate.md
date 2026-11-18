# UI proposal 032 — real estate (property listing and transaction operations)

Status: PROPOSAL (sector framing and priorities GUESS; file, token, component and RPC references
CONFIRMED, read this session). Read-only on product code; this file is the only write. Grounding:
`docs/UI.md`, `spec/08-ui-ux.md`, `apps/web/components/board/board-screen.tsx`,
`apps/web/components/artifacts/artifact-viewer.tsx`, `apps/web/lib/api/rpc.ts`,
`apps/web/lib/run-status.ts`. No upstream brand, code, wording or asset consulted or copied.

Charter (CONFIRMED): Boma Bot only; fresh design (ADR-015) with the owner-supplied robot-cat as the
sole identity; no upstream mark or wording; self-hosted only (no SaaS, no listing, tenant or client
data leaving the machine); BoardUI semantic tokens only — no raw palette class or hex literal —
classes merged with `cx()`; one design system, no second library, no hand-built lookalike of an
installed component.

## 1. Persona and top jobs-to-be-done

**Persona — "the property operations manager":** the person running listings and transactions for a
property-management or brokerage firm on the company's own machine. She works across English, French
and Arabic colleagues and clients, often from a tablet on a site visit, and is measured on accuracy
and traceability: an offer accepted on a colour cue, or a lease sent with the wrong attachment, is a
real loss. Data stays in the building; no third-party cloud. The robot-cat mark is the only identity
art — no broker, agency, portal or lender logo is borrowed.

Top jobs:
1. **Track** — name the state of every listing/transaction run out loud (working / paused / terminal
   vocabulary from spec §8.9, `lib/run-status.ts`), without colour-only cues.
2. **Gate** — review a bot-drafted offer, lease or vendor payment and know exactly what will be sent,
   to whom and with which attachment before approving (REQ-007/REQ-017).
3. **Trace** — tie a listing document (floor plan, photo set, title, survey, valuation) to its
   originating run and version, without exposing internals (REQ-036).
4. **Handle exceptions** — spot a failed listing/viewing sync, read a plain-language cause, then retry
   or reassign the delegated research task (REQ-021, REQ-068).
5. **Publish** — share a tenant/buyer-facing listing page by link, with no login and no leakage of
   unrelated listings (REQ-037).

## 2. Five concrete UI changes

All new strings are `lib/i18n.ts` keys via `useTranslation` / `t(locale, key)`; nothing hardcoded in
JSX. Every colour rides the `docs/UI.md` token map; every class list goes through `cx()`.

**(a) Listing pipeline board — `/app/board` (REQ-038, feature-flagged).**
Reuse `BoardScreen` (`components/board/board-screen.tsx`): columns `backlog/todo/in_progress/done`
read as enquiry / shortlisted / under offer / completed, with listing key, title, assignee bot and
last comment. Status is text + icon (`Chip`/`Badge`), never colour alone. Replace the hand-built
`<input>`, `<select>` and `<textarea>` in the create form with BoardUI `Input`, `Select`, `Button`;
keep focus trapped in the detail dialog and Escape-to-close. BoardUI `Button`, `Input`, `Select`,
`Chip`, `Tooltip`; page `bg-background-full`, surface `bg-background-primary-default`, hairline
`border-separator-border`, text `text-text-primary`/`text-text-secondary`/`text-text-tertiary`, focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`. RPCs (CONFIRMED): `boardsGet`,
`botsList`, `ticketsCreate`, `ticketsMove`, `ticketsComment`. A 404 keeps the route absent, not
locked (spec §8.6); do not render a disabled board.

**(b) Offer / lease approval card — thread `/app/:botId` (REQ-007, REQ-017).**
Make the "what will happen" sentence dominant; add property, counterparty and amount context and an
explicit confirm label ("Approve Offer", not just "Approve"). High-impact spends pair
`border-border-error-default` + `text-text-error-primary` with an icon and the words "High impact" —
never colour alone. BoardUI `Button`, `Badge`, `Tooltip`; card `bg-background-primary-default`,
`rounded-3xl`, `border-border-button-default`. RPCs (CONFIRMED): `threadsAnswer`, `threadsReact`,
`threadsSubscribe`. Focus lands on the card; the transcript stays readable and other threads
reachable; announce once via the polite live region (§8.9), the transcript itself is not a live
region. The answered state replaces the controls with an attributable confirmation line.

**(c) Exception and retry/reassign row — activity + thread (REQ-021, REQ-012, REQ-068).**
Follow the activity pattern: a failed listing sync shows one sentence of plain-language cause and one
of what to do, a working Retry button gated on `isRetryable` (`lib/run-status.ts`), and a Reassign
action for a delegated research task. Correlation id behind a "details" `Tooltip` — no stack trace,
id or provider text on the surface (§8.9). BoardUI `Button`, `Chip`, `Tooltip`;
`text-text-error-primary` **plus** icon and the word "Failed", `border-border-error-default`,
`bg-background-primary-default`, 44 px `.boma-target` so the row survives 360 px and touch. RPCs
(CONFIRMED): `runsList`, `runsRetry`, `runsPause`, `runsResume`, `groupsReassign`.

**(d) Listing-document vault — `/app/artifacts` and `/app/artifacts/:artifactId` (REQ-036).**
Reuse `ArtifactsGrid` and `ArtifactViewer` (`components/artifacts/artifact-viewer.tsx`): floor plans,
photo sets, titles, surveys and valuations with type/bot filter and name search; the detail view
shows the version chain, copyable sha256, owner bot and "open in thread". Replace the raw version
`<select>` with BoardUI `Select`. Keep content untrusted — images/PDFs in a bounded frame, text in
`<pre>`, never HTML in the app document. BoardUI `Input`, `Select`, `Chip`, `Button`; tokens
`text-text-primary`/`text-text-secondary`/`text-text-tertiary`, `text-foreground-icon-secondary`,
`border-border-button-default`, `cx()`. RPCs (CONFIRMED): `artifactsListSpace`, `artifactsGet`,
`artifactsListVersions`, `artifactsRemove`. GUESS: a "re-verify title checksum" action has no RPC —
add no button or test until an owner assigns one; keep the hash read-only.

**(e) Tenant/buyer-facing listing page — `/p/:slugToken` (REQ-037).**
Reuse `SharedPageScreen`: no navigation or product chrome beyond a small footer with the robot-cat
mark. Unknown, revoked and expired tokens render one identical "not available" document — same
words, shape and timing. `noindex`/`no-referrer` and its own CSP stay backend concerns; never render
artifact HTML in the page. Tokens `bg-background-full`, `text-text-primary`,
`bg-background-secondary-default`. Transport (CONFIRMED): `GET /api/v1/pages/shared/{token}`.

## 3. Accessibility

WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; a keyboard-only path for
every primary flow (approve, retry, reassign, board move, version switch); visible
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target`; 360 px width;
`prefers-reduced-motion`; no animation on streaming content. Pipeline status and impact are text +
icon using the exact `lib/run-status.ts` vocabulary (`queued`, `paused`, `leased`, `running`,
`waiting_input`, `waiting_takeover`, `completed`, `failed`, `cancelled`). Board columns carry an
`aria-label`, the listing tables `<caption>`/`<th scope>`; the version `Select` keeps a programmatic
label. Focus lands on a pending card and returns to its trigger when the detail dialog closes;
Escape closes. The uniform unavailable screen is announced in identical text — no leak through the
accessibility tree.

## 4. i18n and RTL

English is the source catalogue, fr/he the translations, missing keys fall back to English — never a
blank or the raw key (`docs/UI.md`). `DirectionProvider`
(`@/components/foundations/direction/direction`) gets an explicit BCP 47 locale; the boot script
writes `lang` and `dir` together. RTL mirrors the board columns, the rail and the drawer motion via
`useDirection()`; arrows flip, screen coordinates stay physical. Logical utilities throughout
(`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Listing refs, prices, dates,
URLs and OTP digits stay LTR in `<bdi>`/`dir="auto"`. Error copy is one sentence of what happened
plus one of what to do — no listing data, id, stack trace or provider text on screen.

## 5. Playwright tests (REQ id in each name)

- `test("REQ-038 the listing board shows text-and-icon columns and moves a listing by keyboard", ...)`.
- `test("REQ-038 the board route is absent, not locked, when the flag returns 404", ...)`.
- `test("REQ-007 the offer card names the property, counterparty, amount, consequence and takes focus", ...)`.
- `test("REQ-017 approve and deny answer the offer card exactly once and leave an attributable confirmation", ...)`.
- `test("REQ-021 a failed listing sync shows a plain cause, one retry, and the id behind details", ...)`.
- `test("REQ-068 a delegated research task is reassigned and the new owner is shown", ...)`.
- `test("REQ-036 listing documents filter by type and bot and show the version chain and hash", ...)`.
- `test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)`.
- `test("REQ-012 every run status renders a distinct named chip with the right terminal action", ...)`.
- `test("REQ-057 Hebrew RTL mirrors the board and axe reports zero critical or serious findings", ...)`.

Goldens under `spec/golden/ui/` stay byte-exact; a new state means a new `REQ-<id>-<slug>.json`
asserting seeded state, never a click path. New behaviour without a REQ must get an owner-assigned id
before code — do not invent one in a test name (the re-verify action in change d gets no test yet).

## 6. Best first slice

**Change (b), the offer/lease approval card — GUESS, recommended.** It is the smallest, highest-value
step: one screen, no new transport (`threadsAnswer`/`threadsReact`/`threadsSubscribe` exist), no new
component (BoardUI `Button`/`Badge`/`Tooltip` are installed) and no token re-tune. It directly
prevents an offer, lease or payment being approved on a colour cue. Test-first: write
`REQ-007 the offer card names the property, counterparty, amount, consequence and takes focus`,
extend `spec/golden/ui/REQ-007-pending-card.json`, then ship only the dominant consequence line, the
explicit "Approve Offer" label and the text+icon high-impact marker. Defer (a), (c), (d), (e) until
the owner assigns any missing REQ ids and approves scope; any externally visible deviation is
recorded in `docs/STATE.md` / `docs/PARITY.md` first.

**Decision needed from the owner**: confirm the sector framing, and confirm whether REQ-038's feature
flag is enabled for this deployment before change (a) is scheduled.
