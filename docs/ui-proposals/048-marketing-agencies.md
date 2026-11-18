# UI proposal 048 — marketing agencies (client services and campaigns)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #048 — marketing agencies.
Read-only on product code; this file is the only write. Grounding (CONFIRMED, read this session):
`docs/UI.md`, `spec/08-ui-ux.md`, `apps/web/components/board/board-screen.tsx`,
`apps/web/components/artifacts/artifacts-grid.tsx`, RPC names in `apps/web/lib/api/rpc.ts`, and
`GET /api/v1/pages/shared/:token` in `spec/03-api-contract.md`.
No upstream brand, code, assets or wording consulted or copied.

Charter (CONFIRMED): Boma Bot only, fresh design (ADR-015), the owner-supplied robot-cat as the sole
identity; no upstream brand/code/assets/copy; BoardUI semantic tokens only — no raw palette class or
hex literal, every class list merged with `cx()`; one design system, no second library or hand-built
lookalike; self-hosted on the agency's own network — no SaaS/cloud, no client brief, unreleased
creative, media plan, performance data or customer list leaving the premises.

## 1. Persona and top jobs-to-be-done

**Persona — "the client services lead"**: an account/operations lead at a self-hosted marketing agency
who runs a fleet of specialist bots (research, copy, creative, media, reporting) for several clients at
once. Desktop-first at an agency desk, tablet in client meetings, working in English, French and
Hebrew. Client briefs, unreleased creative, budgets and performance data are confidential, so Boma Bot
stays on the agency network. She is measured on client sign-off and on-time delivery: pitching a draft
to the wrong client, or approving on a colour cue alone, is worse than latency. The robot-cat mark is
the only identity art — no client, platform or agency logo is borrowed.

Top jobs:
1. **Brief and delegate** — turn a client brief into campaign tickets with a named owner and a status
   the whole team can name out loud, without hunting the rail.
2. **Approve deliverables** — review a bot-proposed copy deck, creative or media plan before it reaches
   a client; know client, campaign, consequence and deadline; never decide by colour alone.
3. **Track campaigns** — see the live picture of research/creative/media runs, name state with the
   exact §8.9 vocabulary, retry only failures, pause/resume queued work.
4. **Report** — keep versioned performance and activity reports, then publish a compiled client-facing
   page by link with no login and no cross-client leakage.
5. **Serve** — answer client questions with a durable, attributable record and hand work between bots
   (strategy → creative → media) without losing attribution.

## 2. Five concrete UI changes

All copy is a new `lib/i18n.ts` key via `useTranslation` / `t(locale, key)`; no string in JSX. Every
class list goes through `cx()`; every colour rides a `docs/UI.md` token. Bot- and client-produced
content is untrusted — render as text/data, never HTML; keep client names and identifiers out of logs.

**(a) Client/campaign context strip — thread `/app/:botId` (header).**
A compact, read-only band above the transcript: named chips for client, campaign, open deliverables and
due approvals, plus a small trend using the `chart-1`…`chart-5` tokens with `-active` variants and a
`Tooltip` (`components/base/tooltip/tooltip.tsx`) that expands on focus, not hover alone. BoardUI
`Chip`/`Badge` (`components/base/badges/chip.tsx`), `IconButton`
(`components/base/buttons/icon-button.tsx`); surface `bg-background-secondary-default`, hairline
`border-separator-border`, text `text-text-secondary`/`text-text-primary`, icons
`text-foreground-icon-secondary`, type `text-caption-1-semibold`. RPCs (CONFIRMED): `botsGet`,
`threadsHead`, `threadsMessages`. Measured performance data stays LTR.

**(b) Deliverable / client-approval card — thread `/app/:botId` (pending card, REQ-007/REQ-017).**
Make the "what will happen" sentence dominant ("Send draft creative to Client X") with an explicit
consequence summary and a distinct confirm label ("Send to client", not just "Approve"). Add client,
campaign and deadline. Client-facing actions pair `border-border-error-default` +
`text-text-error-primary` with a warning icon and the words "Client-facing" — never colour alone.
BoardUI `Button`, `Badge`, `Tooltip`; card `bg-background-primary-default`, `rounded-3xl`,
`border-border-button-default`. RPCs (CONFIRMED): `threadsAnswer`, `threadsReact`. The card is modal
for the effect only — the transcript stays readable, other threads reachable, focus lands on the card;
announce once via the polite live region (spec §8.9), the transcript is not a live region.

**(c) Campaign task board — `/app/board` (REQ-038, feature-flagged).**
Reuse `BoardScreen` (`components/board/board-screen.tsx`): columns `backlog/todo/in_progress/done`
read as brief / planning / in production / delivered, with ticket key, title, assignee bot and last
comment; assigning a ticket to a bot queues work visible here, not in chat. Prefer installed BoardUI
`Input`/`Select` over the raw fields in `board-screen.tsx`, merged with `cx()`; tokens
`bg-background-primary-default`, `border-border-button-default`, `text-text-primary`/
`text-text-secondary`, focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`. RPCs
(CONFIRMED): `boardsGet`, `botsList`, `ticketsCreate`, `ticketsMove`, `ticketsComment`. A 404 keeps the
route absent, not locked (spec §8.6).

**(d) Deliverable library — `/app/artifacts` and `/app/artifacts/:artifactId` (REQ-036).**
Reuse `ArtifactsGrid` (`components/artifacts/artifacts-grid.tsx`): name copy decks, creative files,
media plans and reports by type, filter by type/bot, search by name, show the version chain and "open
in thread"; the viewer adds a version selector and download. Content is untrusted — render as data,
never HTML in the document. Installed BoardUI `Input`, `Select`, `Chip`, `Button`; tokens
`text-text-primary`/`text-text-secondary`/`text-text-tertiary`, `bg-background-primary-default`,
`border-border-button-default`. RPCs (CONFIRMED): `artifactsListSpace`, `artifactsGet`,
`artifactsListVersions`. Keep the robot-cat empty state when nothing is filed.

**(e) Bot handover and shared client report — `/app/g/:groupId`, `/p/:slugToken` (REQ-010/REQ-067/REQ-068, REQ-037).**
Add a dismissal-free handover banner naming outgoing and incoming bot (strategy → creative → media)
with a named status chip, reusing the existing reassign-row pattern with BoardUI `Button`, `Badge`,
`Avatar` (`components/base/avatar/avatar.tsx`); tokens `bg-background-secondary-default`,
`border-separator-border`, `text-text-secondary`, and `border-border-error-default` +
`text-text-error-primary` + text + icon on a failed handover. Every message keeps its bot attribution.
RPCs (CONFIRMED): `groupsGet`, `groupsMessages`, `groupsReassign` (`runsPause`/`runsResume` if a run
must pause). Publish a compiled client report with no session and no chrome beyond a small footer with
the robot-cat mark; unknown, revoked and expired tokens render one identical "not available" screen —
same words, shape and timing. Transport (CONFIRMED): `GET /api/v1/pages/shared/:token`; `noindex`/
`no-referrer` and its own CSP stay backend concerns.

## 3. Accessibility

WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; keyboard-only path for
every primary flow (approve/deny, board moves, filters, snooze, handoff); visible
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target`; 360 px width;
`prefers-reduced-motion` honoured; no animation on streaming content. Run status and client-facing
severity are text + icon using the exact §8.9 vocabulary — no colour-only send or failure cues.
Tables carry `<caption>` and `<th scope>`. Focus lands on a pending card; the uniform unavailable
screen is announced in identical text — no leak through the accessibility tree. Error copy is one
sentence of what happened plus one of what to do — no client names, ids, stack traces or provider text
on screen (correlation id behind "details" only).

## 4. i18n and RTL

English is the source catalogue, fr/he the translations; a missing key falls back to English — never a
blank or the raw key. `DirectionProvider` (`@/components/foundations/direction/direction`) gets an
explicit BCP 47 locale; the boot script writes `lang` and `dir` together. RTL mirrors the board's
columns, the rail and the drawer motion via `useDirection()`; arrows flip. Logical utilities throughout
(`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`, `rounded-s`/`rounded-e`). Client
names, URLs, dates, budgets, media values and OTP digits stay LTR in `<bdi>`/`dir="auto"`; measured
geometry and performance charts are never mirrored. Formatting application data is separate from
layout direction.

## 5. Playwright tests (REQ id in each name)

- `test("REQ-001 sending a campaign instruction creates one optimistic message that is not duplicated", ...)`
- `test("REQ-007 the deliverable card names client, campaign, consequence and deadline, takes focus, and does not block reading", ...)`
- `test("REQ-017 approve and deny answer the client-facing card exactly once with an attributable confirmation", ...)`
- `test("REQ-038 the campaign board shows text-and-icon status columns and moves a ticket by keyboard", ...)`
- `test("REQ-038 the board route is absent, not locked, when the flag returns 404", ...)`
- `test("REQ-010 the group thread attributes each handover message and reassigns by keyboard alone", ...)`
- `test("REQ-036 the deliverable library filters by type and bot and shows the version chain", ...)`
- `test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)`
- `test("REQ-009 the unread divider moves after a campaign thread is read", ...)`
- `test("REQ-057 Hebrew RTL mirrors the board and axe reports zero critical or serious findings", ...)`

Goldens under `spec/golden/ui/` stay byte-exact; a new state means a new `REQ-<id>-<slug>.json`
asserting seeded state, never a click path. New behaviour without a REQ (context strip, client-facing
marker, handover banner) must get an owner-assigned id before code — do not invent one in a test name.

## 6. Best first slice

**Change (b), the deliverable / client-approval card** — GUESS, recommended. It is the smallest,
highest-value step: one screen, no new transport (`threadsAnswer`/`threadsReact` exist), no new
component (BoardUI `Button`/`Badge`/`Tooltip` are installed) and no token re-tune. It directly prevents
a wrong draft reaching a client on a colour cue alone. Test-first: write `REQ-007 ... names client,
campaign, consequence and deadline, takes focus`, extend
`spec/golden/ui/REQ-007-pending-card.json`, then ship only the dominant consequence sentence, the
explicit "Send to client" label and the text+icon client-facing marker. Defer (a), (c), (d), (e) until
the owner assigns any missing REQ ids and approves scope.

**Decision needed from the owner**: confirm the sector framing, and confirm whether REQ-038's feature
flag is in scope for this deployment before change (c) is scheduled; record any externally visible
deviation in `docs/STATE.md` / `docs/PARITY.md` before shipping.
