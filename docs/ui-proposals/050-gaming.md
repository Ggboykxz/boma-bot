# UI proposal 050 — gaming (self-hosted game studio and live-ops)

Status: PROPOSAL (GUESS where the spec is silent). Specialist #050 — gaming. Read-only on product
code; this file is the only write. Grounding (CONFIRMED, read this session):
`docs/UI.md`, `spec/08-ui-ux.md`, `spec/01-requirements.md` (REQ ids), `apps/web/lib/api/rpc.ts`
(procedure names), and two components: `apps/web/components/thread/computer-panel.tsx` (live screen
+ takeover panel) and `apps/web/components/thread/thread-composer.tsx` (composer with reply-chip
pattern). No upstream brand, code, assets, wording or reference capture consulted or copied.

Charter (CONFIRMED): Boma Bot only, fresh design (ADR-015), owner-supplied robot-cat
(`assets/brand/boma-bot-head.svg`) as sole identity; no upstream brand/code/assets/copy; BoardUI
semantic tokens only — no raw palette class or hex, every class list merged with `cx()`; one design
system, no second library or lookalike; self-hosted only — no SaaS/cloud, no build, replay,
telemetry or player datum leaving the building; product name and mark unchanged.

## 1. Persona and top jobs-to-be-done

**Persona — "the live-ops producer"**: a producer at a self-hosted game studio or esports org who
runs a fleet of specialist bots (build/test, QA, matchmaking-config, moderation, telemetry) from
one console, desktop-first at the ops desk and tablet on the show floor. Unexpanded builds, replays,
player data and unreleased configs are sensitive and the audit trail matters. She is measured on
player experience, so shipping a hotfix to the wrong environment or banning on a colour cue is a
real incident — ambiguity is worse than latency. The robot-cat mark is the only identity art; no studio,
publisher, platform or team logo is borrowed.

Top jobs:
1. **Direct a bot** — issue a build, test or QA command without losing it to a stray Enter, and attach a replay or build reference.
2. **Ship safely** — review a bot-proposed deploy, rollback, config change or ban, certain of the exact build, environment and effect; never approve by colour alone.
3. **Triage runs** — watch build, test, replay-processing and matchmaking runs by name and status,
   retry only failures, pause/resume queued work, with a link back to the starting message.
4. **Spectate and take over** — watch a bot's live session, take control of a stuck scrim, and
   release cleanly, with the holder's name always stated.
5. **Track and share** — keep work on the feature-flagged quest board rather than in chat, keep
   versioned replays/builds searchable, and publish a summary by link with no login and no leakage.

## 2. Five concrete UI changes

All new copy is a new `lib/i18n.ts` key via `useTranslation()` / `t(locale, key)`; nothing hardcoded
in JSX. Every class list goes through `cx()`; every colour rides a `docs/UI.md` token. Bot- and
telemetry-produced values are untrusted text/bytes, never HTML.

**(a) Build & telemetry status strip — thread `/app/:botId` header.**
A compact, read-only band above the transcript: named chips for "build queued", "test failures",
"hotfix pending" and "players affected", each with a `Tooltip` that expands detail on focus, not
hover alone. Components: `Chip`/`Badge` (`components/base/badges/chip.tsx`), `Tooltip`,
`IconButton`. Tokens: surface `bg-background-secondary-default`, hairline `border-separator-border`,
text `text-text-primary`/`text-text-secondary`, icon `text-foreground-icon-secondary`, type
`text-caption-1-semibold`, merged with `cx()`. Existing RPCs: `botsList`, `botsGet`, `threadsHead`,
`threadsMessages`.

**(b) Hotfix / live-ops approval card — thread `/app/:botId` pending card (REQ-007/REQ-017).**
Make the "what will happen" sentence dominant: exact build/version, target environment and effect,
plus a distinct confirm label ("Deploy build 1.4.2 to Live" or "Ban player <id>", not just
"Approve"). High-impact actions (public deploy, rollback, ban, config push) pair
`border-border-error-default` + `text-text-error-primary` with a warning icon and the words
"High impact" — never colour alone. Components: BoardUI `Button`, `Badge`, `Tooltip`; card surface
`bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`, focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`. Existing RPC: `threadsAnswer` (the
`CardBlock` guard handles a double answer). Transcript stays readable, other threads reachable,
focus lands on the card; announce once via the polite live region (spec §8.9), the transcript is
not a live region.

**(c) Replay-aware squad-command composer — `/app/:botId` (`thread-composer.tsx`, REQ-001).**
Extend `ThreadComposer` with an "attach replay/build" `IconButton` (`RiAttachment2Line`, via
`@remixicon/react`) plus `Tooltip`, adding a reference as a reply-style chip and reusing the
existing reply-banner pattern. Keep Enter-to-send / Shift+Enter-newline exactly as today, and keep
send disabled while `awaitingApproval`. Components: BoardUI `IconButton`, `Tooltip`; prefer the
installed `file-upload` component over a hand-built control; token-driven focus, 44 px `.boma-target`,
icon `text-foreground-icon-primary`. Existing RPC: `threadsSend`; the reference enters the same
`value` state, so no new transport. Match, replay and build ids stay LTR in `<bdi>`.

**(d) Build-run triage — `/app` Activity view (REQ-011/REQ-012/REQ-064/REQ-067).**
Follow `ActivityScreen`: group build, test, replay-processing and matchmaking runs by day with
prompt snippet, bot name, a named status chip, duration and a link to the starting message. Working
runs sort to the top and update in place; failures get a plain cause and a retry, never a decorative
spinner. Components: installed `table`/`chip`, `Button`, `IconButton`, `Tooltip`; surfaces
`bg-background-primary-default`/`bg-background-secondary-default`, hairline
`border-separator-border`, status as text + icon using the exact §8.9 vocabulary. Existing RPCs:
`runsList`, `runsRetry`, `runsPause`, `runsResume`, `notificationsStatus`, `notificationsSubscribe`,
`threadsHead`. The same named-status vocabulary carries to the feature-flagged quest board
(`/app/board`, REQ-038) via `boardsGet`, `ticketsCreate`, `ticketsMove`, `ticketsComment`.

**(e) Spectate & takeover panel — `/app/:botId` right panel (`computer-panel.tsx`,
REQ-026/REQ-028/REQ-029).**
Reuse the live frame + polling panel as a "spectate the scrim/QA session" surface: keep the named
status, the bounded frame, a `role="status"` live badge, and explicit Take over / Release buttons
that always name the holder, with no dismissal and no autoplay. Components: BoardUI `Button`,
`Badge`; tokens `bg-background-primary-default`, `bg-background-secondary-default`,
`border-border-button-default`, `text-text-error-primary` for a fault plus text + icon; 44 px targets.
Existing RPCs: `computerStatus`, `computerOpenScreen`, `computerOpenScreenFrame`, `computerTakeover`,
`computerRelease`. Keep physical frame coordinates unmirrored.

## 3. Accessibility

WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; a keyboard-only path
for every primary flow (deploy decision, attach replay, retry/pause, takeover/release, board move);
visible `focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target`; 360 px
width; `prefers-reduced-motion` honoured; no animation on streaming content or the live frame.
Status and high-impact markers are text + icon, never colour alone. Live frames/media get meaningful
alt or a transcript, never autoplay. The uniform unavailable screen (REQ-037) is announced in identical
text — no accessibility-tree leak. Error copy is one sentence of what happened plus one of what to do;
no build ids, player ids, stack traces or provider text on screen (correlation id behind "details").
Tables carry `<caption>`/`<th scope>`. Run vocabulary stays exactly
`queued | leased | running | waiting_input | waiting_takeover | completed | failed | cancelled`.

## 4. i18n and RTL

English is the source catalogue, fr/he the translations; a missing key falls back to English — never
a blank or raw key. `DirectionProvider` (`@/components/foundations/direction/direction`) gets an
explicit BCP 47 locale; the boot script writes `lang`/`dir` together. RTL mirrors the strip, rail,
board columns, takeover arrows and drawer motion via `useDirection()`; logical utilities throughout
(`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Build numbers, versions,
player/replay ids, scores, durations and URLs stay LTR in `<bdi>`/`dir="auto"`. Preserve physical
coordinates for the spectated frame; formatting data is separate from layout direction.

## 5. Playwright tests (REQ id in each name)

- `test("REQ-001 the replay-aware composer sends one optimistic message that is not duplicated", ...)`
- `test("REQ-007 the hotfix card names build, environment and effect and takes focus without blocking reading", ...)`
- `test("REQ-017 deploy and cancel answer the hotfix card exactly once with an attributable confirmation", ...)`
- `test("REQ-011 the activity view groups build and replay runs by day and links to the source", ...)`
- `test("REQ-012 every build-run status renders a distinct named text-and-icon chip", ...)`
- `test("REQ-064 a deploy failure announces through the polite live region without a decorative spinner", ...)`
- `test("REQ-067 pause and resume a queued test run in place with the right terminal action", ...)`
- `test("REQ-026 the spectate panel renders starting, running and stopped states with a bounded frame", ...)`
- `test("REQ-029 takeover and release name the holder and are keyboard-operable", ...)`
- `test("REQ-038 the quest board moves a ticket and shows its attached runs by status", ...)`
- `test("REQ-057 Hebrew RTL mirrors the rail and board, keeps build ids LTR, and axe reports zero critical or serious findings", ...)`

Behaviour without a REQ yet (status strip, attach chip, high-impact marker) needs an owner-assigned id
before code — never invent one. Goldens under `spec/golden/ui/` stay byte-exact.

## 6. Best first slice

**Change (b), the hotfix/live-ops approval card** — GUESS, recommended. Smallest, highest-value step:
one screen, no new transport (`threadsAnswer` exists), no new component (`Button`/`Badge`/`Tooltip`
installed), no token re-tune. It prevents a deploy, rollback or ban being confirmed on a colour cue or
against the wrong build. Test-first: write `REQ-017 deploy and cancel answer the hotfix card exactly
once with an attributable confirmation`, extend `spec/golden/ui/REQ-007-pending-card.json`, then ship
only the dominant effect line, the explicit "Deploy build <version> to <environment>" label and the
text+icon high-impact marker. Defer (a), (c), (d), (e) until REQ ids are assigned and scope approved.

**Decision needed**: confirm the gaming framing and whether the feature-flagged quest board (REQ-038)
is in scope before change (d) is scheduled.

Provenance: REQ ids from `spec/01-requirements.md`; RPC names from `apps/web/lib/api/rpc.ts`;
component structure from `computer-panel.tsx` and `thread-composer.tsx`; design rules from `docs/UI.md`
and `spec/08-ui-ux.md` — all CONFIRMED readings. Persona/jobs are a GUESS (no gaming sector in the
spec). No product code was modified.
