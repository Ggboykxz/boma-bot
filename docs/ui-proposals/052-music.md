# UI proposal 052 — music (self-hosted studio and label operations)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #052 — music. Read-only on product
code; this file is the only write. Grounding (CONFIRMED, read this session): `docs/UI.md`, `spec/08-ui-ux.md`,
`spec/01-requirements.md` (REQ ids), `apps/web/lib/api/rpc.ts` (RPC names) and two components —
`apps/web/components/thread/thread-composer.tsx` (reply-chip composer, Enter/Shift+Enter, `awaitingApproval`
gate) and `apps/web/components/artifacts/artifact-viewer.tsx` (bounded, untrusted-byte viewer with version
selector, download, "open in thread"). No upstream brand, code, asset or wording was consulted or copied.

Charter (CONFIRMED): Boma Bot only, fresh design (ADR-015), owner-supplied robot-cat as the sole identity;
no upstream brand/code/assets/copy; BoardUI semantic tokens only — no raw palette class or hex literal,
every class list merged with `cx()`; one design system, no second library or hand-built lookalike;
self-hosted on the operator's own machine — no SaaS/cloud, no stem, session, master or unreleased track
leaving the building; product name and mark unchanged.

## 1. Persona and top jobs-to-be-done

**Persona — "the studio release manager"**: a producer/label operations lead at a self-hosted recording
studio or independent label, working in English, French and Hebrew on a control-room display and a laptop
on a poor link. Boma Bot runs on the studio's own server: stems, masters and licensing terms never leave
the building. She is measured on getting the right version to the right destination; confirming a public
release or a master overwrite against the wrong take or on a colour cue is an irreversible, reputational
loss — legible state and an explicit "what will happen" beat speed. The robot-cat mark is the only identity
art; no label, artist, DSP or platform logo is borrowed.

Top jobs:
1. **Attach and brief** — hand a stem, reference or session file to a bot in a prompt without losing the
   note to a stray Enter, and see which takes are new, unread or awaiting sign-off without scrolling a transcript.
2. **Verify before release** — review a bot-proposed export, master, upload or licence decision and be
   certain of the exact track, version, destination and effect before approving; never approve by colour alone.
3. **Track production** — watch render, master, transcode and distribution runs by name and status, retry
   only failures, pause/resume queued work, with a link back to the originating message.
4. **Move a release** — push a track through rough → mix → master → approved → released, keeping the history
   and attached runs, and keep work visible on the board rather than in chat.
5. **Archive and share** — keep versioned, searchable audio and session packages, open each version in the
   thread that produced it, and send a listening link to a collaborator or licensee with no login and no leakage.

## 2. Five concrete UI changes

All new copy is a new `lib/i18n.ts` key via `useTranslation` / `t(locale, key)`; nothing hardcoded in JSX.
Every class list goes through `cx()`; every colour rides the `docs/UI.md` token map; type uses composite
utilities; icons are `@remixicon/react` component references. Bot- and artifact-produced values are untrusted
text/bytes, never HTML (`artifact-viewer.tsx`).

**(a) Session status strip — thread `/app/:botId` header.** A compact read-only band above the transcript
with named chips for "new stems", "unread notes" and "awaiting sign-off", each with a `Tooltip` carrying
detail (track, version, count). Components: `Chip`/`Badge` (`components/base/badges/chip.tsx`), `Tooltip`,
`IconButton`. Tokens: surface `bg-background-secondary-default`, hairline `border-separator-border`, text
`text-text-primary`/`text-text-secondary`, icon `text-foreground-icon-secondary`, type `text-caption-1-semibold`,
joined with `cx()`. Existing RPCs: `botsGet`, `threadsHead`, `threadsMessages`. Track ids and hashes stay LTR
in `<bdi>`.

**(b) Release/export approval card — thread `/app/:botId` pending card (REQ-007/REQ-017).** Make the "what
will happen" sentence dominant: exact track and version, destination (DSP, licensee, download) and effect,
with an explicit confirm label ("Release 'Track' v3 to <destination>", not just "Approve"). High-impact
actions (public release, overwrite a master, retract) pair `border-border-error-default` +
`text-text-error-primary` with a leading icon and the words "High impact" — never colour alone. Components:
BoardUI `Button`, `Badge`, `Tooltip`; card surface `bg-background-primary-default`, `rounded-3xl`,
`border-border-button-default`, focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`. Existing
RPC `threadsAnswer` (exactly-once effect, keeps the `CONFLICT` guard). Announced once via the polite live
region; transcript stays readable, other threads reachable (spec §8.9).

**(c) Audio-attach composer — thread `/app/:botId` composer (`thread-composer.tsx`, REQ-001).** Extend the
composer with a named "attach audio" `IconButton` (`RiAttachment2Line`) that adds a stem/reference as a
reply-style chip, reusing the existing reply-banner pattern. Keep Enter-to-send / Shift+Enter-newline exactly
as today and keep send blocked while an approval is pending (`awaitingApproval`). Component: BoardUI
`IconButton` + `Tooltip`; token-driven focus, 44 px `.boma-target`, icon `text-foreground-icon-primary`. RPC
`threadsSend`; the reference enters the same `value` state, so no new transport. Prefer the installed
`file-upload` component over a hand-built control. Filenames, hashes, timecodes and BPM/key tags stay LTR in
`<bdi>`.

**(d) Production-run triage — `/app` Activity view (REQ-011/REQ-012/REQ-021/REQ-067).** Extend the activity
list so each render, master, transcode and distribution run names its state out loud with a music caption
(`queued` → "in the queue", `running` → "rendering", `waiting_takeover` → "awaiting operator", `failed` →
"export failed") while the canonical §8.9 vocabulary stays; working/paused rows keep Pause/Resume
(REQ-067), failed rows keep Retry (REQ-021). Working runs sort to the top and update in place — no decorative
spinner. Components: `Button`, `Chip`, `Badge`; surfaces `bg-background-primary-default`/
`bg-background-secondary-default`, `border-separator-border`, error row `border-border-error-default` +
`text-text-error-primary`; status as text + icon via `runStatusKey`/`isRetryable`. RPCs: `runsList`,
`runsPause`, `runsResume`, `runsRetry`, `threadsHead`.

**(e) Audio library and shared listening page — `/app/artifacts` and `/p/:slugToken` (REQ-036/REQ-037).**
Reuse `ArtifactsGrid` and the bounded viewer: name stems, mixes and session packages by type, filter by
type/bot, search by name, show the version chain and "open in thread". Audio renders in a bounded,
keyboard-operable player — never autoplay; other bytes are a download; never inject markup. Prefer installed
BoardUI `Select` over the viewer's raw `<select>`. Components: `Input`, `Select`, `Chip`, `Button`; tokens
`text-text-primary`/`text-text-secondary`/`text-text-tertiary`, `bg-background-primary-default`,
`border-border-button-default`. RPCs: `artifactsListSpace`, `artifactsGet`, `artifactsListVersions`,
`artifactsRemove`. A compiled listening page is shared through `/p/:slugToken` (transport
`GET /api/v1/pages/shared/{token}`, CONFIRMED in `shared-page-screen.tsx`) with no session and no chrome
beyond a small robot-cat footer; unknown, revoked and expired tokens render one identical "not available"
screen — same words, shape and timing — with `noindex`, `no-referrer` and its own CSP.

## 3. Accessibility

WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; a keyboard-only path for every
primary flow (release decision, attach, board move, pause/resume/retry, version select); visible
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target`; 360 px width;
`prefers-reduced-motion` honoured; no animation on streaming content. Audio: any player needs keyboard
play/pause/seek, a focus ring and no autoplay; spoken-word assets carry a transcript or caption; status and
high-impact markers are text + icon, never colour alone. The uniform unavailable screen is announced in
identical text — no leak through the accessibility tree. Error copy is one sentence of what happened plus
one of what to do; no track id, hash, stack trace or provider text on screen (correlation id behind
"details" only). Tables carry `<caption>` and `<th scope>`.

## 4. i18n and RTL

English is the source catalogue, fr/he the translations; a missing key falls back to English — never blank
or the raw key. `DirectionProvider` (`@/components/foundations/direction/direction`) gets an explicit BCP 47
locale; the boot script writes `lang` and `dir` together. RTL mirrors the board columns, rail, pipeline
arrows and drawer motion via `useDirection()`; navigation glyphs flip. Logical utilities throughout
(`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Filenames, hashes, timecodes,
BPM/key tags, URLs, dates and numbers stay LTR in `<bdi>`/`dir="auto"`. Preserve physical coordinates for
measured audio geometry — waveform and timeline positions are never mirrored. Formatting application data is separate from layout direction.

## 5. Playwright tests (REQ id in each name)

- `test("REQ-001 the audio-attach composer sends one optimistic message that is not duplicated", ...)`
- `test("REQ-007 the release card names track, version, destination and effect and takes focus without blocking reading", ...)`
- `test("REQ-017 release and cancel answer the card exactly once with an attributable confirmation", ...)`
- `test("REQ-011 the activity view groups render and master runs by day and links to the source", ...)`
- `test("REQ-012 every production-run status renders a distinct named text-and-icon chip", ...)`
- `test("REQ-021 a failed export shows a plain cause and a working retry action", ...)`
- `test("REQ-067 pause and resume a queued master in place with the right terminal action", ...)`
- `test("REQ-036 the audio library filters by type and bot and shows the version chain", ...)`
- `test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)`
- `test("REQ-057 Hebrew RTL mirrors the board and the composer with a clean axe run", ...)`

Behaviour without a REQ yet (status strip, attach chip, high-impact marker, music captions) must be assigned
an id by the owner before code — do not invent one in a test name. Goldens under `spec/golden/ui/` stay
byte-exact. REQ-049 (voice/calls) is feature-flagged and out of scope here.

## 6. Best first slice

**Change (b), the release/export approval card** — GUESS, recommended. Smallest and highest-value: one
screen, no new transport (`threadsAnswer` exists), no new component (`Button`/`Badge`/`Tooltip` are
installed) and no token re-tune. It directly prevents a public release or a master overwrite being confirmed
on a colour cue or against the wrong version. Test-first: write
`REQ-017 release and cancel answer the card exactly once with an attributable confirmation`, extend
`spec/golden/ui/REQ-007-pending-card.json`, then ship only the dominant effect line, the explicit
"Release '<track>' v<version> to <destination>" label and the text+icon high-impact marker. Defer (a), (c),
(d), (e) until the owner assigns any missing REQ ids and approves scope.

**Decision needed from the owner**: confirm the studio/label framing and whether REQ-038's feature-flagged board is in scope here before (d)/(e) is scheduled.

Provenance: REQ ids CONFIRMED from `spec/01-requirements.md`; RPC names and the shared-page transport CONFIRMED
from `apps/web/lib/api/rpc.ts` and `shared-page-screen.tsx`; component structure CONFIRMED by reading
`thread-composer.tsx` and `artifact-viewer.tsx` this session; design rules CONFIRMED from `docs/UI.md` and
`spec/08-ui-ux.md`. The persona, jobs and music mapping are GUESS (no music sector or REQ exists in the spec). No product code was modified.
