# UI proposal 075 — philanthropy (grant stewardship and accountability)

Status: PROPOSAL. Sector framing and priorities are GUESS (a sector lens, not in `spec/`); file,
component, token, REQ id and RPC references are CONFIRMED (read this session). Read-only on product
code; this file is the only write. Sources: `docs/UI.md`, `spec/08-ui-ux.md`,
`spec/01-requirements.md` (REQ-007/011/018/036/037/038/042/045/057/058),
`apps/web/components/board/board-screen.tsx`, `apps/web/components/artifacts/artifacts-grid.tsx`,
`apps/web/lib/api/rpc.ts`, `spec/golden/ui/`. No upstream brand, code, wording or assets consulted.

Charter (CONFIRMED): Boma Bot only; fresh design (ADR-015) with the owner-supplied robot-cat as the
sole identity — no borrowed foundation seal or agency mark; self-hosted/on-prem only, no SaaS and no
donor or beneficiary record leaving the operator's boundary; BoardUI semantic tokens only — no raw
palette class or hex literal — classes merged with `cx()`; one design system, no second library, no
hand-built lookalike of an installed component.

## 1. Persona and top jobs-to-be-done

**Persona — "the programme officer"**: a grants manager at a foundation or nonprofit who runs a
self-hosted Boma Bot and is accountable for other people's money. She works at a desk, answers to a
board and a regulator, and is measured on stewardship, fairness and traceability rather than speed.
Every disbursement needs an authoriser and a recorded reason; every public claim needs a source. An
ambiguous state is worse than a slow screen. The robot-cat mark stays unchanged.

Top jobs:
1. **Steward** — approve or refuse a grant disbursement after reading exactly what will happen, the
   amount, payee and purpose, never confirming on colour alone (§8.9).
2. **Pipeline** — move a grant application from intake to review to decision to disbursement, with
   each decision note attributable.
3. **Evidence** — keep versioned, attributable grant reports and impact figures with a lineage back
   to the bot and run that produced them.
4. **Publish** — share a donor-facing impact page by link that discloses only its intended content,
   with revocation indistinguishable from never issued.
5. **Account** — trace any decision, actor and grant to a record for board reporting and audit.

## 2. Five concrete UI changes

All new strings are `lib/i18n.ts` keys via `useTranslation`/`t(locale, key)`; nothing hardcoded. All
colour rides `docs/UI.md` tokens; every class list goes through `cx()`.

**(a) Consequence-first disbursement card — thread `/app/:botId` (REQ-007, REQ-017, REQ-018).**
Make "what will happen" the dominant line: amount, payee and purpose, plus a distinct confirm label
("Approve disbursement", not just "Approve"). High-impact cards pair `border-border-error-default`
+ `text-text-error-primary` with an icon and the words "High impact" — never colour alone. The
transcript stays readable, other threads reachable, focus lands on the card; only modal approvals
block the composer. BoardUI `card`, `button`, `badge`, `tooltip`, `agent-thinking`; surface
`bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`, type
`text-body-medium`/`text-caption-1-semibold`, focus `focus-visible:ring-2
focus-visible:ring-border-focus-ring`. RPCs (CONFIRMED): `threadsAnswer`, `threadsSend`,
`threadsSubscribe`. GUESS gap: amount/payee/purpose must exist on the card payload; a spending-limit
or purchase-request client (`spendingLimit`/`paymentWallet` namespaces) has no web client yet — the
owner assigns its RPCs and REQ id before any new transport.

**(b) Grant pipeline board — `/app/board` (REQ-038).** Reframe the existing four columns as intake,
review, decision, disbursed; each ticket is a grant application with an assignee reviewer bot and
decision notes as comments. Extend `board-screen.tsx`, not a new screen: keep its `boards.get`
probe, `Phase` states and flag-gated "route absent" behaviour; change only `t("board.column.*")`
labels and add an attributable note caption. RPCs (CONFIRMED): `boardsGet`, `ticketsCreate`,
`ticketsMove`, `ticketsComment`, `botsList` (all in `rpc.ts`). BoardUI `button`, `badge`, `tooltip`,
`select`; tokens `bg-background-full`, `bg-background-primary-default`, `border-border-button-default`,
`text-text-primary`/`text-text-secondary`, `text-foreground-icon-secondary`, `.boma-target`, `cx()`.

**(c) Grant evidence library and viewer — `/app/artifacts`, `/app/artifacts/:artifactId` (REQ-036).**
Reframe `artifacts-grid.tsx` as the evidence locker: name, type icon, size, owner bot, updated time;
filter by type and bot, search by name. The viewer exposes the version chain, owner bot/run and
"open in thread" — the lineage for "who produced this figure, when". Content stays untrusted: never
render artifact HTML in the app document. RPCs (CONFIRMED): `artifactsListSpace`, `artifactsGet`,
`artifactsListVersions`, `artifactsRemove`, `botsList`. BoardUI `card`/grid, `select`, `button`,
`drawer`, `skeleton`; tokens `border-separator-border`, `text-text-tertiary` metadata, icons
`text-foreground-icon-secondary`, `cx()`.

**(d) Donor-facing impact page — `/p/:slugToken` (REQ-037, REQ-039).** No navigation or product
chrome beyond a small footer. Unknown, revoked and expired tokens render the same "Page not
available" document — same words, same shape, no wording or timing difference. Keep block
rendering data-only via the existing `safeLink`/`safeImage` allow-lists and caps; `noindex`/
`no-referrer` and the page's own CSP stay backend concerns. BoardUI `table`, `badge`, `tooltip`;
tokens `bg-background-full`, `border-border-table`, `fill-accent-400`, `cx()`. Transport (CONFIRMED
in `spec/03`): `pages.save|get|share|revokeShare` and `GET /api/v1/pages/shared/{token}` with
`credentials: "omit"`.

**(e) Stewardship ledger — activity + `/admin/access` (REQ-011, REQ-058, REQ-045).** Activity rows
keep prompt snippet, bot, status chip, duration and a link back to the originating message; working
runs sort to the top. The access ledger shows members, invitations and grants with each grant's
reason, `grantedBy` and `createdAt`; role changes are optimistic with rollback. Together they answer
"who approved what, and on what authority". RPCs (CONFIRMED): `runsList`, `adminMembersList`,
`adminInvitationsList`, `adminGrantsList`, `adminGrantsCreate`, `adminGrantsRevoke`. BoardUI `table`,
`chip`, `badge`, `button`, `tooltip`; tokens `border-border-table`, `text-text-primary`,
`text-text-error-primary` + icon + word for destructive rows, `cx()`. GUESS gap: an `audit.export`
web client does not exist (`docs/STATE.md`); assign its RPC and REQ id before adding export UI.

## 3. Accessibility, i18n and RTL

WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; keyboard-only path for
every primary flow (approve, move a grant, revoke a link); visible
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target`; 360 px width;
`prefers-reduced-motion`; no information conveyed by colour alone. Run status uses the exact §8.9
vocabulary as text + icon; destructive and high-impact states add icon + word. Focus lands on a
pending card; refusal and uniform-unavailable screens announce identical text — no leak through the
accessibility tree. Evidence tables carry `<caption>` and `<th scope>`.

English is the source catalogue, fr/he the translations, missing keys fall back to English, never a
blank or the raw key (`docs/UI.md`). `DirectionProvider` gets an explicit BCP 47 locale and the boot
script writes `lang`/`dir` together; RTL mirrors the rail and flips arrows and drawer motion. Logical
utilities throughout (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Amounts,
currencies, dates, URLs, hashes and OTP digits stay LTR in `<bdi>`/`dir="auto"`. Error copy is one
sentence of what happened plus one of what to do — no id, stack trace or provider text on screen;
the correlation id sits behind "details".

## 4. Playwright tests (REQ id in each name)

- `test("REQ-007 a disbursement card names amount, payee, purpose and the exact action, and takes focus", ...)`.
- `test("REQ-018 approve and refuse answer the card exactly once and leave an attributable confirmation", ...)`.
- `test("REQ-038 a grant moves through intake, review, decision and disbursed with an attributable note", ...)`.
- `test("REQ-038 the board route is absent when the deployment flag is off", ...)`.
- `test("REQ-036 the evidence viewer shows the version chain, owner bot and open-in-thread without rendering HTML", ...)`.
- `test("REQ-037 unknown, revoked and expired impact-page tokens render the identical unavailable document", ...)`.
- `test("REQ-039 a shared impact page renders blocks as data only with safe links and images", ...)`.
- `test("REQ-011 an activity row links back to its originating message", ...)`.
- `test("REQ-058 an access grant lists its reason and granter, and revocation states what is lost", ...)`.
- `test("REQ-057 Hebrew RTL mirrors the ledger and axe reports zero critical or serious findings", ...)`.

Goldens under `spec/golden/ui/` stay byte-exact; a new state means a new `REQ-<id>-<slug>.json`
asserting seeded state, never a click path.

## 5. Best first slice

**Change (b), the grant pipeline board — GUESS, recommended.** `board-screen.tsx` already exists,
all five RPCs are landed in `rpc.ts`, `REQ-038` has a golden-style scaffold and an axe entry, and the
screen is flag-gated (safe and reversible). It needs no new transport, component, permission model or
wire format. One screen: relabel the four `t("board.column.*")` keys as grant stages, add an
attributable decision-note caption, and keep every existing behaviour inside REQ-038 — a strong fit
for a deployment whose core job is moving grants from intake to disbursement with a visible state.
Test-first: `REQ-038 a grant moves through intake, review, decision and disbursed with an
attributable note`, then the flag-off test.

**Decisions needed from the owner**: confirm the sector framing; and before code on (a)/(e), assign
RPCs and REQ ids for a spending/purchase UI and an `audit.export` web client — do not invent a test
name or add a transport without them. Any new behaviour outside a REQ stays a proposal here, never a
silent externally visible deviation.
