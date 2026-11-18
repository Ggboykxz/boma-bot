# UI proposal 053 — film production (self-hosted post house and set)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #053 — film-production. Read-only on product code; this
file is the only write. Grounding (CONFIRMED, read this session): `docs/UI.md`, `spec/08-ui-ux.md`,
`spec/01-requirements.md` (REQ ids), `apps/web/lib/api/rpc.ts` (RPC names), and two components: `artifact-viewer.tsx`
(bounded, untrusted-byte viewer with version selector, "open in thread", confirm-delete) and `board-screen.tsx` (four-column
flag-gated board with a focus-trapped modal). No upstream brand, code, assets, wording or reference capture consulted.

Charter (CONFIRMED): Boma Bot only, fresh design (ADR-015), owner-supplied robot-cat as the sole identity; no upstream
brand/code/assets/copy; BoardUI semantic tokens only — no raw palette class or hex literal, every class list merged with
`cx()`; one design system, no second library or hand-built lookalike; self-hosted on the production's own machine — no
SaaS/cloud, no footage, cut, dailies, sound, subtitle or client deliverable leaving the building; product name and mark
unchanged.

## 1. Persona and top jobs-to-be-done

**Persona — "the post supervisor"**: a line producer / post-production supervisor at a self-hosted film, TV or commercial
house who moves material from camera cards through ingest, dailies, edit, picture lock and delivery. She works in English,
French and Arabic, between a grading-suite display and a phone on set. Boma Bot runs on the production's own storage: camera
originals, rushes, sound, VFX pulls, unreleased cuts and client deliverables never leave the premise. Approving a render,
transcode, conform or delivery against the wrong cut or version, or on a colour cue alone, is a real financial and
reputational loss — ambiguity is worse than latency. The robot-cat mark is the only identity art; no studio, distributor,
festival or camera-brand logo is borrowed.

Top jobs:
1. **Triage dailies** — hand footage, sound and VFX pulls to a bot in a prompt and see at a glance which takes are new, unread or awaiting review, without scrolling a transcript.
2. **Approve a delivery** — review a bot-proposed render, transcode, conform, subtitle burn-in or client delivery knowing the exact cut, version, destination and effect; never approve on colour alone.
3. **Track production runs** — name ingest, sync, transcode, transcription, render, grade and conform runs out loud with the §8.9 vocabulary, retry only failures, pause/resume queued work.
4. **Move a task through the board** — push a shot or sequence from development through pre-production, shoot, edit, picture lock and delivery, keeping attached runs and notes with it.
5. **Archive and share a cut** — keep versioned footage, dailies, cuts and masters, open each version in the thread that produced it, and share a reviewed cut or cover page by link with no login/leakage.

## 2. Five concrete UI changes

All new copy is a new `lib/i18n.ts` key via `useTranslation` / `t(locale, key)` (English source, fr/he translations; a
missing key falls back to English — never blank or the raw key). Every colour rides the `docs/UI.md` token map, type uses
composite utilities, and every class list goes through `cx()`. Footage- and bot-produced values are untrusted text/bytes,
never HTML, per `artifact-viewer.tsx`.

**(a) Dailies/status strip — thread `/app/:botId` header.** A compact, read-only band above the transcript: named chips for
"new takes", "unread rushes" and "awaiting review", plus a small volume trend using `chart-1`…`chart-5` tokens with
`-active` variants. Components: BoardUI `Chip`/`Badge` (`components/base/badges/chip.tsx`), `Tooltip`, `IconButton` (icons
from `@remixicon/react` passed as references). Tokens: surface `bg-background-secondary-default`, hairline
`border-separator-border`, text `text-text-primary`/`text-text-secondary`, icons `text-foreground-icon-secondary`, type
`text-caption-1-semibold`. Existing RPCs (CONFIRMED): `botsGet`, `threadsHead`, `threadsMessages`, `runsList`. Timecodes and
file hashes stay out of hover-only tooltips and logs; measured chart geometry stays LTR.

**(b) Delivery/approval card — thread `/app/:botId` pending card (REQ-007/REQ-017).** Make the "what will happen" sentence
dominant: exact cut name and version, destination (client, festival, broadcast), codec/deliverable and effect, with a
distinct confirm label ("Deliver CUT_07 v4 to the client link as ProRes 422", not just "Approve"). A high-impact action
(deliver, overwrite master, publish) pairs `border-border-error-default` + `text-text-error-primary` with a leading icon and
the words "High impact" — never colour alone. Components: BoardUI `Button`, `Badge`, `Tooltip`; card surface
`bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`, focus `focus-visible:ring-2
focus-visible:ring-border-focus-ring`. RPCs (CONFIRMED): `threadsAnswer` (keep the `CONFLICT` guard against a double
answer), `threadsReact`. Transcript stays readable, other threads reachable, focus lands on the card; the decision is
announced once via the polite live region (§8.9); the transcript is not a live region.

**(c) Production board — screen `/app/board` (REQ-038, feature-flagged).** Reuse `BoardScreen` directly: retitle the four
`COLUMNS` to development → pre-production → production → delivery, each card showing the shot/sequence key, title, assignee
bot and note count; the detail panel shows attached runs. The screen uses raw `<input>`, `<textarea>` and `<select>` for new
tickets — replace those with the installed BoardUI `Input`/`Select` (and a labelled textarea) merged with `cx()`; tokens
`bg-background-primary-default`, `border-separator-border`, `text-text-primary`/`text-text-secondary`, focus via the
focus-ring token. Keep the existing focus-trapped modal and keyboard path. RPCs (CONFIRMED): `boardsGet`, `botsList`,
`ticketsCreate`, `ticketsMove`, `ticketsComment`. A flag-off `404` keeps the route absent, not locked (§8.6); status is text
+ icon, never colour alone.

**(d) Production-run triage — `/app` Activity view (REQ-011/REQ-012/REQ-064/REQ-067).** Follow `ActivityScreen`: group
ingest, sync, transcode, transcription, render, grade and conform runs by day with prompt snippet, bot name, a named status
chip, duration and a link to the starting message. Working runs sort to the top and update in place; failures get a plain
cause and a retry, never a decorative spinner. Components: installed `table`/`chip`, `Button`, `IconButton`, `Tooltip`;
surfaces `bg-background-primary-default`/`bg-background-secondary-default`, hairline `border-separator-border`; status as
text + icon using the exact §8.9 vocabulary. RPCs (CONFIRMED): `runsList`, `runsRetry`, `runsPause`, `runsResume`,
`notificationsStatus`. Run/card arrivals use the `notifications.*` policy surface (REQ-064).

**(e) Footage library and shared review link — `/app/artifacts` and `/p/:slugToken` (REQ-036/REQ-037/REQ-039).** Extend
`ArtifactsGrid` and the bounded viewer in `artifact-viewer.tsx`: name camera originals, dailies, cuts, mains and
deliverables by type, filter by type/bot, search by name, show the version chain and "open in thread". Keep the viewer's
walls — images in a bounded frame, PDFs natively, other bytes download-only, bytes never injected as markup; replace its raw
`<select>` with the installed BoardUI `Select`. Components: `Input`, `Select`, `Chip`, `Button`; tokens
`text-text-primary`/`text-text-secondary`/`text-text-tertiary`, `bg-background-primary-default`,
`border-border-button-default`. RPCs (CONFIRMED): `artifactsListSpace`, `artifactsGet`, `artifactsListVersions`,
`artifactsRemove`. Reuse `SharedPageScreen` to publish a reviewed cut or cover page with no session and no chrome beyond a
small footer with the robot-cat mark; unknown, revoked and expired tokens render one identical "not available" screen — same
words, shape and timing. Transport (CONFIRMED): `GET /api/v1/pages/shared/{token}`; `noindex`/`no-referrer` and its own CSP
stay backend concerns (REQ-039). Keep the robot-cat empty state when nothing is filed.

## 3. Accessibility

WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; a keyboard-only path for every primary flow
(delivery decision, board moves, pause/resume, retry, version select); visible `focus-visible:ring-2
focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target` on grading-suite and on-set controls; 360 px width;
`prefers-reduced-motion` honoured; no animation on streaming content. Status and high-impact markers are text + icon, never
colour alone. Footage previews get meaningful alt/captions or a transcript, never autoplay, with keyboard-operable controls.
The uniform unavailable screen is announced in identical text — no leak through the accessibility tree; the board modal
traps and returns focus as today. Error copy is one sentence of what happened plus one of what to do — no timecode, hash,
client name, stack trace or provider text on screen (correlation id behind "details" only, §8.9). Tables carry `<caption>`
and `<th scope>`.

## 4. i18n and RTL

English is the source catalogue, fr/he the translations; a missing key falls back to English — never blank or the raw key.
`DirectionProvider` (`@/components/foundations/direction/direction`) gets an explicit BCP 47 locale; the boot script writes
`lang` and `dir` together. RTL mirrors the board's columns, the rail and the drawer motion via `useDirection()`; navigation
and disclosure arrows flip. Logical utilities throughout (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`,
`border-s`/`border-e`). Timecodes, frame rates (24/25/30), resolutions (1920x1080), codecs, file hashes, URLs, dates and
numbers stay LTR in `<bdi>`/`dir="auto"`. Preserve physical coordinates for measured media geometry — frames, waveforms and
thumbnails are never mirrored. Formatting application data is separate from layout direction.

## 5. Playwright tests (REQ id in each name)

- `test("REQ-001 the dailies composer sends one optimistic message that is not duplicated", ...)`
- `test("REQ-007 the delivery card names cut, version, destination and effect and takes focus", ...)`
- `test("REQ-017 approve and cancel answer the delivery card exactly once with an attributable confirmation", ...)`
- `test("REQ-011 the activity view groups ingest and render runs by day and links to the source", ...)`
- `test("REQ-012 every production-run status renders a distinct named text-and-icon chip", ...)`
- `test("REQ-021 a failed transcode shows a plain cause and a retry without a decorative spinner", ...)`
- `test("REQ-036 the footage library filters by type and bot and shows the version chain", ...)`
- `test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)`
- `test("REQ-038 the production board shows text-and-icon columns and moves a shot by keyboard", ...)`
- `test("REQ-057 Hebrew RTL mirrors the board and footer and axe reports zero critical or serious findings", ...)`
- `test("REQ-067 pause and resume a queued render announce their status through the polite live region", ...)`

Behaviour without a REQ yet (dailies strip, high-impact marker, footage captions) must get an owner-assigned id before code
— do not invent one in a test name. Goldens under `spec/golden/ui/` stay byte-exact; a new state means a new
`REQ-<id>-<slug>.json` asserting seeded state, not a click path.

## 6. Best first slice

**Change (b), the delivery/approval card** — GUESS, recommended. It is the smallest, highest-value step: one screen, no new
transport (`threadsAnswer` exists, with its `CONFLICT` guard), no new component (`Button`/`Badge`/`Tooltip` are installed)
and no token re-tune. It directly prevents a delivery, overwrite or publish being confirmed on a colour cue or against the
wrong cut/version. Test-first: write `REQ-017 approve and cancel answer the delivery card exactly once...`, extend
`spec/golden/ui/REQ-007-pending-card.json`, then ship only the dominant effect line, the explicit "Deliver <cut> <version>
to <destination>" label and the text+icon high-impact marker. Defer (a), (c), (d), (e) until the owner assigns any missing
REQ ids and approves scope.

**Decision needed from the owner**: confirm the film/post-house framing, and confirm whether REQ-038's feature-flagged board
and the shared-review path (REQ-039) are in scope for this deployment before changes (c)/(e) ship.

Provenance: RPC names and the `CONFLICT` guard CONFIRMED from `apps/web/lib/api/rpc.ts`; component structure
(`artifact-viewer.tsx` bounded frame, version selector, "open in thread", safe-byte walls, uniform error state;
`board-screen.tsx` `COLUMNS`, raw form fields, focus-trapped modal, flag-off `404`) CONFIRMED by reading the files this
session; REQ ids and golden names CONFIRMED from `spec/01-requirements.md`, `docs/UI.md` and `spec/golden/ui/`. The persona,
its jobs and the film-production mapping are GUESS (proposal only — no film-production sector or REQ exists in the spec). No
product code was modified.
