# UI proposal 062 — recruiting (screening, outreach and hiring decisions)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #062 — recruiting.
Read-only on product code; this file is the only write. Grounding (CONFIRMED): `docs/UI.md`,
`spec/08-ui-ux.md`, REQ ids in `spec/01-requirements.md`, RPC names in `apps/web/lib/api/rpc.ts`,
and two components read in full: `thread/message-blocks.tsx` (`CardBlock`) and `board/board-screen.tsx`.
No upstream brand, code, assets or wording consulted or copied.

Charter (CONFIRMED): Boma Bot only, fresh design (ADR-015), the owner-supplied robot-cat as the sole
identity; no upstream brand/code/assets/copy; BoardUI semantic tokens only — no raw palette class or
hex literal, every class list merged with `cx()`; one design system, no second library or hand-built
lookalike; self-hosted on the recruiter's own machine — no CV, candidate detail, salary, requisition
or client name leaves the premises (no SaaS/cloud, no third-party hiring-tool widget CSS). Where
current code uses raw `<button>`, `<input>` or `<select>`, new work uses BoardUI
`Button`/`IconButton`/`Input`/`Select` instead.

## 1. Persona and top jobs-to-be-done

**Persona — "the talent lead"**: lead at an in-house people team or small staffing firm running Boma
Bot on the firm's own machine. She directs bots to screen applicants, draft outreach and build
shortlists, reviews results on a calibrated desktop and phone, and serves English, French, Arabic and
Hebrew colleagues. Candidate PII, CVs, salaries and client names never leave the firm; no cloud. She is
measured on fairness, not speed: advancing the wrong candidate, or deciding on a protected
characteristic or colour cue, is a real loss — ambiguity is worse than latency. Robot-cat is the only
identity art; no agency, ATS or client logo is borrowed.

Top jobs:
1. **Decide a hiring step** — advance or reject a bot-proposed candidate knowing the exact candidate,
   role, stage and consequence; never decide by colour alone.
2. **Take the outreach** — take over the bot's messaging/scheduling session to write a personal note,
   then release it with an attributable record.
3. **Direct from anywhere** — send a candidate brief, attach a CV, and reply in-thread without losing
   the draft.
4. **Name the work** — state every screening/outreach run out loud (§8.9 vocabulary), retry only
   failures, pause or resume queued work, with a link to the message that started it.
5. **Register and share** — keep a versioned candidate-dossier library and publish a compiled hiring
   summary by link with no login, never leaking another client's or candidate's data.

## 2. Five concrete UI changes

All new copy is an i18n-catalogue key via `useTranslation()`'s `t(key)`; no string in JSX. Every class
list goes through `cx()`; every colour rides the `docs/UI.md` token map. Bot-produced bytes are
untrusted text or images, never markup; keep candidate names, salaries and requisition ids out of logs.

**A. Hiring-decision card — thread `/app/:botId` (REQ-007, REQ-017).** Extend `CardBlock` in
`message-blocks.tsx`: make the "what will happen" sentence dominant — candidate, role, stage,
consequence — with an explicit confirm label ("Advance <candidate> to onsite for <role>", not just
"Approve"). A candidate-facing or irreversible step pairs `border-border-error-default` +
`text-text-error-primary` with a leading icon and the words "Candidate-facing" — never colour alone.
Replace the card's raw buttons/input with BoardUI `Button`/`Input`, plus `Badge`/`Tooltip`; card
`bg-background-secondary-default`, `rounded-3xl`, `border-separator-border`, text
`text-text-primary`/`text-text-secondary`, focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`.
Existing RPC: `threadsAnswer` (the `CONFLICT` guard already handles a double answer); `threadsReact`
for reactions. Transcript stays readable, other threads reachable, focus lands on the card; announce
once via the polite live region (§8.9), the transcript is not a live region.

**B. Outreach canvas panel with takeover — thread right panel (REQ-026, REQ-028, REQ-029).** Reuse
`computer-panel.tsx` for the bot drafting outreach or scheduling interviews: starting / running /
taken over / released. A clear "Take over" `Button` hands input to the talent lead; "Release" returns
it; status is text + icon. BoardUI `Drawer`, `Spinner`, `Skeleton`, `Button`, `Badge`; surfaces
`bg-background-secondary-default` and `bg-background-full`, hairline `border-separator-border`, icon
`text-foreground-icon-primary`, 44 px `.boma-target`. Existing RPCs: `computerStatus`,
`computerOpenScreenFrame`, `computerTakeover`, `computerRelease`. No decorative spinner that outlives
its work; no second viewer or canvas library.

**C. Candidate-brief composer — `/app/:botId` composer (REQ-001, REQ-008).** Extend
`thread-composer.tsx` with an "attach CV" `IconButton` (`RiFileTextLine`) using the installed
`file-upload` component and the existing reply-chip pattern; keep `@bot` mention, Enter to send /
Shift+Enter new line, stop while a run is active, and send disabled while an approval is pending.
BoardUI `IconButton`, `Tooltip`; icons from `@remixicon/react` as references,
`text-foreground-icon-primary`, focus token ring. Existing RPC `threadsSend`; the CV enters the same
draft state, so no new transport. Candidate names, requisition ids and dates stay LTR in `<bdi>`.

**D. Screening-run monitor — `/app` Activity view (REQ-011, REQ-012, REQ-021, REQ-067).** Reuse
`ActivityScreen`: group screening and outreach runs by day with prompt snippet, bot name, a named
status chip (queued/leased/running, waiting_input/waiting_takeover, completed/failed/cancelled — §8.9),
duration and a link back to the starting message. Working runs sort to the top and update in place; a
failed screen gets one sentence of cause plus one of what to do and a working Retry, never a bare
spinner. BoardUI table/chip, `Button`, `IconButton`, `Tooltip`; surfaces
`bg-background-primary-default`/`bg-background-secondary-default`, `border-separator-border`, status
text + icon. Existing RPCs: `runsList`, `runsRetry`, `runsPause`, `runsResume`, `threadsHead`.

**E. Dossier library, requisition board and hiring-summary link — `/app/artifacts` (REQ-036),
`/app/board` (REQ-038, feature-flagged), `/p/:slugToken` (REQ-037).** Reuse `ArtifactsGrid` as a
versioned candidate-dossier library: name, type, size, owner bot, updated time, filter, search, version
chain, "open in thread". Replace its raw `<select>`/`<input>` with BoardUI `Select`/`Input` (one-system
rule); keep bounded frames and untrusted-byte rules. Retitle the flag-gated board columns as applied →
screen → interview → offer via `boardsGet`, `ticketsCreate`, `ticketsMove`, `ticketsComment`; a 404 keeps
the route absent, not locked (§8.6). Publish a compiled hiring summary at `/p/:slugToken` with no chrome
beyond a small robot-cat footer; unknown, revoked and expired tokens render one identical "not
available" document — same words, shape and timing. RPCs: `artifactsListSpace`, `artifactsGet`,
`artifactsListVersions`, `artifactsRemove`; shared transport: `GET /api/v1/pages/shared/:token`.

## 3. Accessibility, i18n and RTL

- Every visible string goes through `t(key)` (English source; fr/he; missing keys fall back to English); no string in JSX.
- `DirectionProvider` (`@/components/foundations/direction/direction`) gets an explicit BCP 47 locale;
  the boot script writes `lang`/`dir` together. Logical utilities throughout (`ms`/`me`, `ps`/`pe`,
  `text-start`/`text-end`, `border-s`/`border-e`). Mirror the rail, drawer motion and board columns
  via `useDirection()`; flip arrows. Candidate names, requisition ids, dates, URLs and OTP digits stay
  LTR in `<bdi>`/`dir="auto"`, formatted with `Intl`. Measured geometry and artwork are never mirrored.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px `.boma-target`;
  token-driven visible focus; full keyboard path (decision, takeover/release, attach, retry/pause,
  board move, version select); status and decision are text + icon, never colour alone; 360 px width;
  `prefers-reduced-motion`; no animation on streaming content.
- The hiring decision is announced once via the polite live region — the transcript is not a live
  region, so streaming tokens are not read aloud. The uniform unavailable screen is announced in
  identical text — no leak through the accessibility tree. Error copy is one sentence of what happened
  plus one of what to do; no candidate name, salary, stack trace or provider text on screen
  (correlation id behind "details"). The robot-cat mark stays unchanged.

## 4. Playwright tests (REQ id in each name)

```
test("REQ-001 the candidate-brief composer sends one optimistic message that is not duplicated", ...)
test("REQ-007 a pending hiring-decision card names candidate, role, stage, consequence and takes focus", ...)
test("REQ-017 advancing a candidate answers the card exactly once with an attributable confirmation", ...)
test("REQ-021 a failed screening shows a plain cause and a working retry action", ...)
test("REQ-026 the outreach canvas panel moves from starting to running with a named status", ...)
test("REQ-028 takeover and release are keyboard-operable and update the panel status", ...)
test("REQ-011 the activity view lists working screening runs and links to the starting message", ...)
test("REQ-012 every run status renders as text plus icon in light, dark and RTL", ...)
test("REQ-036 the dossier library selects a version and opens it in the originating thread", ...)
test("REQ-038 the requisition board moves a card by keyboard and is absent when the flag is off", ...)
test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)
test("REQ-057 Hebrew RTL mirrors the board and axe reports zero critical or serious findings", ...)
```

Behaviour without a REQ yet (decision framing, candidate-facing marker) needs an owner-assigned id before implementation — do not invent one in a test name.
Goldens under `spec/golden/ui/` stay byte-exact; a new seeded state means a new `REQ-<id>-<slug>.json`,
never a click path.

## 5. Best first slice

**Change A — the hiring-decision card.** GUESS, recommended: it is the smallest, highest-value step
and the only one that directly prevents a wrong candidate-facing decision through an ambiguous
confirmation. `threadsAnswer`, `Button`, `Badge` and `Tooltip` are installed, the tokens exist, and no
new RPC, component install or data model is needed (the `CONFLICT` guard in `message-blocks.tsx`
already owns the double-answer case). Test-first: write `REQ-007 a pending hiring-decision card names
candidate, role, stage, consequence and takes focus`, extend `spec/golden/ui/REQ-007-pending-card.json`,
then ship only the dominant consequence line, the explicit "Advance <candidate> to onsite" label, the
candidate/role attribution, the text+icon candidate-facing marker, keyboard operability and the polite
announcement. Defer (B)–(E) until the owner assigns any missing REQ ids and approves scope.

**Decision needed from the owner**: confirm the sector framing, and whether REQ-038's feature flag is
in scope for this deployment before change (E) is scheduled.

Provenance: REQ ids, RPC names, both components and the design docs above are CONFIRMED readings.
The persona and its jobs are a GUESS (no recruiting sector exists in the spec). No product code changed.
