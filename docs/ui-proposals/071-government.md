# UI proposal 071 — government / public sector

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #071. Read-only on product code;
this file is the only write. Grounding (CONFIRMED, read this session): `docs/UI.md`, `spec/08-ui-ux.md`,
`spec/01-requirements.md` (REQ ids), `apps/web/lib/api/rpc.ts` (procedure names), and two components:
`apps/web/components/admin/access-screen.tsx` (members, invitations, reason-bearing grants, distinct
forbidden/error phases, `role="alert"`, all labels via `t()`) and
`apps/web/components/shared-page/shared-page-screen.tsx` (no-session public page, data blocks as
text/bytes, `safeLink`/`safeImage`, caps, one uniform unavailable document). No upstream/OpenBot brand,
code, asset, wording or reference capture consulted or copied.

Charter (CONFIRMED): Boma Bot only; fresh design (ADR-015); the owner-supplied robot-cat is the sole
identity art. BoardUI semantic tokens only — no raw palette class or hex literal; every class list
merged with `cx()`; dark mode rides the tokens; one design system, no second library or lookalike.
This is a **self-hosted** deployment inside an agency's own network — no hosted Boma service, no cloud
signup, no SaaS framing, no case data, credential or telemetry leaving the network. Name/mark unchanged.

## 1. Persona and top jobs-to-be-done

**Persona — "the records officer"**: a program officer in a municipality/agency, working in English,
French and Arabic across a desktop console and a phone on call. Bots help with public-records/FOIA
requests, permit reviews, notices and internal admin on agency-owned hardware. She is measured on
accountability, least privilege and correct disclosure, not speed; a wrong approval or an over-broad
share is a reportable incident, so ambiguity is worse than latency. The robot-cat mark is the only
identity art — no government seal, vendor or department logo is borrowed.

Top jobs:
1. **Brief and delegate** — hand a bot a case, record or redacted excerpt without losing the draft
   to a stray Enter, and see what is new, unread or awaiting sign-off without scrolling a transcript.
2. **Approve accountably** — know the exact action, amount, target *and requester* of a bot-proposed
   spend, data release or case decision before one click commits it; never decide by colour alone.
3. **Control access** — grant time-bound, reason-bearing access, revoke it at once, and keep a named
   trace of who granted what; a former member must fail closed at the next space RPC.
4. **Publish a notice** — share a transparency page by link with no session and no product chrome,
   rendering compiled data as text, never markup, with one identical unavailable document for every
   bad or revoked token.
5. **Trace and account** — find run activity by name and status with a link to the source message,
   read plain-language failures, and keep case data out of logs, tooltips and another tenant's view.

## 2. Five concrete UI changes

New copy is a key through `useTranslation()` → `t(locale, key)` (English source, fr/he translations,
missing key falls back to English — never blank or bare key); nothing hardcoded in JSX. Every class
list goes through `cx()`; every colour rides a `docs/UI.md` token. Bot- and citizen-supplied values
(case notes, names, emails, reasons, page blocks) are untrusted text/bytes, never HTML; no credential
or case datum reaches the DOM or a log.

**(a) Accountable access & grant register — `/admin/access` (REQ-042, REQ-058), grounded in
`access-screen.tsx`.** Keep the three sections (members, pending invitations, grants) and the reason +
`grantedBy` attribution on every grant, which is the accountable record. Give revoke an explicit
confirm label ("Revoke access for <email>") plus icon and text; keep the distinct `forbidden` phase
with `border-border-error-default` + `text-text-error-primary` **and** an icon and the word
"Forbidden", and the `role="alert"` error with retry. Use installed `table` (in a `"use client"`
module), `Button`, `Badge`/`Chip`, `Tooltip`, `Input`; surfaces `bg-background-full`,
`bg-background-secondary-default`, `border-separator-border`, `border-border-button-default`, text
`text-text-primary`/`text-text-secondary`/`text-text-tertiary`, focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`. RPCs (CONFIRMED): `adminMembersList`,
`adminInvitationsList`, `adminGrantsList`, `adminGrantsCreate`, `adminGrantsRevoke`,
`adminInvitationsRevoke`. A non-admin gets a refusal, not a disabled screen.

**(b) Accountable approval card — thread `/app/:botId` pending card (REQ-007, REQ-017, REQ-018).**
Make the "what will happen" sentence dominant: exact action, amount and target ("Release case 2026-114
to the requestor", "Spend 40 credits on OCR"). Add a named requester/why line and an explicit confirm
label, not "Approve". High-impact spend or disclosure pairs `border-border-error-default` +
`text-text-error-primary` with an icon and the word "High impact" — never colour alone. Keep the
`threadsAnswer` double-answer guard. BoardUI `Button`, `Badge`/`Chip`, `Tooltip`; card
`bg-background-primary-default rounded-3xl border-border-button-default`, type `text-body-medium`/
`text-caption-1-semibold`. The transcript stays readable, other threads reachable, focus lands on the
card; announce once through the polite live region (spec §8.9) — the transcript is not a live region.
RPCs (CONFIRMED): `threadsAnswer`, `threadsReact`.

**(c) Public notice / transparency page — `/p/:slugToken` (REQ-037, REQ-039), grounded in
`shared-page-screen.tsx`.** Keep: no session (`credentials: "omit"`), text/table/chart/link/image
blocks rendered as data with caps and `safeLink`/`safeImage`, `rel="noopener noreferrer nofollow"`,
and **one** "not available" document for 404, 410 and network error — same words, shape and timing;
add `noindex`/`no-referrer` and a small robot-cat footer, no navigation chrome. Tokens
`bg-background-full`, `border-border-table`, `border-separator-border`, `text-text-primary`/
`text-text-secondary`, chart `fill-accent-400`; links LTR in `<bdi>`. Transport (CONFIRMED): `GET /api/v1/pages/shared/{token}`. No upstream wording reused; no chrome.

**(d) Records activity & run trace — `/app` Activity view (REQ-011, REQ-012; REQ-045 is GUESS).**
Group runs by day: prompt snippet, bot name, named status chip, duration and a link to the source
message; working runs sort to the top and update in place; failures get one plain cause and a retry,
never a decorative spinner; skeletons while loading. Use the exact §8.9 vocabulary (`queued`,
`leased`, `running`, `waiting_input`, `waiting_takeover`, `completed`, `failed`, `cancelled`) as
text + icon. Installed `table`/`chip`, `Button`, `IconButton`, `Tooltip`; surfaces
`bg-background-primary-default`/`bg-background-secondary-default`, hairline
`border-separator-border`. RPCs (CONFIRMED): `runsList`, `runsRetry`, `runsPause`, `runsResume`,
`threadsHead`, `notificationsStatus`. **Gap (GUESS):** no audit-export RPC exists in `rpc.ts`; an audit viewer (REQ-045) needs owner confirmation and a REQ-tagged test before any UI — do not invent one.

**(e) Case triage in the rail — `/app` shell left rail (REQ-009, REQ-012).** Replace the unread
count's bare number with `Counter` + a named `Chip` ("Awaiting you", "Working", "Snoozed till 16:00"),
due detail behind a keyboard-focusable `Tooltip`; group the snoozed shelf by due time using the
installed `date-picker`/`date-calendar` if a picker is added. `animate-number-fade` for the count per
`docs/UI.md`; tokens `text-text-primary`, `text-text-secondary`, `bg-background-tertiary-default`.
RPCs (CONFIRMED): `threadsMarkRead`, `threadsMarkUnread`, `threadsSnooze`, `threadsUnsnooze`, `runsList`. Tenant isolation (REQ-043) stays server-side: the UI never shows another space's ids.

## 3. Accessibility

WCAG 2.2 AA: zero critical/serious axe findings in light, dark and RTL; keyboard-only path for every
primary flow (grant/revoke, approval, run retry/pause, snooze); visible token focus ring; ≥44 px
`.boma-target`; 360 px width; `prefers-reduced-motion` honoured; no animation on streaming content.
Approval impact, forbidden state and run status are text + icon, never colour alone; the unavailable
page is announced in identical text — no leak through the accessibility tree. Consequences are one
sentence of what happened plus one of what to do; no case id, stack trace, provider text or secret on
screen (correlation id behind "details" only). Tables carry `<caption>`/`<th scope="col">`.

## 4. i18n and RTL

English is the source catalogue, fr/he the translations; a missing key falls back to English — never
blank or raw key. `DirectionProvider` gets an explicit BCP 47 locale; the boot script writes `lang`
and `dir` together. RTL mirrors the rail, arrows and drawer motion via `useDirection()`; navigation
and disclosure glyphs flip. Logical utilities throughout (`ms`/`me`, `ps`/`pe`, `text-start`/
`text-end`, `border-s`/`border-e`). Case numbers, permit refs, emails, URLs and access codes stay LTR
in `<bdi>`/`dir="auto"`. Preserve physical coordinates for measured charts — never mirror them;
formatting application data is separate from layout direction.

## 5. Playwright tests (REQ id in each name)

- `test("REQ-042 an access grant records reason and grantor, and revoke names the email", ...)`
- `test("REQ-058 a non-admin receives a named forbidden refusal, never a disabled screen", ...)`
- `test("REQ-007 the approval card names action, amount and requester and takes focus without blocking reading", ...)`
- `test("REQ-017 a high-impact release is marked by text and icon and answers the card exactly once", ...)`
- `test("REQ-018 a spend request above the cap shows the refusal reason and is not paid before approval", ...)`
- `test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)`
- `test("REQ-011 the activity view groups runs by day and links each to its source message", ...)`
- `test("REQ-012 every run status renders a distinct named text-and-icon chip", ...)`
- `test("REQ-009 the rail snooze shelf shows named states and returns a case on time", ...)`
- `test("REQ-057 Hebrew RTL mirrors the rail and axe reports zero critical or serious findings", ...)`

Behaviour without a REQ yet needs an owner-assigned id before code — do not invent one in a test name.
Goldens under `spec/golden/ui/` stay byte-exact; a new seeded state gets a new `REQ-<id>-<slug>.json`.

## 6. Best first slice

**Change (b), the accountable approval card** — GUESS, recommended. One screen, no new transport
(`threadsAnswer` exists), no new component (`Button`/`Badge`/`Tooltip` are installed), no token
re-tune, and an existing golden (`spec/golden/ui/REQ-007-pending-card.json`) to extend. It removes the
risk of an irreversible spend or record release confirmed on a colour cue or against the wrong case.
Test-first: write `REQ-017 a high-impact release is marked by text and icon and answers the card
exactly once`, extend the REQ-007 golden, then ship only the dominant consequence line, the named
requester, the explicit confirm label and the text+icon high-impact marker. Defer (a), (c), (d), (e)
until the owner approves scope and confirms the REQ-045 audit RPC gap.

**Decision needed:** confirm the self-hosted public-sector framing and whether a REQ-045 audit-export
RPC exists before any audit UI. Provenance: REQ ids from `spec/01-requirements.md`, RPC names from
`apps/web/lib/api/rpc.ts`, component structure from `access-screen.tsx` and `shared-page-screen.tsx`,
design rules from `docs/UI.md` and `spec/08-ui-ux.md` — all CONFIRMED. Persona, jobs, changes and first
slice are a GUESS (proposal only — no government sector exists in the spec). No product code modified.
