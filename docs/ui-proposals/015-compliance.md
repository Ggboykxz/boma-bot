# UI proposal 015 — compliance (accountable administration)

Status: PROPOSAL. Sector framing and priorities are GUESS (a sector lens, not in `spec/`); file,
component, token and RPC references are CONFIRMED (read this session). Read-only on product code;
this is the only file written. Sources: `docs/UI.md`, `spec/08-ui-ux.md`,
`apps/web/components/admin/{access-screen,plugins-screen}.tsx`,
`apps/web/components/shared-page/shared-page-screen.tsx`, `apps/web/lib/api/rpc.ts`,
`apps/web/tests/admin.spec.ts`. No upstream brand, code, wording or assets consulted or copied.

Charter (CONFIRMED): Boma Bot only; fresh design (ADR-015) with the robot-cat as the sole identity;
no upstream mark or wording; self-hosted only (no SaaS, no tenant data leaving the machine);
BoardUI semantic tokens only — no raw palette classes or hex literals — classes merged with `cx()`;
one design system, no second library, no hand-built lookalike of an installed component.

## 1. Persona and top jobs-to-be-done

**Persona — "the accountable administrator"**: a space admin / compliance officer in a regulated
organisation. She works at a desk, serves multilingual staff (English, French, Arabic), and is
measured on proof, not speed: every privileged act must be attributable, every external link
minimum-disclosure, every consequential decision explicit. Boma Bot runs self-hosted; no record
leaves the building. The robot-cat mark stays unchanged — no borrowed corporate or regulator mark.

Top jobs:
1. **Prove access** — show who belongs, what is pending, and why each privilege grant exists.
2. **Vet code** — review third-party plugin submissions and record the decision without executing them.
3. **Bound exposure** — confirm a public link reveals only its intended page and that revoked links are indistinguishable from never-issued ones.
4. **Authorise** — approve or refuse a consequential bot action after reading exactly what will happen and who is authorising it.
5. **Trace** — tie a run or artifact back to its originating message and version, without leaking internals.

## 2. Five concrete UI changes

All new strings are keys in `lib/i18n.ts` via `useTranslation` / `t(locale, key)`; nothing hardcoded.

**(a) Access ledger — `/admin/access` (REQ-058).**
Reframe the existing three sections (members, pending invitations, access grants) as one ledger:
columns for name, email (role), last seen; each grant shows reason, `grantedBy` and `createdAt`;
revocation asks for confirmation naming what the person loses. Replace the hand-built `<table>` and
raw `<input>` with BoardUI `table`, `input`, `button`, `badge`, `tooltip`; route the hardcoded
"Name"/"Role" headers through `t()` (current i18n gap). Tokens: `bg-background-full` page,
`border-border-table`, `text-text-primary`, `text-text-error-primary` + icon + word for destructive
rows, `cx()`. RPCs (CONFIRMED): `adminMembersList`, `adminInvitationsList`, `adminGrantsList`,
`adminGrantsCreate`, `adminGrantsRevoke`, `adminInvitationsRevoke`; `FORBIDDEN` renders the refusal
sentence, never a table.

**(b) Plugin vetting queue — `/admin/plugins` (REQ-058).**
Queue newest-first; show source, submitter-supplied name, review state and the parts the installer
refused with their reasons; add a one-sentence "approving only flips review status, it runs nothing"
caption and record who decided. Source is untrusted text (`<code dir="auto">`), never markup or code
to execute. BoardUI `table`, `button`, `badge`/`chip`, `tooltip`; tokens `border-border-table`,
`text-text-secondary`, `text-text-error-primary` + icon + "Refused" word, `cx()`. RPCs (CONFIRMED):
`adminPluginsList`, `adminPluginsDecide`.

**(c) Shared-link privacy surface — `/p/:slugToken` (REQ-037).**
No navigation or product chrome. Unknown, revoked and expired tokens render the same "Page not
available" document — same words, same shape, no wording/timing difference. `noindex`/`no-referrer`
and the page's own CSP stay backend concerns. Keep block rendering data-only with the existing
`safeLink`/`safeImage` allow-lists and caps. BoardUI `table`, `badge`, `tooltip`; tokens
`bg-background-full`, `border-border-table`, `fill-accent-400`/`stroke-separator-border`, `cx()`.
Transport (CONFIRMED): `GET /api/v1/pages/shared/{token}` with `credentials: "omit"`.

**(d) Consequential-action approval card — thread `/app/:botId` (REQ-007, REQ-017).**
Make the "what will happen" sentence the dominant line; add the authorising member and a distinct
confirm label ("Approve payout", not just "Approve"). High-impact cards use
`border-border-error-default` + `text-text-error-primary` **plus** an icon and the words
"High impact" — never colour alone. Transcript stays readable, other threads reachable, focus lands
on the card; the answered state replaces controls with an attributable confirmation line. BoardUI
`card`, `button`, `badge`, `textarea`, `agent-thinking`; `rounded-3xl`, `cx()`. RPCs (CONFIRMED):
`threadsAnswer`, `threadsSend`, `threadsSubscribe`; announced once via the polite live region, the
transcript itself is not a live region.

**(e) Accountable run and artifact trail — activity + `/app/artifacts/:artifactId`
(REQ-011, REQ-036).**
Activity rows keep prompt snippet, bot, status chip, duration and link back to the originating
message; working runs sort to the top. The artifact viewer exposes version chain, owner bot/run,
hash and "open in thread" — the lineage needed to answer "who produced this, when". Never render
artifact HTML; correlation id behind "details" only. BoardUI `table`, `chip`, `select`, `button`,
`drawer`, `skeleton`; tokens `text-text-secondary` metadata, `border-border-table`, `cx()`. RPCs
(CONFIRMED): `runsList`, `artifactsListSpace`, `artifactsGet`, `artifactsListVersions`. GUESS gap:
`RunView` has `trigger` but no named human actor; if "who triggered it" must be shown, the owner
assigns that field/REQ — do not invent it.

## 3. Accessibility

WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; keyboard-only path for
every primary flow (revoke, decide, approve, takeover); visible
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target`; 360 px width;
`prefers-reduced-motion`; no animation on streaming content. Evidence tables carry `<caption>` and
`<th scope>`; chips are text + icon, never colour (exact run-status vocabulary from spec §8.9).
Destructive and refused states add icon + word, not red alone. Focus lands on a pending card; the
refusal and the uniform unavailable screen are announced in identical text — no leak through the
accessibility tree.

## 4. i18n and RTL

English is the source catalogue, fr/he the translations, missing keys fall back to English
(`docs/UI.md`). `DirectionProvider` gets an explicit BCP 47 locale and the boot script writes
`lang`/`dir`; RTL mirrors the ledger rail and flips arrows/drawer motion. Logical utilities
throughout (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`). Emails, URLs, hashes, dates and OTP digits
stay LTR in `<bdi>`/`dir="auto"`. Error copy is one sentence of what happened plus one of what to do —
no id, stack trace or provider text on screen; the correlation id sits behind "details".

## 5. Playwright tests (REQ id in each name)

- `test("REQ-058 the access ledger lists members, invitations and grants with reasons", ...)`.
- `test("REQ-058 a grant with a reason is created and revoked, and revocation states what is lost", ...)`.
- `test("REQ-058 a plain member sees the refusal sentence and no table rows", ...)`.
- `test("REQ-058 plugin vetting shows refused parts with reasons and records one decision without executing source", ...)`.
- `test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)`.
- `test("REQ-037 shared blocks render as data only with safe links and images", ...)`.
- `test("REQ-007 a consequential approval card names the action, consequence and authorizer and takes focus", ...)`.
- `test("REQ-017 approve and deny answer the card exactly once and leave an attributable confirmation", ...)`.
- `test("REQ-011 an activity row links back to its originating message", ...)`.
- `test("REQ-036 the artifact viewer shows the version chain, hash and owner without rendering HTML", ...)`.
- `test("REQ-057 Hebrew RTL mirrors the ledger and axe reports zero critical or serious findings", ...)`.

Goldens under `spec/golden/ui/` stay byte-exact; a new state means a new `REQ-<id>-<slug>.json`
asserting seeded state, never a click path.

## 6. Best first slice

Change (a), the access ledger — GUESS, recommended. All six `admin.*` RPCs are landed and
`admin.spec.ts` already carries three named REQ-058 tests, so it needs no new transport, component,
permission model or wire format. One screen: swap the raw `<table>`/`<input>` for BoardUI
`table`/`input`, route the two hardcoded headers through `t()`, add the reason/grantedBy/createdAt
columns, and add the revocation confirmation; keep every behaviour inside REQ-058.

**Decision needed from the owner**: confirm the sector framing. If an audit export is in scope
(REQ-044/045 per `docs/STATE.md`, no web client), assign its RPCs and REQ ids before adding it —
no invented test name, no externally visible deviation outside `docs/STATE.md` / `docs/PARITY.md`.
