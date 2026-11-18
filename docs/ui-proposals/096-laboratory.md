# UI proposal 096 — laboratory (clinical and diagnostic specimen workflow)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #096 — laboratory.
Read-only on product code; this file is the only write. Grounding (CONFIRMED, read this session):
`docs/UI.md`, `spec/08-ui-ux.md`, `spec/01-requirements.md` (REQ ids), `apps/web/lib/api/rpc.ts`
(RPC names) and two components — `thread/message-blocks.tsx` (`CardBlock`: `threadsAnswer` with its
`CONFLICT` guard, raw `<button>`/`<input>`, bot text as data, text+icon error row) and
`artifacts/artifact-viewer.tsx` (bounded frame `max-h-[60vh]`, version selector via raw `<select>`,
sha256, untrusted-byte walls). No upstream brand, code, asset, wording or capture consulted or copied.

Charter (CONFIRMED): Boma Bot only; fresh design (ADR-015) with the owner-supplied robot-cat as sole
identity; no upstream mark/code/assets/copy. BoardUI semantic tokens only — no raw palette class or
hex literal, composite type utilities, every class list merged with `cx()`; one design system, no
second library and no hand-built lookalike of an installed component. Self-hosted on the lab's own
machine — no SaaS/cloud; no specimen, patient, sequence or result leaves the premises. Lab data is
untrusted text or bytes, never markup.

## 1. Persona and top jobs-to-be-done

**Persona — "the lab supervisor":** a clinical/diagnostic laboratory lead who directs bots to
accession specimens, queue analyzer runs, flag critical results and release verified patient reports.
She works between a bench workstation and a phone, in English, French, Arabic and Hebrew. A released
wrong result or a missed critical value is a patient-safety event, so ambiguity is worse than latency.
The robot-cat mark is the only identity art; a specimen label, plot or micrograph is user content.

Top jobs:
1. **Track specimens** — see accession status, unread results and open questions at a glance, not
   by scrolling the transcript (REQ-009, REQ-012).
2. **Release safely** — review a proposed result/report knowing the exact specimen id, assay and
   revision; never confirm a critical value on colour alone (REQ-007, REQ-017).
3. **Name run state** — say out loud which analyzer, QC and review runs are working, paused or
   terminal (§8.9 vocabulary); retry only failures, pause/resume queued work (REQ-011, REQ-067).
4. **Trace a result** — tie a report to its sha256, version chain, owner bot and originating run
   (REQ-036); attach chromatograms and QC files without losing the draft.
5. **Share a verified report** — publish a reviewed result page by link with no login, identical
   "not available" for unknown/revoked tokens, no leak of another patient's data (REQ-037).

## 2. Five concrete UI changes

All new copy is a `lib/i18n.ts` key via `useTranslation()` → `t(locale, key)` (English source, fr/he
translations; a missing key falls back to English, never blank or raw key); no string in JSX. Every
colour rides the `docs/UI.md` token map, type uses composite utilities, dark mode rides the tokens,
every class list goes through `cx()`. Bot-produced values are untrusted text/bytes, never HTML.

**(a) Specimen status strip — thread `/app/:botId` header (REQ-009, REQ-012).** A compact read-only
band above the transcript with named chips for "specimens pending", "unread results" and "open
flags", each a keyboard-focusable `Tooltip`. Installed BoardUI `Chip`/`Badge`
(`components/base/badges/chip.tsx`), `Tooltip`, `IconButton` (icons from `@remixicon/react` as
component references). Tokens: `bg-background-secondary-default`, `border-separator-border`,
`text-text-primary`/`text-text-secondary`, `text-foreground-icon-secondary`,
`text-caption-1-semibold`, joined with `cx()`. Existing RPCs: `botsGet`, `threadsHead`,
`threadsMessages`, `runsList`. Accession ids stay LTR in `<bdi>` and out of logs.

**(b) Critical-result / release approval card — thread pending card (REQ-007, REQ-017).** Extend
`CardBlock` so the "what will happen" sentence dominates: exact specimen id, assay and result value,
with a distinct verdict label ("Release report", not just "Approve"). A critical or irreversible
release pairs `border-border-error-default` + `text-text-error-primary` with a leading icon and the
words "Critical value" — never colour alone. Replace the raw `<button>`/`<input>` in `CardBlock`
with installed BoardUI `Button`/`Input` (one-system rule); card `bg-background-secondary-default`,
`rounded-3xl`, `border-separator-border`, focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`,
44 px `.boma-target`. Existing RPC: `threadsAnswer`, keeping the `CONFLICT` guard `CardBlock` owns.
The card takes focus but does not block reading or other threads; send waits while approval is pending.

**(c) Analyzer / QC run triage — activity `/app` (REQ-011, REQ-012, REQ-021, REQ-067).** Group runs
by day with prompt snippet, bot name and a distinct text+icon chip for the full run vocabulary
(`queued`/`leased`/`running`, `waiting_input`/`waiting_takeover`, `completed`/`failed`/`cancelled`);
sort failing and waiting to the top, retry only `failed`, pause/resume working states. Installed
BoardUI `Tabs` as status filter, `Chip`, `Tooltip`, `IconButton`, `Button`; surface
`bg-background-primary-default`, hairline `border-separator-border`, text `text-text-primary`/
`text-text-secondary`. Existing RPCs: `runsList`, `runsRetry`, `runsPause`, `runsResume`,
`threadsHead`. Failures get one cause sentence + one action sentence, correlation id behind
"details" — never a stack trace, patient id or provider text. Changes go to the polite live region.

**(d) Result-and-file composer — thread composer and transcript (REQ-001, REQ-002).** Extend the
composer with an "attach QC file" `IconButton` (`RiFileTextLine`) and a named file `Chip`; keep the
React Aria `TextField`/`TextArea`, `@bot` mention, reply chip, Enter-to-send / Shift+Enter-newline,
stop while a run is active and send-disabled-on-empty. Render attached chromatograms and QC reports
as bounded file/image blocks in the viewer frame with real `alt` — blob/data URL only, never app
HTML. Installed BoardUI `Button`/`IconButton`/`Chip`; surface `bg-background-primary-default`, icon
`text-foreground-icon-primary`, token focus ring, classes via `cx()`. Existing RPCs: `threadsSend`,
`threadsMessages`. Strip EXIF/GPS; never expose a patient identifier in DOM, `alt` or logs.

**(e) Report register and verified share link — `/app/artifacts` + `/p/:slugToken` (REQ-036,
REQ-037).** Reuse `ArtifactsGrid`/`ArtifactViewer` as a versioned report register: filter by
type/bot, search by name, show the result → review → release chain, "open in thread" and version
selector; keep the bounded frame and untrusted-byte walls. Replace the raw `<select>` in
`artifact-viewer.tsx` with the installed BoardUI `Select`. Surface sha256 in `<bdi>`, owner bot and
originating run. Existing RPCs: `artifactsListSpace`, `artifactsGet`, `artifactsListVersions`,
`artifactsRemove`. Publish a reviewed result page with no session and no chrome beyond a small
robot-cat footer; unknown, revoked and expired tokens render one identical "not available" document
— same words, shape and timing. `noindex`/`no-referrer` and its CSP stay backend concerns.

## 3. Accessibility, i18n and RTL

- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; keyboard-only path for
  every primary flow (card verdict, status filter, pause/retry, attach, version select, report share);
  `focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target`; 360 px;
  `prefers-reduced-motion`; no animation on streaming content.
- Critical value, status and failure are always text + icon + token, never colour alone; the card
  effect is announced once via the polite live region — the transcript is not a live region (§8.9),
  so streaming tokens are not read aloud. Close/verdict returns focus to the trigger; Escape closes.
- Every user-visible string goes through `t(locale, key)`; English fallback. `DirectionProvider`
  (`@/components/foundations/direction/direction`) gets an explicit BCP 47 locale; the boot script
  writes `lang`/`dir` together. RTL mirrors rail, strip and drawer motion via `useDirection()`;
  arrows flip. Logical utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`).
  Accession ids, dates, doses, sha256 and URLs stay LTR in `<bdi>`/`dir="auto"`; physical plot
  geometry is never mirrored.
- Self-host only: no SaaS destination, no lab datum in a hover-only tooltip, nothing patient-related
  written to logs. Error copy is one sentence of what happened plus one of what to do (§8.9).

## 4. Playwright tests (REQ id in each name)

- `test("REQ-007 a critical-result card names specimen, assay and value and uses text+icon, not colour", ...)`
- `test("REQ-017 release and hold answer the card exactly once and leave an attributable confirmation", ...)`
- `test("REQ-012 every run status renders a distinct named chip with the correct terminal action", ...)`
- `test("REQ-011 activity sorts failed and waiting analyzer runs to the top", ...)`
- `test("REQ-067 pause and resume are keyboard-operable and announced once", ...)`
- `test("REQ-021 a failed QC run shows a plain cause, one retry, and the correlation id behind details", ...)`
- `test("REQ-001 attaching a QC file sends one optimistic message that is not duplicated", ...)`
- `test("REQ-036 the report register shows the sha256, version chain, owner bot and run without rendering HTML", ...)`
- `test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)`
- `test("REQ-057 Hebrew RTL mirrors the strip and report list and axe reports zero critical or serious findings", ...)`
- Goldens under `spec/golden/ui/` stay byte-exact; a new seeded state means a new
  `REQ-<id>-<slug>.json`, never a click path. Behaviour without a REQ yet needs an owner-assigned id
  before code — do not invent one in a test name.

## 5. Best first slice

**Change (b), the critical-result / release approval card — GUESS, recommended.** One screen, no new
transport (`threadsAnswer` exists and `CardBlock` already owns the `CONFLICT` guard), no new component
(BoardUI `Button`/`Input`/`Chip` are installed per `docs/UI.md`), no token re-tune. It directly reduces
the risk of releasing a critical value on a colour cue. Test-first: write `REQ-007 a critical-result
card names specimen, assay and value…`, extend `spec/golden/ui/REQ-007-pending-card.json`, add an axe
assertion (light, dark, RTL). Ship only the dominant consequence sentence, the text+icon critical
marker, the `Button`/`Input` migration in `CardBlock`, keyboard path and polite announcement. Defer
(a), (c), (d), (e) until the owner confirms the lab framing and approves scope.

Provenance: REQ ids from `spec/01-requirements.md`, RPCs from `apps/web/lib/api/rpc.ts`, the two
listed components, token vocabulary and rules from `docs/UI.md`/`spec/08-ui-ux.md` — all CONFIRMED.
The laboratory persona, jobs and priorities are a GUESS (no laboratory sector in the spec). No
product code was modified.
