# UI proposal 045 — media (self-hosted newsroom and content operations)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #045 — media.
Read-only on product code; this file is the only write. Grounding (CONFIRMED, read this session):
`docs/UI.md`, `spec/08-ui-ux.md`, `spec/01-requirements.md` (REQ ids), `apps/web/lib/api/rpc.ts`
(RPC names), and two components: `apps/web/components/artifacts/artifact-viewer.tsx` (bounded,
untrusted-byte media viewer) and `apps/web/components/thread/thread-composer.tsx` (composer with the
reply-chip pattern). No upstream brand, code, assets, wording or reference capture consulted or copied.

Charter (CONFIRMED): Boma Bot only, fresh design (ADR-015), owner-supplied robot-cat as the sole
identity; no upstream brand/code/assets/copy; BoardUI semantic tokens only — no raw palette class or
hex literal, every class list merged with `cx()`; one design system, no second library or hand-built
lookalike; self-hosted on the operator's own machine — no SaaS/cloud, no asset, story, transcript or
audience datum leaving the building; product name and mark unchanged.

## 1. Persona and top jobs-to-be-done

**Persona — "the media operations editor"**: a producer/editor at a self-hosted newsroom, studio or
publisher, working across English, French and Arabic on a shared newsroom display and a handheld.
Boma Bot runs on the organisation's own network: raw footage, audio, graphics, drafts and unpublished
stories never leave the building. She is measured on accuracy and timing, so approving a publish,
render or takedown against the wrong asset or on a colour cue is a real reputational loss — ambiguity
is worse than latency. The robot-cat mark is the only identity art; no outlet, network or channel logo
is borrowed.

Top jobs:
1. **Attach and triage** — hand a media asset to a bot in a prompt without losing the draft to a stray
   Enter, and see which assets are new, unread or awaiting review without scrolling a transcript.
2. **Verify before publish** — review a bot-proposed publish, render, transcode or takedown and be
   certain of the exact asset, destination and effect before approving; never approve by colour alone.
3. **Track production** — watch transcode, transcription, render and publish runs by name and status,
   retry only failures, pause/resume queued work, with a link back to the originating message.
4. **Move a story** — push content through draft → review → approved → published, keeping the history
   and the attached runs, and keep work visible on the board rather than in chat.
5. **Archive and share** — keep versioned, searchable media and compiled packages, open each version
   in the thread that produced it, and publish a package to an editor, contributor or partner by link
   with no login and no leakage.

## 2. Five concrete UI changes

All new copy is a new `lib/i18n.ts` key via `useTranslation` / `t(locale, key)`; nothing hardcoded in
JSX. Every class list goes through `cx()`; every colour rides a `docs/UI.md` token. Bot- and asset-produced
values are untrusted text/bytes, never HTML, per `artifact-viewer.tsx`.

**(a) Ingest and review status strip — thread `/app/:botId` header.**
A compact, read-only band above the transcript: named chips for "new assets", "unread cuts" and "awaiting
review", each with a `Tooltip`. Components: `Badge`/`Chip` (`components/base/badges/chip.tsx`), `Tooltip`,
`IconButton` (icons from `@remixicon/react` passed as references). Tokens: surface
`bg-background-secondary-default`, hairline `border-separator-border`, text
`text-text-primary`/`text-text-secondary`, icon `text-foreground-icon-secondary`, type
`text-caption-1-semibold`, merged with `cx()`. Existing RPCs: `botsGet`, `threadsHead`, `threadsMessages`.

**(b) Publish/render approval card — thread `/app/:botId` pending card (REQ-007/REQ-017).**
Make the "what will happen" sentence dominant: exact asset name, destination/channel and effect, plus a
distinct confirm label ("Publish Story — Morning Brief", not just "Approve"). High-impact actions
(public publish, takedown, overwrite) pair `border-border-error-default` + `text-text-error-primary`
with an icon and the words "High impact" — never colour alone. Components: BoardUI `Button`, `Badge`,
`Tooltip`; card `bg-background-primary-default`, `rounded-3xl`, `border-border-button-default`, focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`. RPC: `threadsAnswer` (already present; the
`CardBlock` guard handles a double answer). Transcript stays readable, other threads reachable, focus
lands on the card; announce once via the polite live region (spec §8.9), the transcript is not a live region.

**(c) Media-attach composer — thread `/app/:botId` composer (`thread-composer.tsx`, REQ-001).**
Extend the composer with a named "attach asset" `IconButton` (`RiAttachment2Line`) that adds a reference
as a reply-style chip, reusing the existing reply-banner pattern. Keep Enter-to-send / Shift+Enter-newline
exactly as today and keep send blocked while an approval is pending. Component: BoardUI `IconButton` +
`Tooltip`; token-driven focus, 44 px `.boma-target`, icon `text-foreground-icon-primary`. RPC
`threadsSend`; the reference enters the same `value` state, so no new transport. File ids, hashes and
timecodes stay LTR in `<bdi>`. Prefer the installed `file-upload` component over a hand-built control.

**(d) Production-run triage — `/app` Activity view (REQ-011/REQ-012/REQ-064/REQ-067).**
Follow `ActivityScreen`: group transcode, transcription, render and publish runs by day with prompt
snippet, bot name, a named status chip, duration and a link to the starting message. Working runs sort
to the top and update in place; failures get a plain cause and a retry, never a decorative spinner.
Components: installed `table`/`chip`, `Button`, `IconButton`, `Tooltip`; surfaces
`bg-background-primary-default`/`bg-background-secondary-default`, hairline `border-separator-border`,
status as text + icon using the exact §8.9 vocabulary. RPCs: `runsList`, `runsRetry`, `runsPause`,
`runsResume`, `notificationsStatus`, `notificationsSubscribe`, `threadsHead`.

**(e) Media library and shared package — `/app/artifacts` and `/p/:slugToken` (REQ-036/REQ-037).**
Reuse `ArtifactsGrid` and the bounded viewer in `artifact-viewer.tsx`: name assets and packages by type,
filter by type/bot, search by name, show the version chain and "open in thread". Images render in a
bounded frame, PDFs natively, other bytes as a download — never markup. Prefer the installed BoardUI
`Select` over the raw `<select>` there. Components: `Input`, `Select`, `Chip`, `Button`; tokens
`text-text-primary`/`text-text-secondary`/`text-text-tertiary`, `bg-background-primary-default`,
`border-border-button-default`. RPCs: `artifactsListSpace`, `artifactsGet`, `artifactsListVersions`,
`artifactsRemove`. Publish a compiled package with no session and no chrome beyond a small footer with
the robot-cat mark; unknown, revoked and expired tokens render one identical "not available" screen —
same words, shape and timing. Transport (CONFIRMED): `GET /api/v1/pages/shared/{token}`.

## 3. Accessibility

WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; a keyboard-only path for
every primary flow (publish decision, attach, board move, retry/pause, version select); visible
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target`; 360 px width;
`prefers-reduced-motion` honoured; no animation on streaming content. Media-specific: images and video
get meaningful alt/captions or a transcript, never autoplay, and keyboard-operable controls; status and
high-impact markers are text + icon, never colour alone. The uniform unavailable screen is announced in
identical text — no leak through the accessibility tree. Error copy is one sentence of what happened
plus one of what to do; no asset ids, hashes, stack traces or provider text on screen (correlation id
behind "details" only). Tables carry `<caption>` and `<th scope>`.

## 4. i18n and RTL

English is the source catalogue, fr/he the translations; a missing key falls back to English — never a
blank or the raw key. `DirectionProvider` (`@/components/foundations/direction/direction`) gets an
explicit BCP 47 locale; the boot script writes `lang` and `dir` together. RTL mirrors the board's
columns, the rail, the pipeline arrows and the drawer motion via `useDirection()`; navigation glyphs
flip. Logical utilities throughout (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`).
Asset ids, hashes, timecodes, resolutions (1920x1080), codecs, URLs, dates and numbers stay LTR in
`<bdi>`/`dir="auto"`. Preserve physical coordinates for measured media geometry — video frames,
waveforms and thumbnails are never mirrored. Formatting application data is separate from layout direction.

## 5. Playwright tests (REQ id in each name)

- `test("REQ-001 the media-attach composer sends one optimistic message that is not duplicated", ...)`
- `test("REQ-007 the publish card names asset, destination and effect and takes focus without blocking reading", ...)`
- `test("REQ-017 publish and cancel answer the card exactly once with an attributable confirmation", ...)`
- `test("REQ-011 the activity view groups transcode and publish runs by day and links to the source", ...)`
- `test("REQ-012 every production-run status renders a distinct named text-and-icon chip", ...)`
- `test("REQ-064 a publish failure announces through the polite live region without a decorative spinner", ...)`
- `test("REQ-067 pause and resume a queued render in place with the right terminal action", ...)`
- `test("REQ-036 the media library filters by type and bot and shows the version chain", ...)`
- `test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)`
- `test("REQ-057 Hebrew RTL mirrors the board and the composer and axe reports zero critical or serious findings", ...)`

Behaviour without a REQ yet (status strip, attach chip, high-impact marker) must get an owner-assigned
id before code — do not invent one in a test name. Goldens under `spec/golden/ui/` stay byte-exact.

## 6. Best first slice

**Change (b), the publish/render approval card** — GUESS, recommended. It is the smallest, highest-value
step: one screen, no new transport (`threadsAnswer` exists), no new component (`Button`/`Badge`/`Tooltip`
are installed) and no token re-tune. It directly prevents a publish, takedown or overwrite being confirmed
on a colour cue or against the wrong asset. Test-first: write
`REQ-017 publish and cancel answer the card exactly once with an attributable confirmation`, extend
`spec/golden/ui/REQ-007-pending-card.json`, then ship only the dominant effect line, the explicit
"Publish Story — <asset>" label and the text+icon high-impact marker. Defer (a), (c), (d), (e) until the
owner assigns any missing REQ ids and approves scope.

**Decision needed from the owner**: confirm the media/newsroom framing, and confirm whether REQ-038's
feature-flagged board is in scope for this deployment before change (d)/(e) is scheduled.

Provenance: REQ ids verified against `spec/01-requirements.md`; RPC names read from `apps/web/lib/api/rpc.ts`;
component structure read from `artifact-viewer.tsx` and `thread-composer.tsx`; design rules from `docs/UI.md`
and `spec/08-ui-ux.md` — all CONFIRMED readings. The persona and its jobs are a GUESS (proposal only — no
media sector exists in the spec). No product code was modified.
