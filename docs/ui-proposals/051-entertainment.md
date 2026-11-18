# UI proposal 051 — entertainment (self-hosted creative production and release)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #051 — entertainment.
Read-only on product code; this file is the only write. Grounding (CONFIRMED, read this session):
`docs/UI.md`, `spec/08-ui-ux.md`, `spec/01-requirements.md` (REQ ids), `apps/web/lib/api/rpc.ts` (RPC
names), and two components: `apps/web/components/thread/message-blocks.tsx` (card and disclosure
blocks, the `threadsAnswer` double-answer guard) and `apps/web/components/artifacts/artifact-viewer.tsx`
(bounded, untrusted-byte preview + version chain). No upstream brand, code, assets, wording or
reference capture consulted or copied.

Charter (CONFIRMED): Boma Bot only, fresh design (ADR-015), owner-supplied robot-cat as the sole
identity; no upstream brand/code/assets/copy; BoardUI semantic tokens only — no raw palette class
or hex literal, every class list merged with `cx()`; one design system, no second library or
hand-built lookalike; self-hosted on the studio's own machine — no SaaS/cloud, no build, cut,
concept, storyboard or audience datum leaving the building; product name and mark unchanged.
## 1. Persona and top jobs-to-be-done

**Persona — "the release producer"**: a producer/lead at a self-hosted game, animation or streaming
studio, working in English, French and Arabic across a review wall and a handheld. Boma Bot runs on
the studio's own network: unreleased builds, cuts, concept art, masters and schedules never leave the
building. She is measured on shipping the right thing on time, so approving a release, publish or
takedown against the wrong build or channel — or on a colour cue alone — is a real launch and
reputation loss. Ambiguity is worse than latency. The robot-cat mark is the only identity art; no
studio, platform, store or channel logo is borrowed.

Top jobs:
1. **Brief and attach** — hand a bot a build, cut or concept in a prompt without losing the draft to
   a stray Enter, and see what is new, unread or awaiting sign-off without scrolling a transcript.
2. **Green-light safely** — review a bot-proposed release, publish or takedown and be certain of the
   exact build, destination channel and effect before approving; never decide by colour alone.
3. **Track production** — watch render, encode, build and QA runs by name and status, retry only
   failures, pause/resume queued work, with a link back to the originating message.
4. **Move a title** — push work through backlog → in production → review → shipped on the board,
   keeping the history and attached runs visible there rather than in chat.
5. **Archive and showcase** — keep versioned, searchable assets and compiled press/showcase packages,
   open each version in the thread that produced it, and share a package with a distributor, festival
   or partner by link with no login and no leakage.
## 2. Five concrete UI changes

All new copy is a new `lib/i18n.ts` key via `useTranslation` / `t(locale, key)`; nothing hardcoded in
JSX. Every class list goes through `cx()`; every colour rides a `docs/UI.md` token. Bot- and asset-produced values are untrusted text/bytes, never HTML, per `artifact-viewer.tsx`.

**(a) Green-light status strip — thread `/app/:botId` header.** A compact, read-only band above the
transcript: named chips for "new assets", "renders queued" and "awaiting sign-off", each with a
`Tooltip`. Components: BoardUI `Chip`/`Badge` (`components/base/badges/chip.tsx`), `Tooltip`,
`IconButton` (icons from `@remixicon/react` passed as references). Tokens: surface
`bg-background-secondary-default`, hairline `border-separator-border`, text
`text-text-primary`/`text-text-secondary`, icon `text-foreground-icon-secondary`, type
`text-caption-1-semibold`, merged with `cx()`. RPCs (CONFIRMED): `botsGet`, `threadsHead`,
`threadsMessages`. Treat bot values as text; keep build numbers out of logs.

**(b) Release / showcase approval card — thread `/app/:botId` pending card (REQ-007/REQ-017).**
Grounded in `message-blocks.tsx` `CardBlock`: make the "what will happen" sentence dominant, with the
exact build/version, destination channel and effect, plus a distinct confirm label ("Release build
1.4.2 to Channel X", not just "Approve"). High-impact actions (public release, publish, takedown,
overwrite) pair `border-border-error-default` + `text-text-error-primary` with an icon and the words
"Release-critical" — never colour alone. Keep the `threadsAnswer` guard so a double answer is honest.
Components: BoardUI `Button`, `Badge`, `Tooltip`; card `bg-background-primary-default`, `rounded-3xl`,
`border-border-button-default`, focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`.
Transcript stays readable, other threads reachable, focus lands on the card; announce once via the
polite live region (spec §8.9), the transcript is not a live region. RPC: `threadsAnswer`.

**(c) Render-run triage — `/app` Activity view (REQ-011/REQ-012/REQ-064/REQ-067).** Follow
`ActivityScreen`: group render, encode, build and QA runs by day with prompt snippet, bot name, a
named status chip, duration and a link to the starting message. Working runs sort to the top and
update in place; failures get a plain cause and a retry, never a decorative spinner. Components:
installed `table`/`chip`, `Button`, `IconButton`, `Tooltip`; surfaces
`bg-background-primary-default`/`bg-background-secondary-default`, hairline
`border-separator-border`, status as text + icon using the exact §8.9 vocabulary. RPCs (CONFIRMED):
`runsList`, `runsRetry`, `runsPause`, `runsResume`, `notificationsStatus`, `notificationsSubscribe`.

**(d) Production ticket board — `/app/board` (REQ-038, feature-flagged).** Reuse `BoardScreen`
(`components/board/board-screen.tsx`): columns `backlog/todo/in_progress/done` read as backlog / in
production / review / shipped, with title key, assignee bot and last comment; assigning a ticket to
a bot queues work visible here, not in chat. Status is text + icon, never colour alone. Prefer
installed BoardUI `Input`/`Select` over the raw fields, merged with `cx()`; tokens
`bg-background-primary-default`, `border-border-button-default`, `text-text-primary`/`text-text-secondary`.
RPCs (CONFIRMED): `boardsGet`, `botsList`, `ticketsCreate`, `ticketsMove`, `ticketsComment`. A flag-off
404 keeps the route absent, not locked (spec §8.6).

**(e) Asset library and shared showcase — `/app/artifacts` and `/p/:slugToken` (REQ-036/REQ-037).**
Reuse `ArtifactsGrid` and the bounded viewer in `artifact-viewer.tsx`: name builds, cuts, masters and
packages by type, filter by type/bot, search by name, show the version chain and "open in thread".
Images render in a bounded frame, PDFs natively, other bytes as a download — never markup; keep the
`MAX_INLINE_BYTES` / `textPreview` guard. Prefer the installed BoardUI `Select` over the raw
`<select>` there. Components: `Input`, `Select`, `Chip`, `Button`; tokens `text-text-primary`/
`text-text-secondary`/`text-text-tertiary`, `bg-background-primary-default`,
`border-border-button-default`. RPCs (CONFIRMED): `artifactsListSpace`, `artifactsGet`,
`artifactsListVersions`, `artifactsRemove`. Share a compiled showcase with no session and no chrome
beyond a small robot-cat footer; unknown, revoked and expired tokens render one identical "not
available" screen — same words, shape and timing. Transport (CONFIRMED):
`GET /api/v1/pages/shared/{token}`. Keep the robot-cat empty state when nothing is filed.
## 3. Accessibility

WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; a keyboard-only path for
every primary flow (release decision, board move, retry/pause, version select, filters); visible
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target`; 360 px width;
`prefers-reduced-motion` honoured; no animation on streaming content. Entertainment specifics:
previews get meaningful alt/captions or a transcript, never autoplay, with keyboard-operable controls;
release-critical markers and run status are text + icon, never colour alone. The uniform unavailable
screen is announced in identical text — no leak through the accessibility tree. Error copy is one
sentence of what happened plus one of what to do; no build ids, hashes, stack traces or provider text
on screen (correlation id behind "details" only). Tables carry `<caption>`/`<th scope>`.
## 4. i18n and RTL

English is the source catalogue, fr/he the translations; a missing key falls back to English — never
a blank or the raw key. `DirectionProvider` (`@/components/foundations/direction/direction`) gets an
explicit BCP 47 locale; the boot script writes `lang` and `dir` together. RTL mirrors the board's
columns, the rail, the pipeline arrows and drawer motion via `useDirection()`; navigation glyphs flip.
Logical utilities throughout (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`).
Build numbers, version codes, store SKUs, timecodes, resolutions, codecs, URLs and dates stay LTR in
`<bdi>`/`dir="auto"`. Preserve physical coordinates for measured creative geometry — frames,
storyboards and waveforms are never mirrored. Formatting application data is separate from layout direction.
## 5. Playwright tests (REQ id in each name)

- `test("REQ-001 briefing a bot creates one optimistic message that is not duplicated", ...)`
- `test("REQ-007 the release card names build, channel, effect and consequence and takes focus without blocking reading", ...)`
- `test("REQ-017 release and deny answer the card exactly once with an attributable confirmation", ...)`
- `test("REQ-038 the production board shows text-and-icon columns and moves a ticket by keyboard", ...)`
- `test("REQ-038 the board route is absent, not locked, when the flag returns 404", ...)`
- `test("REQ-011 the activity view groups render and build runs by day and links to the source", ...)`
- `test("REQ-012 every production-run status renders a distinct named text-and-icon chip", ...)`
- `test("REQ-064 a release failure announces through the polite live region without a decorative spinner", ...)`
- `test("REQ-067 pause and resume announce their status through the polite live region", ...)`
- `test("REQ-036 the asset library filters by type and bot and shows the version chain", ...)`
- `test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)`
- `test("REQ-057 Hebrew RTL mirrors the board and axe reports zero critical or serious findings", ...)`

Behaviour without a REQ yet (status strip, release-critical marker, day grouping) must get an owner-assigned id before code — do not invent one in a test name. Goldens under `spec/golden/ui/` stay byte-exact.
## 6. Best first slice

**Change (b), the release / showcase approval card** — GUESS, recommended. Smallest, highest-value
step: one screen, no new transport (`threadsAnswer` exists), no new component (BoardUI
`Button`/`Badge`/`Tooltip` installed), no token re-tune; the guard already lives in `message-blocks.tsx`
`CardBlock`. It directly prevents a build, publish or takedown being confirmed on a colour cue or
against the wrong build. Test-first: write `REQ-017 release and deny answer the card exactly once
with an attributable confirmation`, extend `spec/golden/ui/REQ-007-pending-card.json`, then ship only
the dominant consequence line, the explicit "Release build 1.4.2 to Channel X" label and the
text+icon release-critical marker. Defer (a), (c), (d), (e) until the owner assigns missing REQ ids
and approves scope.

**Decision needed from the owner**: confirm the entertainment/studio framing, and confirm whether
REQ-038's feature-flagged board is in scope for this deployment before change (d) is scheduled.

Provenance: REQ ids verified against `spec/01-requirements.md`; RPC names read from
`apps/web/lib/api/rpc.ts`; component structure read from `message-blocks.tsx` and `artifact-viewer.tsx`;
design rules from `docs/UI.md` and `spec/08-ui-ux.md` — all CONFIRMED readings. The persona and its jobs
are a GUESS (proposal only — no entertainment sector exists in the spec). No product code was modified.
