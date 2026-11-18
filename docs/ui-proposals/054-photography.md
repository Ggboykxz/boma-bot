# UI proposal 054 — photography (studio capture, selection and client proofing)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #054 — photography.
Read-only on product code; this file is the only write. Grounding (CONFIRMED, read this session):
`docs/UI.md`, `spec/08-ui-ux.md`, `apps/web/lib/api/rpc.ts`, and two components:
`apps/web/components/thread/thread-composer.tsx` (attachment/composer pattern, Enter-to-send) and
`apps/web/components/artifacts/artifact-viewer.tsx` (bounded image frame `max-h-[60vh] max-w-full
object-contain`, version selector, untrusted-byte rule). REQ ids from `spec/01-requirements.md`;
RPC names from `rpc.ts`. No upstream brand, code, assets or wording consulted or copied.

Charter (CONFIRMED): Boma Bot only, fresh design (ADR-015), the owner-supplied robot-cat as the sole
identity; no upstream brand/code/assets/copy; BoardUI semantic tokens only — no raw palette class or
hex literal, every class list merged with `cx()`; one design system, no second library or hand-built
lookalike; self-hosted on the studio's own machine — no SaaS/cloud and no image, RAW file, EXIF/GPS
tag or client proof leaving the premises. The robot-cat mark is the only identity art — no camera,
lens or studio logo is borrowed.

## 1. Persona and top jobs-to-be-done

**Persona — "the studio's digital asset manager"**: a photo studio's DAM/operator at an independent
studio or in-house brand team, running Boma Bot on the studio's own machine. She ingests cards, culls
frames, oversees bot batch edits and delivers client proofs. She works in English, French and Arabic,
between a colour-managed desktop and a phone on set. RAW files, faces, EXIF GPS and client galleries
never leave the building; a wrong delivery, a wrong usage-rights grant or an approval on a colour cue
is an irreversible, real loss, so ambiguity is worse than latency. The robot-cat mark is the only
identity art; photographs are user content, never brand art.

Top jobs:
1. **Ingest and review** — watch imports land in the thread as image blocks in a bounded frame that
   is never wider than the panel, with real alt text, on poor studio networks.
2. **Select and cull** — move a frame/shoot from ingested through selected and edited to delivered,
   keeping the history and the bot runs attached to it.
3. **Authorise delivery** — review a bot-proposed delivery or licensing card and know the client, the
   exact image set, export settings and usage rights before approving; never decide by colour alone.
4. **Name run state** — state every import, batch-edit and export run out loud using the §8.9
   vocabulary; retry only failures, pause/resume queued work.
5. **Version and share** — keep RAW → edited versions as an artifact version chain, and publish a
   client proofing gallery by link with no login and no leakage of other shoots.

## 2. Five concrete UI changes

All copy is a new `lib/i18n.ts` key via `useTranslation()` → `t(locale, key)` (English source;
French/Hebrew translations; missing key falls back to English); no string in JSX. Every class list
goes through `cx()`; every colour rides a `docs/UI.md` token.

**(a) Shot-drop composer and bounded image blocks — thread `/app/:botId` (REQ-001/REQ-002/REQ-036).**
Extend `ThreadComposer` with an attachment `IconButton` (`RiImageLine`) and a named file `Chip`;
keep Enter-to-send / Shift+Enter-newline and send-disabled-on-empty (CONFIRMED in the component).
Render `Block type:"image"` in the same bounded frame as `artifact-viewer.tsx`
(`max-h-[60vh] max-w-full ... object-contain`, real `alt` from the name) — never wider than the
panel. BoardUI `TextArea`/`Button`/`IconButton`/`Chip`; surface `bg-background-primary-default`,
border `border-border-button-default`, icons `text-foreground-icon-primary`, focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`, 44 px `.boma-target`. RPCs (CONFIRMED):
`threadsSend`, `threadsMessages`, `artifactsGet`. Image bytes are untrusted: blob/data URL only,
never HTML; strip and never expose EXIF/GPS in the DOM, alt text or logs.

**(b) Delivery / licensing approval card — thread `/app/:botId` (REQ-007/REQ-017).**
Make the "what will happen" sentence dominant; name the client, the exact image set, export settings
and usage rights, and give confirm an explicit label ("Deliver to <client>", not "Approve").
Irreversible or rights-granting deliveries pair `border-border-error-default` +
`text-text-error-primary` with an icon and the word "High impact" — never colour alone. BoardUI
`Button`/`Badge`/`Tooltip`; card `bg-background-primary-default`, `rounded-3xl`,
`border-border-button-default`. RPCs (CONFIRMED): `threadsAnswer`, `threadsReact`. Transcript stays
readable, other threads reachable, focus lands on the card; announce once via the polite live
region (§8.9); send waits while the approval is pending (`awaitingApproval`).

**(c) Photo-production board — `/app/board` (REQ-038, feature-flagged).**
Reuse `BoardScreen`: columns ingested → selected → edited → delivered, with ticket key, title,
assignee bot and last comment; a detail panel shows the bot runs attached with named status chips.
Status is text + icon, never colour alone. Prefer installed BoardUI `Input`/`Select`/`Chip` over raw
fields, merged with `cx()`; tokens `bg-background-primary-default`, `border-border-button-default`,
`text-text-primary`/`text-text-secondary`. RPCs (CONFIRMED): `boardsGet`, `botsList`,
`ticketsCreate`, `ticketsMove`, `ticketsComment`. A 404 keeps the route absent, not locked (§8.6).

**(d) Batch-run triage — activity view (REQ-011/REQ-012/REQ-021).**
Follow `ActivityScreen`: group import, batch-edit and export runs by day, with named status chips
(queued, leased, running, waiting_input, waiting_takeover, completed, failed, cancelled) via text +
icon; retry only `failed`, pause/resume queued work. BoardUI `Button`/`Chip`; error row
`border-border-error-default` + `text-text-error-primary` + `text-foreground-icon-error`. RPCs
(CONFIRMED): `runsList`, `runsRetry`, `runsPause`, `runsResume`. Working runs sort to the top and
update in place; correlation id behind a "details" `Tooltip`, never stack traces or filenames.

**(e) Proofing library and shared client gallery — `/app/artifacts` + `/p/:slugToken`
(REQ-036/REQ-037).**
Reuse `ArtifactsGrid` + `ArtifactViewer`: name RAW and edited files by shoot, filter by type/bot,
search by name, show the version chain (RAW → edited) and "open in thread". Content is untrusted —
render as data, never HTML. BoardUI `Input`/`Select`/`Chip`/`Button`; tokens
`text-text-primary`/`text-text-secondary`/`text-text-tertiary`, `bg-background-primary-default`,
`border-border-button-default`. RPCs (CONFIRMED): `artifactsListSpace`, `artifactsGet`,
`artifactsListVersions`. Publish a proofing gallery with no session and no product chrome beyond a
small footer with the robot-cat mark; unknown, revoked and expired tokens render one identical
"not available" screen — same words, shape and timing. `noindex`/`no-referrer` and its own CSP stay
backend concerns; transport (CONFIRMED): `GET /api/v1/pages/shared/{token}`. Keep the robot-cat empty
state when nothing is filed.

## 3. Accessibility

WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; keyboard-only path for
every primary flow (attach, deliver, board moves, pause/resume, retry, filters); visible
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target`; 360 px width;
`prefers-reduced-motion` honoured; no animation on streaming content. Images carry meaningful `alt`
(the filename only when no better label), run status uses the exact §8.9 vocabulary, and delivery
impact is text + icon — no colour-only signal. Tables carry `<caption>` and `<th scope>`. Focus lands
on a pending card; the uniform unavailable screen is announced in identical text — no leak through
the accessibility tree. Error copy is one sentence of what happened plus one of what to do — no EXIF,
face, filename, client or provider text on screen (correlation id behind "details" only).

## 4. i18n and RTL

English is the source catalogue, fr/he the translations; a missing key falls back to English — never a
blank or the raw key. `DirectionProvider`
(`@/components/foundations/direction/direction`) gets an explicit BCP 47 locale; the boot script
writes `lang` and `dir` together. RTL mirrors the board's columns, the rail and the drawer motion via
`useDirection()`; arrows flip. Logical utilities throughout (`ms`/`me`, `ps`/`pe`,
`text-start`/`text-end`, `border-s`/`border-e`). Filenames, EXIF coordinates, dates, rights codes,
URLs and OTP digits stay LTR in `<bdi>`/`dir="auto"`; a photograph's measured geometry and bounded
frame are never mirrored. Formatting application data is separate from layout direction.

## 5. Playwright tests (REQ id in each name)

- `test("REQ-001 sending a shot creates one optimistic image message that is not duplicated", ...)`
- `test("REQ-036 an image block renders in a bounded frame with alt text and never as HTML", ...)`
- `test("REQ-007 the delivery card names client, image set, export, rights, takes focus without blocking", ...)`
- `test("REQ-017 approve and deny answer the delivery card exactly once with an attributable confirmation", ...)`
- `test("REQ-038 the production board shows text-and-icon columns and moves a shoot by keyboard", ...)`
- `test("REQ-038 the board route is absent, not locked, when the flag returns 404", ...)`
- `test("REQ-011 the activity view groups import, edit and export runs by day", ...)`
- `test("REQ-012 every run status renders a distinct named chip with the right terminal action", ...)`
- `test("REQ-021 a failed batch export shows a plain-language cause and a working retry", ...)`
- `test("REQ-036 the library filters by type and bot and shows the RAW-to-edited version chain", ...)`
- `test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)`
- `test("REQ-057 Hebrew RTL mirrors the board and axe reports zero critical or serious findings", ...)`

Goldens under `spec/golden/ui/` stay byte-exact; a new state means a new `REQ-<id>-<slug>.json`
asserting seeded state, never a click path. New behaviour without a REQ (shot strip, production
columns, rights marker) needs an owner-assigned id before code — do not invent one in a test name.

## 6. Best first slice

**Change (b), the delivery / licensing approval card** — GUESS, recommended. It is the smallest,
highest-value step: one screen, no new transport (`threadsAnswer`/`threadsReact` exist), no new
component (BoardUI `Button`/`Badge`/`Tooltip` are installed) and no token re-tune. It directly
prevents an irreversible delivery to the wrong client or a rights grant confirmed on a colour cue.
Test-first: write `REQ-007 ... names client, image set, export, rights and takes focus`, extend
`spec/golden/ui/REQ-007-pending-card.json`, then ship only the dominant consequence line, the
explicit "Deliver to <client>" label and the text+icon high-impact marker. Defer (a), (c), (d), (e)
until the owner assigns any missing REQ ids and approves scope.

**Decision needed from the owner**: confirm the sector framing and whether REQ-038's flag is in scope before (c).
