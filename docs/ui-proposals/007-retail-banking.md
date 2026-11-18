# 007 — Retail-banking sector UI proposal

Status: PROPOSAL. Design framing and priorities are GUESS (a sector lens, not in `spec/`);
file, component, token and RPC references are CONFIRMED (read this session). No product code was
touched; this is the only file written.

Sources (CONFIRMED): `docs/UI.md`, `spec/08-ui-ux.md`,
`apps/web/components/shell/app-shell.tsx`, `apps/web/components/thread/thread-composer.tsx`,
`apps/web/lib/api/rpc.ts` (procedure names). Charter (CONFIRMED, `docs/UI.md` + AGENTS.md):
fresh design (ADR-015), robot-cat identity (`assets/brand/boma-bot-head.svg`), BoardUI semantic
tokens only with `cx()`, one design system, self-host only (no SaaS), and no upstream
brand/code/assets/wording.

## Persona and jobs

**Persona — "the duty manager"**: a retail-banking operations lead who supervises a fleet of
specialist bots (case intake, KYC, card disputes, reconciliation) from one console. Desktop-first
at a desk, tablet on the floor; regulated and audited. The robot-cat mark is the only identity:
it sits beside the wordmark on sign-in and in empty states — never a bank logo or a borrowed mark.

Jobs to be done:
1. Triage incoming customer cases and send each to the right specialist bot.
2. Authorise or refuse money-touching actions with a plain-language consequence and a durable record.
3. Watch unattended jobs (reconciliation, KYC sweeps) and pause, resume or reassign them.
4. Retrieve a customer artifact (statement, dispute evidence), check its version, share it in-thread.
5. Produce an evidence trail for compliance: who saw what, who approved, when.
6. Get told about fraud/SLA breaches without leaving the thread.

## Five UI changes

### 1. Case triage queue — activity view (`/app`)
**Screen**: the existing activity list becomes "Cases": grouped by day, prompt snippet, bot name,
status chip, duration, link back to the originating message. Filter by bot and status; working runs
sort to the top and update in place.
**BoardUI**: `select` (filters), `table` / `chip` / `badge` (rows), `icon-button`, `scroll-fade`
for the list, `skeleton` while loading.
**Tokens**: `bg-background-full` page, `bg-background-primary-default` rows,
`border-separator-border`, `text-text-primary` labels, `cx()`; status chips are text + icon, never
colour alone.
**RPCs (CONFIRMED)**: `runs.list`, `runs.pause`, `runs.resume`, `groups.reassign`.
**REQ**: 011, 012, 067, 068.

### 2. Money-action approval card — thread (`/app/:botId`)
**Screen**: a pending card names the action ("Change overdraft limit to X for customer Y") and
exactly what will happen, with Approve / Deny. The transcript stays readable and other threads
reachable; the composer is disabled only while a modal approval waits (spec §8.3), with an
`aria-live` waiting note. The answered state replaces controls with a check-mark confirmation.
**BoardUI**: `card`, `button`, `badge`, `textarea` (question cards), `agent-thinking` while pending.
**Tokens**: `text-text-error-primary` + `border-border-error-default` for refusal, `accent-*`
ramp for the primary action, `rounded-3xl`, `cx()`.
**RPCs (CONFIRMED)**: `threads.answer`, `threads.send`, `threads.subscribe`, `runs.retry`.
**REQ**: 007, 017, 021.

### 3. Evidence drawer — right panel + `/app/artifacts/:artifactId`
**Screen**: thread-scoped artifacts newest-first in the right panel; the viewer has a version-chain
selector, download, "open in thread" and copy. Images render in a bounded frame, PDF/text natively.
Artifact content is untrusted — never render its HTML in the app document. On mobile the panel is
a bottom sheet with three tabs.
**BoardUI**: `drawer`, `table`, `select`, `button`, `icon-button`, `skeleton`, `spinner`.
**Tokens**: `text-text-secondary` metadata, `border-border-table`,
`bg-background-secondary-default`, `cx()`.
**RPCs (CONFIRMED)**: `artifacts.listSpace`, `artifacts.get`, `artifacts.listVersions`,
`artifacts.remove`.
**REQ**: 036, 037.

### 4. Compliance and access review — `/admin/access`
**Screen**: members table (name, email, role, last seen), pending invitations, and access grants
with reasons. Role changes are optimistic with rollback; removal states what is lost. An "Export
evidence" action starts an audit export and shows its status without leaving the screen.
**BoardUI**: `table`, `badge`, `button`, `select`, `settings-modal`.
**Tokens**: `text-text-primary`, `border-border-table`, `text-text-error-primary` for destructive
rows, `cx()`.
**RPCs**: `admin.members.list`, `admin.grants.list`, `admin.grants.create`, `admin.grants.revoke`,
`admin.invitations.list`, `admin.invitations.revoke` (CONFIRMED in client); `export.start`,
`export.status`, `export.download` (per `docs/STATE.md`, REQ-044/045 — no web client yet).
**REQ**: 044, 045, 058.

### 5. Alert policy center — shell notifications panel
**Screen**: a header-triggered panel lists fraud/SLA alerts (text + icon, never colour alone) and
exposes Web Push policy: enable/disable, per-bot allow-list, destinations. Self-hosted VAPID only;
no third-party service and no outbound payload beyond `{v,kind,title,tag,url}`.
**BoardUI**: `settings-modal`, `switch`, `table`, `badge`, `button`, `tooltip`.
**Tokens**: `border-separator-border`, `text-text-secondary`, `accent-*` for the enabled state,
`cx()`.
**RPCs (CONFIRMED)**: `notifications.status`, `notifications.policyGet`, `notifications.policySet`,
`notifications.subscribe`, `notifications.unsubscribe`,
`notifications.destinations.list` / `.create` / `.delete`.
**REQ**: 064.

## Accessibility

WCAG 2.2 AA gate: zero critical/serious axe findings, a keyboard-only path for every primary flow,
visible `focus-visible:ring-2 focus-visible:ring-border-focus-ring`, ≥44 px targets (`.boma-target`),
360 px width, light and dark, and `prefers-reduced-motion`. No status, amount or severity is
conveyed by colour alone: chips and deltas always carry text + icon (spec §8.9). The run-status
vocabulary stays exactly `queued | leased | running | waiting_input | waiting_takeover |
completed | failed | cancelled`. Run changes, card arrivals and errors use a polite live region;
the transcript itself is not a live region. Focus lands on a pending card; a refused action is
announced, not merely coloured.

## i18n and RTL

Every string goes through `useTranslation` / `lib/i18n.ts` from day one — no hardcoded labels.
English is the source catalogue, fr/he the current translations, missing keys fall back to English
(`docs/UI.md`). Gap to close, not a new string path: REQ-057 asks for 14 locales while the client
ships 3 (issue #16). Locale drives `DirectionProvider` with an explicit BCP 47 locale and writes
`lang`/`dir` on `<html>`; RTL mirrors the rail to the right and flips arrows and drawer motion.
Use logical properties throughout (`ms`/`me`, `ps`/`pe`, `start`/`end`, `text-start`/`text-end`).
IBANs, case/account numbers, amounts, dates and OTP digits stay LTR via `<bdi>` / `dir="auto"`.
Money is formatted with `Intl.NumberFormat` and an explicit currency, never a float; a negative
value is shown by sign + icon, not red alone.

## Playwright coverage (REQ id in the test name)

- `test("REQ-011 triage queue filters by bot and status and working runs sort first", ...)`.
- `test("REQ-012 every run status renders a distinct named chip", ...)`.
- `test("REQ-067 pause and resume update the triage row in place", ...)`.
- `test("REQ-068 reassign hands the case to a sibling bot and logs the hand-off", ...)`.
- `test("REQ-007 a pending money-action card takes focus and does not block reading", ...)`.
- `test("REQ-017 approve and deny answer the card exactly once", ...)`.
- `test("REQ-036 the evidence drawer lists versions and opens a bounded viewer", ...)`.
- `test("REQ-058 an access grant with a reason is created and revoked", ...)`.
- `test("REQ-064 alert policy toggles Web Push and keeps URLs LTR in RTL", ...)`.
- `test("REQ-057 rtl mirrors the rail and axe reports zero critical or serious findings", ...)`.

Every golden under `spec/golden/ui/` stays byte-exact; a new screen state means a new
`REQ-<id>-<slug>.json` golden asserting seeded state, never a click path.

## Best first slice

Change 1 (case triage queue) — GUESS, recommended. It reuses already-landed RPCs (`runs.list`,
`runs.pause`, `runs.resume`, `groups.reassign`) and the existing activity route and golden pattern:
no new RPC, component, permission model or wire format. One screen, one `REQ-011` golden, and four
named tests (REQ-011/012/067/068). It proves the retail-banking lens end to end — a status you can
name out loud, a human decision, an evidence trail — before the more intricate approval-card and
alert-policy changes.

**Decision needed from the owner**: confirm the sector framing and assign REQ ids for any behaviour
the spec does not already cover. Until then keep every behaviour inside an existing REQ and record
any externally visible deviation in `docs/STATE.md` / `docs/PARITY.md`.
