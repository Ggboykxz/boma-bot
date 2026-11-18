# UI proposal 063 — training (learning and development)

Status: PROPOSAL (the sector lens and persona are GUESS — no "training" sector exists in `spec/`).
Sector specialist #063. Read-only on product code; this file is the only write.
Grounding (CONFIRMED, read this session): `docs/UI.md`, `spec/08-ui-ux.md`,
`spec/01-requirements.md` (REQ ids), `apps/web/lib/api/rpc.ts` (procedure names) and two product
components: `apps/web/components/artifacts/artifacts-grid.tsx` and
`apps/web/components/board/board-screen.tsx`. No upstream/OpenBot brand, code, wording, asset or
reference capture was consulted or copied.

Charter (CONFIRMED): Boma Bot only, fresh design (ADR-015); the owner-supplied robot-cat head
(`assets/brand/boma-bot-head.svg`) is the sole identity — keep it in empty states and the learner
footer. No upstream mark, copy or asset. BoardUI semantic tokens only — never a raw palette class or
hex literal — composite type utilities, and every class list merged with `cx()`; one design system,
no second library or hand-built lookalike. Self-hosted on the operator's own machine — no SaaS/cloud;
no learner record or training content leaves the building.

## 1. Persona and top jobs-to-be-done

**Persona — "the L&D lead"**: the training manager of a self-hosted Boma Bot space, working
desktop-first in English, French or Arabic. She owns what learners see and is measured on *release
correctness*: a draft exercise published to all learners, or a wrong version of a guide, is a real
harm. She is not a technologist; an ambiguous state is worse than a slow one. The robot-cat mark is
the only identity art — no vendor, platform or training-brand logo is borrowed.

Jobs:
1. **Build** — turn a topic outline into draft materials and keep versions straight.
2. **Review** — read a bot-proposed release and know exactly what reaches learners before approving.
3. **Publish** — hand a finished guide to learners who have no account, and revoke it cleanly.
4. **Track** — see which training runs are queued, working, waiting or finished and follow the one
   that needs her.
5. **Reuse** — curate a curriculum board where assigning a ticket to a bot queues work visibly.

## 2. Five concrete UI changes

All new copy is a key in `lib/i18n.ts` via `useTranslation()` / `t(locale, key)` — nothing hardcoded
in JSX. Every colour rides a `docs/UI.md` token; every class list goes through `cx()`.

**(a) Training-material library — `/app/artifacts` (REQ-036).**
Reuse `ArtifactsGrid`: name lesson plans, decks and quizzes by type with a type/bot filter and name
search; make the version chain explicit. Replace the raw `<input>`/`<select>` with installed BoardUI
`Input`, `Select`, `Chip`, `Tooltip`; surface `bg-background-primary-default`,
`border-border-button-default`, text `text-text-primary`/`text-text-secondary`/`text-text-tertiary`,
icons `text-foreground-icon-secondary`, focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`.
RPCs (CONFIRMED): `artifactsListSpace`, `artifactsListVersions`, `botsList`. Keep the robot-cat empty
state; artifact content is untrusted — render as text, never HTML.

**(b) Content-release approval card — thread `/app/:botId` (REQ-007, REQ-017).**
Make the "what will happen" sentence dominant, add a consequence summary, and give a distinct
confirm label ("Publish to Learners", not just "Approve"). High-impact releases pair
`border-border-error-default` + `text-text-error-primary` with a warning icon and the words
"High impact" — never colour alone. BoardUI `Button`, `Chip`, `Tooltip`; semantic-surface container
`bg-background-primary-default rounded-3xl border-border-button-default` (no lookalike component).
RPCs (CONFIRMED): `threadsAnswer`, `threadsMessages`. Announce once through the polite live region;
the transcript itself is not a live region (spec §8.9).

**(c) Curriculum board — `/app/board` (feature-flagged, REQ-038).**
From `BoardScreen`: columns by state (draft / review / approved / published, via `t()`), ticket cards
with key, title, assignee bot and last comment. BoardUI `Button`, `Chip`, `Input`; tokens
`bg-background-secondary-default`, `border-separator-border`, `text-text-primary`. Status is text +
icon, never colour. RPCs (CONFIRMED): `boardsGet`, `ticketsCreate`, `ticketsMove`, `ticketsComment`,
`botsList`. When the flag is off the route is absent, not locked.

**(d) Learner-facing guide — `/p/:slugToken` (REQ-037).**
Publish a compiled guide with no session and no product chrome beyond a small footer carrying the
robot-cat mark. Unknown, revoked and expired tokens render one identical "not available" document —
same words, shape and timing. BoardUI `Chip`, `Tooltip` for metadata; tokens `bg-background-full`,
`border-border-table`, `text-text-primary`. Transport (CONFIRMED):
`GET /api/v1/pages/shared/{token}` with `credentials: "omit"`; `noindex`/`no-referrer` and the page
CSP stay backend concerns. Never render artifact HTML in the app document.

**(e) Training activity — rail Activity view (REQ-011).**
A grouped-by-day list: prompt snippet, bot, status chip, duration and a link back to the originating
message; working runs sort to the top and update in place. BoardUI `Chip`, `IconButton`; tokens
`bg-background-primary-default`, `border-separator-border`, `text-text-secondary`, status
`text + icon` only. RPCs (CONFIRMED): `runsList`, `botsList`; background (ticket) runs appear here,
not in the chat. No routines/memory/skills web client exists yet — flagged below.

## 3. Accessibility

WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; a keyboard-only path for
every primary flow (filter, approve, move ticket, revoke); visible
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target`; 360 px width;
`prefers-reduced-motion` honoured. Focus lands on a pending card; the uniform unavailable document is
announced in identical text — no leak through the accessibility tree. Error copy is one sentence of
what happened plus one of what to do, with no learner id, version id, stack trace or provider text on
screen (correlation id behind "details", spec §8.9).

## 4. i18n and RTL

English is the source catalogue, fr/he translations; a missing key falls back to English — never a
blank or the raw key. `DirectionProvider` (`@/components/foundations/direction/direction`) takes an
explicit BCP 47 locale; the boot script writes `lang` and `dir` together. RTL mirrors the library,
board and activity list and flips arrows/drawer motion via `useDirection()`; logical utilities
throughout (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Codes, URLs, dates
and version numbers stay LTR in `<bdi>`/`dir="auto"`. Preserve measured geometry unchanged.

## 5. Playwright tests (REQ id in each name)

- `test("REQ-036 library filters materials by type and bot and shows the version chain", ...)`.
- `test("REQ-007 a release card names the exact effect, takes focus and does not block reading", ...)`.
- `test("REQ-017 publishing answers the card once and reopens the composer without duplicating", ...)`.
- `test("REQ-038 curriculum board moves a ticket by keyboard and hides when the flag is off", ...)`.
- `test("REQ-037 a learner guide renders without chrome and invalid tokens show one uniform screen", ...)`.
- `test("REQ-011 activity lists runs by named status with the right terminal action", ...)`.
- `test("REQ-009 unread divider moves after a training thread is read", ...)`.
- `test("REQ-012 every run status renders a distinct named chip with the right terminal action", ...)`.
- `test("REQ-057 Hebrew RTL mirrors the library and axe reports zero critical or serious findings", ...)`.

Behaviour without a REQ (release wording, high-impact marker) needs an owner-assigned id before code
— do not invent one in a test name. Goldens under `spec/golden/ui/` stay byte-exact.

## 6. Best first slice

**Change (b), the content-release approval card** — GUESS, recommended. One screen, no new transport
(`threadsAnswer` exists), no new component (installed `Button`/`Chip`/`Tooltip` suffice), and it
directly prevents a draft or wrong-version guide reaching every learner through an ambiguous confirm.
Test-first: write `REQ-007 a release card names the exact effect, takes focus and does not block
reading` and extend `spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant
consequence sentence, the explicit release label and the text+icon high-impact marker. Defer (a),
(c), (d), (e) until the owner approves scope.

**Decision needed from the owner**: confirm the L&D framing; and confirm whether a "save as skill /
remember for next time" teaching control (REQ-022 memory, REQ-035 skills) enters scope — those have no
web RPC client today, so an RPC and REQ id must be assigned before any such code.

Provenance: REQ ids and RPC names verified against `spec/01-requirements.md` and `apps/web/lib/api/rpc.ts`;
component structure read from `artifacts-grid.tsx` and `board-screen.tsx`; design rules from
`docs/UI.md` and `spec/08-ui-ux.md` — all CONFIRMED readings. The persona, its jobs and the slice
ranking are GUESS (proposal only — no training sector exists in the spec). No product code modified.
