# UI proposal 025 — manufacturing (discrete and batch plant operations)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #025. Read-only on product code;
only this file was written.
Grounding: `docs/UI.md`, `spec/08-ui-ux.md`, `spec/01-requirements.md`, and
`apps/web/components/{board/board-screen,activity/activity-screen}.tsx`; RPC names checked against
`apps/web/lib/api/rpc.ts`.
Charter: Boma Bot only; no upstream brand, code, wording or assets; BoardUI semantic tokens only
(never raw palette/hex), classes merged with `cx()`; one design system; self-hosted on the plant's own
machine — no SaaS/cloud, no recipe or quality data leaving the building; keep the robot-cat identity.

## 1. Persona and top jobs-to-be-done

**Persona — Karim, production lead in a 60-person discrete/batch plant.** He runs a multilingual shift
(French, English, Arabic), a shared workstation at the line, and gloves half the time. Boma Bot is
installed on the plant server: routings, BOMs, machine parameters and quality records never leave the
building. He is not a technologist; a wrong click that releases a quality hold or changes a machine
setting is a safety and scrap event, so ambiguity is worse than latency.

Top jobs:
1. **Plan** — at shift start, see one compact band of line, current order and open quality holds without hunting through the rail.
2. **Dispatch** — create a work order, assign it to a bot/operator, and see queued work progress on the board, not in chat.
3. **Approve** — review a bot-proposed material release, machine-setting change or hold release and be certain exactly what will happen before approving; never decide by colour alone.
4. **Monitor** — across the shift, find working/paused/failed runs, retry only failures, and pause or resume queued work; no decorative spinners.
5. **Hand over and trace** — pass an open line to the next shift with an attributable record, keep work instructions and quality certificates versioned, and share a production summary with an auditor/supplier without giving them a login.

## 2. Five concrete changes

All strings are new keys in `lib/i18n.ts` through `t(locale, key)`; no hardcoded copy. All use installed
BoardUI components under `apps/web/components/` and tokens from `docs/UI.md`.

**(a) Line-status context strip — screen `/app/:botId` (thread header).**
A compact, read-only band above the transcript: chips for "line", "current order", "open quality holds",
each with a `Tooltip` for detail. BoardUI `Chip`/`Badge`, `Tooltip`, `IconButton`
(`components/base/badges/chip.tsx`, `components/base/buttons/icon-button.tsx`); surface
`bg-background-secondary-default`, hairline `border-separator-border`, text
`text-text-secondary`/`text-text-primary`, icons `text-foreground-icon-secondary`, type
`text-caption-1-semibold`. RPCs already present: `botsGet`, `threadsHead`, `threadsMessages`.
Bot-produced values are untrusted — render as text, never HTML; keep machine codes out of logs.

**(b) Work-order approval card — screen `/app/:botId` (pending card, REQ-007/REQ-017).**
Make the "what will happen" sentence dominant; add an explicit consequence summary and a distinct
confirm label ("Release material lot", not just "Approve"). High-impact actions (machine-setting
change, quality-hold release) use `border-border-error-default` + `text-text-error-primary` **plus**
an icon and the word "High impact" — never colour alone. BoardUI `Button`, `Badge`, `Tooltip`;
card `bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`. RPCs:
`threadsAnswer`, `threadsReact`. Announce once through the polite live region (spec §8.9); the
transcript is not a live region.

**(c) Work-order board — screen `/app/board` (feature-flagged, REQ-038).**
Reuse `BoardScreen`: columns backlog / todo / in progress / done carry work-order key, title,
assignee bot and last comment; creating a work order assigned to a bot queues work visible here, not
in chat. Status is text + icon via `Chip` (`components/base/badges/chip.tsx`), never colour alone.
Prefer the installed BoardUI `Input`/`Select` over the raw fields currently in `board-screen.tsx`,
merged with `cx()`; tokens `bg-background-primary-default`, `border-border-button-default`,
`text-text-primary`/`text-text-secondary`, focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`.
RPCs: `boardsGet`, `botsList`, `ticketsCreate`, `ticketsMove`, `ticketsComment`. When the flag is
off the route is absent, not locked.

**(d) Production-run triage — screen activity view (REQ-011/REQ-012/REQ-067).**
Extend `ActivityScreen` with shift/day grouping and named status chips (`queued`, `leased`, `running`,
`waiting_input`, `waiting_takeover`, `completed`, `failed`, `cancelled`) using `runStatusKey` text +
icon; pause/resume queued runs, retry only `failed` (`runsRetry`). BoardUI `Button`, `Chip`; tokens
`border-border-button-default`, `bg-background-primary-default`, `text-text-primary`/`text-text-secondary`,
`border-border-error-default` + `text-text-error-primary` + `text-foreground-icon-error` on the error
row (as in `activity-screen.tsx`). RPCs: `runsList`, `runsPause`, `runsResume`, `runsRetry`.

**(e) Work-instruction library and shared summary — screens `/app/artifacts` and `/p/:slugToken`
(REQ-036/REQ-037).**
Reuse `ArtifactsGrid`: name routings, BOMs, work instructions and quality certificates by type,
filter by type/bot, search by name, and show the version chain clearly. BoardUI `Input`, `Select`,
`Chip`; tokens `text-text-primary`/`text-text-secondary`/`text-text-tertiary`,
`bg-background-primary-default`, `border-border-button-default`. RPCs: `artifactsListSpace`,
`artifactsListVersions`, `artifactsGet`, `botsList`. Publish a compiled production summary to an
auditor/supplier with no session and no product chrome beyond a small footer with the robot-cat mark;
unknown or revoked tokens render one identical "not available" screen, same wording and timing;
`noindex`, `no-referrer`, its own CSP. Endpoint: `GET /api/v1/pages/shared/{token}`. Keep the robot-cat
empty state when nothing is filed. Never render artifact HTML in the app document.

## 3. Accessibility, i18n and RTL

- New labels go in `lib/i18n.ts` (English source; French/Hebrew translations; a missing key falls back
  to English, never a blank or the key). No user-visible string in JSX.
- `DirectionProvider` sets locale/direction; the boot script writes `lang`/`dir`. Use logical utilities
  (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Mirror the board's columns,
  arrow glyphs and drawer motion via `useDirection()`; keep work-order keys, machine codes, BOM part
  numbers, dates and URLs LTR in `<bdi>`/`dir="auto"`.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets via
  `.boma-target` for gloved use; visible token-driven focus; full keyboard path (approval, board moves,
  pause/resume, filters); status by text + icon, not colour (no colour-only machine alarm); 360 px
  width; `prefers-reduced-motion` honoured.
- Shop-floor error copy is one sentence of what happened plus one of what to do — no order ids,
  machine ids, stack traces or provider text on screen (correlation id behind "details" only, spec §8.9).

## 4. Playwright tests (REQ ids where they exist)

- `REQ-001 send creates one optimistic message that is not duplicated`
- `REQ-007 work-order approval card names the exact action, takes focus, and does not block reading`
- `REQ-017 answering the card reopens the composer without duplicating the message`
- `REQ-038 work-order board moves a ticket by keyboard alone and hides when the flag is off`
- `REQ-011 activity view replaces the list and groups runs by shift`
- `REQ-012 every run status renders a distinct named chip with the right terminal action`
- `REQ-067 pause and resume announce their status through the polite live region`
- `REQ-036 work-instruction library filters by type and bot and shows the version chain`
- `REQ-037 shared production summary renders without chrome; invalid token shows the uniform screen`
- `REQ-009 unread divider moves after a production thread is read`
- `REQ-057 Hebrew RTL loads dir=rtl with a clean axe run`
- New behaviour above without a REQ yet (line-status strip, high-impact marker, shift grouping) must
  be given a REQ id by the owner before implementation, per the traceability gate — do not invent one
  in test names.

## 5. Highest-impact first slice

**Change (b), the work-order approval card.** Smallest, highest-value step: one screen, no new
transport (`threadsAnswer` exists), no new component (BoardUI `Button`/`Badge`/`Tooltip` are
installed), and it directly prevents an ambiguous confirmation from releasing a quality hold or
changing a machine setting on a live line. Test-first: write `REQ-007 ... names the exact action` and
extend `spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant consequence sentence,
the explicit confirm label, and the text+icon high-impact marker. Defer (a), (c), (d), (e) until the
owner assigns REQ ids and approves.
