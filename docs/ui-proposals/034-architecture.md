# UI proposal 034 — architecture (self-hosted practice desk)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #034 — architecture.
Read-only on product code; this file is the only write. Grounding (CONFIRMED, read this session):
`docs/UI.md`, `spec/08-ui-ux.md`, `spec/01-requirements.md` (REQ ids), `apps/web/lib/api/rpc.ts`
(RPC names), and two components: `thread/message-blocks.tsx` and `artifacts/artifact-viewer.tsx`.
No upstream brand, code, assets or wording consulted or copied.

Charter (CONFIRMED): Boma Bot only, fresh design (ADR-015), the owner-supplied robot-cat as the sole
identity; no upstream brand/code/assets/copy; BoardUI semantic tokens only — no raw palette class or
hex literal, classes merged with `cx()`; one design system, no second library or hand-built lookalike;
self-hosted only (no SaaS — no drawing, site photo or client record leaves the practice machine).

## 1. Persona and top jobs-to-be-done

**Persona — "the project architect"**: lead architect at a small practice running Boma Bot on the
practice's own machine. She reviews bot-drafted drawing revisions, coordinates structural and MEP
consultants through bot delegates, and issues sheet sets to clients. She serves English, French,
Arabic and Hebrew colleagues. Drawings, mark-ups and client data never leave the building; no cloud.
She is measured on liability, not speed: issuing revision C when the client approved B, or approving
a change on a colour cue, is a real loss — ambiguity is worse than latency. The robot-cat mark is the
only identity art; no studio, institute or software logo is borrowed.

Top jobs:
1. **Issue** — approve a bot-proposed revision knowing the exact sheet, revision, scope and code
   impact; never decide by colour alone.
2. **Red-line** — take over the bot's CAD/BIM session when a draft needs a human hand, then release it
   with an attributable record.
3. **Command** — ask the bot from site and attach a marked-up sheet or photo without losing the draft.
4. **Track** — name the state of every submittal/RFI run out loud (§8.9 vocabulary) and retry or park
   one in place, with a link back to the message that started it.
5. **Register and share** — keep a versioned drawing register and publish a sheet set by link with no
   login, never leaking another project.

## 2. Five concrete UI changes

All new copy is a `lib/i18n.ts` key via `useTranslation` / `t(locale, key)`; no string in JSX. Every
class list goes through `cx()`; every colour rides the `docs/UI.md` token map. Bot-produced bytes are
untrusted text or images, never markup.

**A. Revision-issue approval card — thread `/app/:botId` (REQ-007, REQ-017).**
Extend `CardBlock` in `message-blocks.tsx`: make the "what will happen" sentence dominant — project,
sheet number, revision, scope and code impact — with an explicit confirm label ("Issue revision C",
not just "Approve"). High-impact changes pair `border-border-error-default` + `text-text-error-primary`
with a leading icon and the words "High impact" — never colour alone. BoardUI `Button` (in place of the
raw `<button>`), `Badge`, `Tooltip`; card `bg-background-secondary-default`, `rounded-3xl`,
`border-separator-border`, text `text-text-primary`/`text-text-secondary`, focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`. Existing RPC: `threadsAnswer` (the
`CONFLICT` guard already handles a double answer). Transcript stays readable, other threads reachable,
focus lands on the card; announce once via the polite live region (§8.9).

**B. CAD/BIM review panel with takeover — thread right panel (REQ-026, REQ-028, REQ-029).**
Reuse `computer-panel.tsx` for the bot drafting a sheet: starting / running / taken over / released.
A clear "Take over" `Button` hands input to the architect; "Release" returns it; status is text + icon.
BoardUI `Drawer`, `Spinner`, `Skeleton`, `Button`, `Badge`; surfaces `bg-background-secondary-default`
and `bg-background-full`, hairline `border-separator-border`, icon `text-foreground-icon-primary`,
44 px `.boma-target`. Existing RPCs: `computerStatus`, `computerOpenScreen`,
`computerOpenScreenFrame`, `computerTakeover`, `computerRelease`. Never a decorative spinner that
outlives its work; no second viewer library.

**C. Mark-up command composer — `/app/:botId` composer (REQ-001).**
Extend `thread-composer.tsx` with an "attach marked-up sheet" `IconButton` (`RiFileTextLine`) using the
installed `file-upload` component and the existing reply-chip pattern; keep `@bot` mention, Enter to
send / Shift+Enter new line, and send disabled while an approval is pending. BoardUI `IconButton`,
`Tooltip`; icons from `@remixicon/react` as references, `text-foreground-icon-primary`, focus token
ring. Existing RPC `threadsSend`; the attachment enters the same draft state, so no new transport.
Sheet numbers, revision letters and dates stay LTR in `<bdi>`.

**D. Submittal/RFI run monitor — `/app` Activity view (REQ-011, REQ-012, REQ-021).**
Reuse `ActivityScreen`: group submittal and RFI runs by day with prompt snippet, bot name, a named
status chip (queued/leased/running, waiting_input/waiting_takeover, completed/failed/cancelled —
§8.9), duration and a link back to the starting message. Working runs sort to the top and update in
place; a failed issue check gets one sentence of cause plus one of what to do and a working Retry,
never a bare spinner. BoardUI table/chip, `Button`, `IconButton`, `Tooltip`; surfaces
`bg-background-primary-default`/`bg-background-secondary-default`, `border-separator-border`, status
text + icon. Existing RPCs: `runsList`, `runsRetry`, `runsPause`, `runsResume`, `threadsHead`.

**E. Drawing register, issue board and client link — `/app/artifacts` (REQ-036), `/app/board`
(REQ-038, feature-flagged), `/p/:slugToken` (REQ-037).**
Reuse `ArtifactsGrid` and `ArtifactViewer` as a versioned register: sheet number, discipline, revision,
owner bot, version chain, download and "open in thread". Replace the raw `<select>` in
`artifact-viewer.tsx` with the installed BoardUI `Select` to honour the one-system rule; keep its
bounded frame and untrusted-byte rules. BoardUI `Input`, `Select`, `Chip`, `Button`; token surfaces and
text as above. Retitle the flag-gated board columns as draft → coordinated → issued → archived via
`boardsGet`/`ticketsCreate`/`ticketsMove`/`ticketsComment`. Publish a compiled sheet set at
`/p/:slugToken` with no chrome beyond a small robot-cat footer; unknown, revoked and expired tokens
render one identical "not available" document. Register RPCs: `artifactsListSpace`, `artifactsGet`,
`artifactsListVersions`, `artifactsRemove`; shared transport `GET /api/v1/pages/shared/{token}`.

## 3. Accessibility, i18n and RTL

- All visible strings go through `t(locale, key)` (English source; fr/he translations; a missing key
  falls back to English, never a blank or the raw key). No string in JSX.
- `DirectionProvider` (`@/components/foundations/direction/direction`) gets an explicit BCP 47 locale;
  the boot script writes `lang`/`dir` together. Logical utilities throughout (`ms`/`me`, `ps`/`pe`,
  `text-start`/`text-end`, `border-s`/`border-e`). Mirror the rail, drawer motion and board columns via
  `useDirection()`; flip arrows. Sheet numbers, revision letters, dimensions, codes, URLs and OTP
  digits stay LTR in `<bdi>`/`dir="auto"`, formatted with `Intl` (no floats for areas).
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px `.boma-target`;
  token-driven visible focus; full keyboard path (issue decision, takeover/release, attach, retry/
  pause, board move, version select); status is text + icon, never colour alone; 360 px width;
  `prefers-reduced-motion`; no animation on streaming content.
- The issue decision is announced once via the polite live region — the transcript is not a live
  region, so streaming tokens are not read aloud. Error copy is one sentence of what happened plus one
  of what to do; no sheet id, stack trace or provider text on screen (correlation id behind "details").
  The robot-cat mark stays unchanged.

## 4. Playwright tests (REQ id in each name)

```
test("REQ-001 the mark-up composer sends one optimistic message that is not duplicated", ...)
test("REQ-007 a pending revision card names sheet, revision, scope, code impact and takes focus", ...)
test("REQ-017 issuing revision C answers the card exactly once and reopens the composer", ...)
test("REQ-021 a failed issue check shows a plain cause and a working retry action", ...)
test("REQ-026 the CAD panel moves from starting to running with a named status", ...)
test("REQ-028 takeover and release are keyboard-operable and update the panel status", ...)
test("REQ-011 the activity view lists working submittal runs and links to the starting message", ...)
test("REQ-012 every run status renders as text plus icon in light, dark and RTL", ...)
test("REQ-036 the drawing register selects a version and opens it in the originating thread", ...)
test("REQ-038 the issue board moves a sheet by keyboard and is absent when the flag is off", ...)
test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)
test("REQ-057 the architecture screens mirror for Hebrew with a clean axe run", ...)
```

Behaviour without a REQ yet (revision-issue framing, mark-up chip, high-impact marker) must be
assigned an id by the owner before implementation per the traceability gate — do not invent one in a
test name. Goldens under `spec/golden/ui/` stay byte-exact; a new seeded state means a new
`REQ-<id>-<slug>.json`, never a click path.

## 5. Best first slice

**Change A — the revision-issue approval card.** GUESS, recommended: it is the smallest, highest-value
step and the only one that directly prevents a wrong revision being issued through an ambiguous
confirmation. `threadsAnswer`, `Button`, `Badge` and `Tooltip` are installed, the tokens exist, and no
new RPC, component install or data model is needed (the `CONFLICT` guard in `message-blocks.tsx`
already owns the double-answer case). Test-first: write `REQ-007 a pending revision card names sheet,
revision, scope, code impact and takes focus`, extend `spec/golden/ui/REQ-007-pending-card.json`, then
ship only the dominant consequence line, the explicit "Issue revision C" label, the sheet/revision
attribution, the text+icon high-impact marker, keyboard operability and the polite announcement.
Defer (B)–(E) until the owner assigns any missing REQ ids and approves scope.

**Decision needed from the owner**: confirm the sector framing, and whether REQ-038's feature flag is
in scope for this deployment before change (E) is scheduled.

Provenance: REQ ids verified this session against `spec/01-requirements.md`; RPC names and two
components read from `apps/web/lib/api/rpc.ts`, `thread/message-blocks.tsx` and
`artifacts/artifact-viewer.tsx`; design rules from `docs/UI.md` and `spec/08-ui-ux.md` — all CONFIRMED
readings. The persona and its jobs are a GUESS (proposal only — no architecture sector exists in the
spec). No product code was modified.
