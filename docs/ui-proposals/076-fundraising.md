# UI proposal 076 — fundraising (development office)

Status: PROPOSAL. Sector framing, persona and jobs are GUESS (a sector lens, not in `spec/`);
file, component, token and RPC references are CONFIRMED (read this session). Product code is
read-only; this is the only file written. Grounding read this session: `docs/UI.md`,
`spec/08-ui-ux.md`, `apps/web/lib/api/rpc.ts` (procedure names), and two components —
`apps/web/components/thread/message-blocks.tsx` (`CardBlock`) and
`apps/web/components/board/board-screen.tsx`.

Charter (CONFIRMED, `docs/UI.md` + AGENTS.md): fresh design (ADR-015); the owner-supplied robot-cat
mark (`assets/brand/boma-bot-head.svg`) is the only identity — no upstream brand, code, wording or
asset; BoardUI semantic tokens only, no raw palette class or hex literal, classes merged with
`cx()`; one design system, no hand-built lookalike; self-hosted only — no SaaS/cloud, no donor,
gift or grant record leaving the operator's machine.

## 1. Persona and jobs

**Persona — "the development director"**: owner-operator of a small nonprofit development team who
runs a fleet of specialist bots (prospect research, grant drafting, pledge reminders,
acknowledgement drafts, campaign reporting) from one self-hosted console. Desktop-first at a desk,
phone between donor calls; donor PII, gift amounts and grant deadlines are sensitive and the audit
trail matters. An acknowledgement sent with the wrong amount, or a match pledge approved without a
named consequence, is a real trust, financial and compliance event — legible state beats a fast
screen. The robot-cat mark sits beside the wordmark on sign-in and empty states, never a payment,
CRM or charity-brand logo, never borrowed copy.

Top jobs:
1. **Steward** — keep donor records and gift amounts private on the machine, never in logs or tool output.
2. **Acknowledge safely** — review a proposed gift acknowledgement or match pledge and know the exact
   donor, fund, amount and effect before committing; never approve by colour alone.
3. **Triage** — find prospect-research and grant runs that are stuck, failed or waiting on her
   without scrolling a transcript.
4. **Report** — produce and verify a versioned campaign report or grant packet and open it in the
   originating thread.
5. **Audit** — keep a legible, attributable record of what each bot did, for the board and auditors.

## 2. Five concrete changes

All strings are new keys reached through `useTranslation()` / `t(locale, key)`
(`lib/use-translation.ts`, `lib/i18n.ts`); no hardcoded copy in JSX. All use BoardUI components
under `apps/web/components/` and the token map in `docs/UI.md`, merged with `cx()`. Bot- and
connector-produced content (donor notes, draft acknowledgements, grant text) is untrusted: render
as text, never HTML.

**(a) Campaign context strip — `/app/:botId` (thread header).** A compact, read-only band above the
transcript with named chips for "pledges due", "gifts to acknowledge" and "grants closing", each with
a `Tooltip` that reveals detail on focus, not hover alone. Components: `Chip`/`Badge`, `Tooltip`,
`IconButton` (`@remixicon/react` reference). Tokens: `bg-background-secondary-default`,
`border-separator-border`, `text-text-primary`/`text-text-secondary`,
`text-foreground-icon-secondary`, type `text-caption-1-semibold`. Existing RPCs: `botsGet`,
`threadsHead`, `threadsMessages`. Read as state, not a data dump — keep donor names/ids out of the
DOM where possible.

**(b) Gift & grant approval card — `/app/:botId` (`CardBlock`, `message-blocks.tsx`; REQ-007/017).**
Make the "what will happen" sentence dominant ("Send an acknowledgement for a 500 gift to the
General Fund") with an explicit consequence summary and a distinct confirm label ("Send
acknowledgement", not just "Approve"). High-value gifts and match pledges carry
`border-border-error-default` + `text-text-error-primary` **plus** a warning icon and the words
"High value" — never colour alone. Migrate the local `<button>`/`<input>` and inline SVG to the
installed BoardUI `Button` and `Input` (design rule: extend installed form components). Card
surface `bg-background-secondary-default`, `rounded-3xl`, `border-separator-border`, focus
`focus-visible:ring-border-focus-ring`. Existing RPC: `threadsAnswer`, already exactly-once; its
`CONFLICT` path stays honest. Announced once via the polite live region (spec §8.9); the transcript
is not a live region.

**(c) Campaign triage on Activity — `components/activity/activity-screen.tsx` (REQ-011/012/067).**
Group by named status, sort failing and waiting runs to the top, add a status-filter `Tabs`, keep
the existing pause/resume/retry actions. Components: `Tabs`, `Button`, `Badge`/chip (text + icon
status). Tokens: `bg-background-primary-default`, `border-border-button-default`,
`text-text-secondary`. RPCs: `runsList`, `runsPause`, `runsResume`, `runsRetry`. Gifts, dates and
grant deadlines formatted with `Intl`, LTR.

**(d) Donor pipeline on the board — `/app/board` (`board-screen.tsx`, REQ-038).** Reuse the four
columns as identified → qualified → solicited → pledged/closed, with cards showing key, title,
assignee bot and last comment; a named detail panel shows attached research/grant runs. Change:
migrate the raw `<input>`/`<select>`/`<textarea>` to the already installed BoardUI `Input`/`Select`
(design rule), keep the modal focus trap and Escape close. Components: `Button`, `Input`, `Select`,
`Badge`. Tokens: `text-text-primary`, `text-text-secondary`, `border-border-button-default`,
`bg-background-primary-default`, `text-start`. RPCs: `boardsGet`, `ticketsCreate`, `ticketsMove`,
`ticketsComment`.

**(e) Report & grant-packet shelf — `/app/artifacts` + notifications panel (REQ-036/064).** Show
versioned campaign reports and grant packets in the artifact grid/viewer with version selector and
"open in thread"; add a per-bot policy exposing only `waiting`/`completed` alerts with a daily cap —
no SaaS destination by default (self-host only). Components: `Select`, `Button`, `Switch`, `Badge`.
Tokens: `bg-background-secondary-default`, `border-separator-border`, `text-text-secondary`.
RPCs: `artifactsListSpace`, `artifactsGet`, `artifactsListVersions`, `notificationsPolicyGet`,
`notificationsPolicySet`. Content is untrusted: never inject artifact HTML into the document. A
compiled campaign page may reuse `SharedPageScreen` (REQ-037) with one uniform "not available"
screen for unknown, revoked and expired tokens.

## 3. Accessibility, i18n and RTL

- Every visible string goes through `t(locale, key)` / `useTranslation()` (English source; French/
  Hebrew translations; a missing key falls back to English, never a blank or the key). No string in JSX.
- `DirectionProvider` sets locale/direction; the boot script writes `lang`/`dir`. Use logical utilities
  (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`, `rounded-s`/`rounded-e`) and
  mirror the rail/drawer motion via `useDirection()`. Donor names, amounts, pledge/grant ids, dates,
  emails and URLs stay LTR in `<bdi>`/`dir="auto"`.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets via
  `.boma-target`; token-driven focus ring; full keyboard path (card decision, filter, board move,
  policy, report download); status by text + icon, not colour; 360 px width; `prefers-reduced-motion`.
- Money uses `Intl.NumberFormat` with an explicit currency and integer minor units, never a float;
  a high-value marker is icon + the word "High value", and the approval effect is announced once via
  the polite live region. Error copy is one sentence of what happened plus one of what to do — no
  donor PII, ids, stack traces or provider text on screen (correlation id behind "details", spec §8.9).
- Run-status vocabulary stays exactly `queued | leased | running | waiting_input | waiting_takeover |
  completed | failed | cancelled` (spec §8.9).

## 4. Playwright coverage (REQ id in the test name)

- `test("REQ-001 the campaign strip shows one optimistic message that is not duplicated", ...)`.
- `test("REQ-007 a pending gift card states its effect, takes focus, and does not block reading", ...)`.
- `test("REQ-011 campaign triage sorts failed and waiting grant runs to the top", ...)`.
- `test("REQ-012 every run status renders as text plus icon in light, dark and RTL", ...)`.
- `test("REQ-017 a high-value gift names donor, fund and amount and answers exactly once", ...)`.
- `test("REQ-036 the report viewer selects a version and opens the packet in thread", ...)`.
- `test("REQ-038 the donor pipeline moves a ticket and records a comment through the same RPCs", ...)`.
- `test("REQ-057 rtl mirrors the rail, keeps amounts LTR, and axe reports zero critical or serious findings", ...)`.
- `test("REQ-064 the desk policy enables only waiting and completed alerts with a daily cap", ...)`.
- New behaviour without a REQ yet (context strip, high-value marker) must be given a REQ id by the
  owner before implementation — do not invent one in a test name (traceability gate).
- Every golden under `spec/golden/ui/` stays byte-exact; a new seeded state means a new
  `REQ-<id>-<slug>.json` golden, never a click path.

## 5. Best first slice

**Change (b), the gift & grant approval card** — GUESS, recommended. It is the smallest,
highest-value step: one screen, no new transport (`threadsAnswer` already exists), no new component
(`Button`/`Badge`/`Input` are installed), and it directly reduces the risk of an unchecked
acknowledgement or match pledge reaching a donor. Test-first: write
`REQ-007 a pending gift card states its effect, takes focus, and does not block reading` and extend
`spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant consequence sentence, the
explicit "Send acknowledgement" label, the text+icon high-value marker, keyboard operability and the
polite announcement. Defer (a), (c), (d), (e) until the owner assigns REQ ids and approves.

**Decision needed from the owner**: confirm the sector framing and assign REQ ids for behaviour the
spec does not already cover; record any externally visible deviation in `docs/STATE.md` /
`docs/PARITY.md` before shipping.

Provenance: all file, component, token and RPC references marked CONFIRMED were read this session
from `docs/UI.md`, `spec/08-ui-ux.md`, `apps/web/lib/api/rpc.ts` and the two named components; the
persona and its jobs are a GUESS (no fundraising sector exists in the spec). No product code was
modified.
