# UI proposal 047 — advertising (campaign operations and creative approval)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #047 — advertising.
Read-only on product code; this file is the only write. Grounding (CONFIRMED, read this session):
`docs/UI.md`, `spec/08-ui-ux.md`, `apps/web/components/thread/thread-screen.tsx`,
`apps/web/components/artifacts/artifacts-grid.tsx`, RPC names in `apps/web/lib/api/rpc.ts`, and
`GET /api/v1/pages/shared/:token` in `spec/03-api-contract.md`.
No upstream brand, code, assets or wording consulted or copied.

Charter (CONFIRMED): Boma Bot only, fresh design (ADR-015), the owner-supplied robot-cat as the sole
identity; no upstream brand/code/assets/copy; BoardUI semantic tokens only — no raw palette class or
hex literal, every class list merged with `cx()`; one design system, no second library or hand-built
lookalike, no embedded ad-platform widget CSS; self-hosted on the operator's own ad-operations
network — no SaaS/cloud, no campaign, budget, audience or client-creative data leaving the premises.

## 1. Persona and top jobs-to-be-done

**Persona — "the campaign operations lead"**: an agency or in-house media manager running paid search,
social and display campaigns. She works a desktop planning wall and a phone between client meetings,
in English, French and Arabic. Boma Bot is installed on the operator's own network: media plans,
spend caps, audience definitions, creative files and client names never leave it. She is measured on
budget accuracy and on-time launch — approving a spend cap against the wrong campaign, or approving on
a green/red colour cue alone, is worse than a slow screen. The robot-cat mark is the only identity
art; no ad network, agency or client logo is borrowed.

Top jobs:
1. **See the live picture** — one band of campaign status, phase, pacing and delivery alerts on the
   thread, without hunting the rail.
2. **Approve spend and launch** — review a bot-proposed budget change, flight date or creative
   sign-off and know campaign, amount, consequence and deadline before confirming; never by colour.
3. **Track campaign work** — move briefs from brief through production and review to live, keeping the
   approval history traceable to a person and a run.
4. **Name run state** — state every launch/delivery run out loud using the §8.9 vocabulary; retry only
   failures, pause/resume queued work.
5. **File and share** — keep creative versions, media plans and delivery reports versioned, and
   publish a compiled client report by link with no login and no leaked internal figures.

## 2. Five concrete UI changes

All copy is a new `lib/i18n.ts` key via `useTranslation` / `t(locale, key)`; no string in JSX. Every
class list goes through `cx()`; every colour rides a `docs/UI.md` token. Bot-produced values are
untrusted — render as text, never HTML; keep client names and budget ids out of logs.

**(a) Campaign pulse strip — thread `/app/:botId` (header).** A compact, read-only band above the
transcript: named chips for campaign status, flight phase, pacing and delivery alerts, with a small
pacing trend using `chart-1`…`chart-5` tokens and `-active` variants. BoardUI `Chip`/`Badge`
(`components/base/badges/chip.tsx`), `Tooltip`, `IconButton`; surface
`bg-background-secondary-default`, hairline `border-separator-border`, text
`text-text-secondary`/`text-text-primary`, icons `text-foreground-icon-secondary`, type
`text-caption-1-semibold`. RPCs (CONFIRMED): `botsGet`, `threadsHead`, `threadsMessages`. Measured
figures, currency codes and dates stay LTR.

**(b) Spend / launch approval card — thread `/app/:botId` (pending card, REQ-007/REQ-017).** Make the
"what will happen" sentence dominant; add campaign, amount, flight date and a distinct confirm label
("Raise cap for <campaign> to <amount>", not just "Approve"). High-impact spend actions pair
`border-border-error-default` + `text-text-error-primary` with an icon and the word
"Budget-critical" — never colour alone. BoardUI `Button`, `Badge`, `Tooltip`; card panel
`bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`. RPCs (CONFIRMED):
`threadsAnswer`, `threadsReact`. Transcript stays readable, other threads reachable, focus lands on
the card; announce once via the polite live region (§8.9), the transcript is not a live region.

**(c) Campaign pipeline board — `/app/board` (REQ-038, feature-flagged).** Reuse `BoardScreen`
(`components/board/board-screen.tsx`): columns `backlog/todo/in_progress/done` read as brief /
production / review / live, with brief key, title, assignee bot and last comment; assigning a brief
to a bot queues production work visible here, not in chat. Status is text + icon. Prefer installed
BoardUI `Input`/`Select` over raw fields, merged with `cx()`; tokens
`bg-background-primary-default`, `border-border-button-default`,
`text-text-primary`/`text-text-secondary`, focus ring from `border-border-focus-ring`.
RPCs (CONFIRMED): `boardsGet`, `botsList`, `ticketsCreate`, `ticketsMove`, `ticketsComment`.
A 404 keeps the route absent, not locked (§8.6).

**(d) Delivery-run triage — activity view (REQ-011/REQ-012/REQ-067).** Follow `ActivityScreen`
(`components/activity/activity-screen.tsx`): group launch and delivery runs by day, with named status
chips (queued, leased, running, waiting_input, waiting_takeover, completed, failed, cancelled) via
`runStatusKey` text + icon; pause/resume queued work, retry only `failed`. BoardUI `Button`, `Chip`;
`border-border-button-default`, `bg-background-primary-default`,
`text-text-primary`/`text-text-secondary`, and `border-border-error-default` +
`text-text-error-primary` + `text-foreground-icon-error` on the error row. RPCs (CONFIRMED):
`runsList`, `runsRetry`, `runsPause`, `runsResume`. Working runs sort to the top, update in place.

**(e) Creative library and client report — `/app/artifacts` and `/p/:slugToken` (REQ-036/REQ-037).**
Reuse `ArtifactsGrid` (`components/artifacts/artifacts-grid.tsx`): name brand assets, size plans,
media kits and reports by type, filter by type/bot, search by name, show the version chain and "open
in thread". Content is untrusted — render as data, never HTML in the document. BoardUI `Input`,
`Select`, `Chip`, `Button`; tokens `text-text-primary`/`text-text-secondary`/`text-text-tertiary`,
`bg-background-primary-default`, `border-border-button-default`. RPCs (CONFIRMED):
`artifactsListSpace`, `artifactsGet`, `artifactsListVersions`. Publish a compiled client report with
no session and no chrome beyond a small footer carrying the robot-cat mark; unknown, revoked and
expired tokens render one identical "not available" screen — same words, shape and timing.
Transport (CONFIRMED): `GET /api/v1/pages/shared/:token`. `noindex`/`no-referrer` and its own CSP
stay backend concerns. Keep the robot-cat empty state when nothing is filed.

## 3. Accessibility

WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; keyboard-only path for
every primary flow (approve, board moves, pause/resume, retry, filters); visible
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target` for wall-display
and touch use; 360 px width; `prefers-reduced-motion` honoured; no animation on streaming content.
Pacing, run status and budget severity are text + icon using the exact §8.9 vocabulary — no
colour-only spend or delivery alarm. Tables carry `<caption>` and `<th scope>`. Focus lands on a
pending card; the uniform unavailable screen is announced in identical text — no leak through the
accessibility tree. Error copy is one sentence of what happened plus one of what to do — no client
names, campaign ids, stack traces or provider text on screen (correlation id behind "details" only).

## 4. i18n and RTL

English is the source catalogue, fr/he the translations; a missing key falls back to English — never
a blank or the raw key. `DirectionProvider`
(`@/components/foundations/direction/direction`) gets an explicit BCP 47 locale; the boot script
writes `lang` and `dir` together. RTL mirrors the board's columns, the rail and the drawer motion via
`useDirection()`; arrows flip. Logical utilities throughout (`ms`/`me`, `ps`/`pe`,
`text-start`/`text-end`, `border-s`/`border-e`). Amounts, currency codes, campaign ids, dates,
URLs and OTP digits stay LTR in `<bdi>`/`dir="auto"`; measured geometry and pacing charts are never
mirrored. Formatting application data is separate from layout direction.

## 5. Playwright tests (REQ id in each name)

- `test("REQ-001 sending a campaign instruction creates one optimistic message that is not duplicated", ...)`
- `test("REQ-007 the spend card names campaign, amount, flight date, consequence and takes focus without blocking reading", ...)`
- `test("REQ-017 approve and deny answer the spend card exactly once with an attributable confirmation", ...)`
- `test("REQ-038 the campaign board shows text-and-icon columns and moves a brief by keyboard", ...)`
- `test("REQ-038 the board route is absent, not locked, when the flag returns 404", ...)`
- `test("REQ-011 the activity view replaces the list and groups delivery runs by day", ...)`
- `test("REQ-012 every run status renders a distinct named chip with the right terminal action", ...)`
- `test("REQ-067 pause and resume announce their status through the polite live region", ...)`
- `test("REQ-036 the creative library filters by type and bot and shows the version chain", ...)`
- `test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)`
- `test("REQ-009 the unread divider moves after a campaign thread is read", ...)`
- `test("REQ-057 Hebrew RTL mirrors the board and axe reports zero critical or serious findings", ...)`

Goldens under `spec/golden/ui/` stay byte-exact; a new state means a new `REQ-<id>-<slug>.json`
asserting seeded state, never a click path. New behaviour without a REQ (pulse strip, budget-critical
marker, day grouping) must get an owner-assigned id before code — do not invent one in a test name.

## 6. Best first slice

**Change (b), the spend / launch approval card** — GUESS, recommended. It is the smallest,
highest-value step: one screen, no new transport (`threadsAnswer`/`threadsReact` exist), no new
component (BoardUI `Button`/`Badge`/`Tooltip` are installed) and no token re-tune. It directly
prevents a budget change being approved on a colour cue or against the wrong campaign. Test-first:
write `REQ-007 ... names campaign, amount, flight date, consequence and takes focus`, extend
`spec/golden/ui/REQ-007-pending-card.json`, then ship only the dominant consequence line, the
explicit "Raise cap for <campaign> to <amount>" label and the text+icon budget-critical marker.
Defer (a), (c), (d), (e) until the owner assigns any missing REQ ids and approves scope.

**Decision needed from the owner**: confirm the sector framing, and confirm whether the board feature
flag is in scope for this deployment before change (c) is scheduled.
