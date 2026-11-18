# UI proposal 055 — design agencies (creative review and client delivery)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #055 — design agencies.
Read-only on product code; this file is the only write. Grounding (CONFIRMED, read this session):
`docs/UI.md`, `spec/08-ui-ux.md`, `spec/01-requirements.md` (REQ ids), `apps/web/lib/api/rpc.ts`
(RPC names), `thread/message-blocks.tsx` (`CardBlock`) and `artifacts/artifacts-grid.tsx`.
No upstream brand, code, assets or wording consulted or copied.

Charter (CONFIRMED): Boma Bot only, fresh design (ADR-015), the owner-supplied robot-cat as the sole
identity; no upstream brand/code/assets/copy; BoardUI semantic tokens only — no raw palette class or
hex literal, every class list merged with `cx()`; one design system, no second library or hand-built
lookalike; self-hosted on the agency's own machine — no client brief, moodboard, unreleased creative,
scope, fee or client name leaves the premises (no SaaS/cloud, no external design-tool widget CSS).
Where current code uses raw `<button>`, `<input>` or `<select>`, new work uses installed BoardUI
`Button`/`IconButton`/`Input`/`Select` instead.

## 1. Persona and top jobs-to-be-done

**Persona — "the creative director"**: lead at a small brand/design studio running Boma Bot on the
studio's own machine. She directs bots to draft identities, layouts and campaign visuals, reviews them
on a calibrated desktop and a phone between client calls, and serves English, French, Arabic and Hebrew
colleagues. Briefs, reference images and unreleased work never leave the studio; no cloud. She is
measured on judgment, not speed: shipping version B when a client approved A, or signing off on a
green/red colour cue, is a real loss — ambiguity is worse than latency. The robot-cat mark is the sole
identity art; no studio, client, type-foundry or software logo is borrowed.

Top jobs:
1. **Approve a deliverable** — know the exact client, deliverable, version and scope before sign-off; never decide by colour alone.
2. **Take the canvas** — take over the bot's design session to adjust a layout by hand, then release it with an attributable record.
3. **Direct from anywhere** — send a brief, attach a moodboard or reference, and reply in-thread without losing the draft.
4. **Name the work** — state every review/production run out loud (§8.9 vocabulary); retry only failures, pause/resume queued work.
5. **Register and share** — keep a versioned asset library and publish a compiled client review by link with no login, never leaking another client's work.

## 2. Five concrete UI changes

All new copy is a `lib/i18n.ts` key via `useTranslation` / `t(locale, key)`; no string in JSX. Every
class list goes through `cx()`; every colour rides the `docs/UI.md` token map. Bot-produced bytes are
untrusted text or images, never markup; keep client names and fee ids out of logs.

**A. Creative sign-off card — thread `/app/:botId` (REQ-007, REQ-017).** Extend `CardBlock` in
`message-blocks.tsx`: make the "what will happen" sentence dominant — client, deliverable, version,
scope — with an explicit confirm label ("Release logo v3 to <client>", not just "Approve"). A
high-impact release pairs `border-border-error-default` + `text-text-error-primary` with a leading
icon and the words "Client-visible" — never colour alone. Replace the card's raw buttons with BoardUI
`Button`, plus `Badge`/`Tooltip`; card `bg-background-secondary-default`, `rounded-3xl`,
`border-separator-border`, text `text-text-primary`/`text-text-secondary`, focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`. Existing RPC: `threadsAnswer` (the
`CONFLICT` guard already handles a double answer); `threadsReact` for reactions. Transcript stays
readable, other threads reachable, focus lands on the card; announce once via the polite live region
(§8.9), the transcript is not a live region.

**B. Design-canvas review panel with takeover — thread right panel (REQ-026, REQ-028, REQ-029).**
Reuse `computer-panel.tsx` for the bot drafting a layout: starting / running / taken over / released.
A clear "Take over" `Button` hands input to the creative director; "Release" returns it; status is
text + icon. BoardUI `Drawer`, `Spinner`, `Skeleton`, `Button`, `Badge`; surfaces
`bg-background-secondary-default` and `bg-background-full`, hairline `border-separator-border`, icon
`text-foreground-icon-primary`, 44 px `.boma-target`. Existing RPCs: `computerStatus`,
`computerOpenScreen`, `computerOpenScreenFrame`, `computerTakeover`, `computerRelease`. No decorative
spinner that outlives its work; no second viewer or canvas library.

**C. Creative-brief composer — `/app/:botId` composer (REQ-001, REQ-008).** Extend `thread-composer.tsx`
with an "attach reference" `IconButton` (`RiImageLine`) using the installed `file-upload` component
and the existing reply-chip pattern; keep `@bot` mention, Enter to send / Shift+Enter new line, stop
while a run is active, and send disabled while an approval is pending. BoardUI `IconButton`,
`Tooltip`; icons from `@remixicon/react` as references, `text-foreground-icon-primary`, focus token
ring. Existing RPC `threadsSend`; the attachment enters the same draft state, so no new transport.
Version numbers, dates and client codes stay LTR in `<bdi>`.

**D. Review-run monitor — `/app` Activity view (REQ-011, REQ-012, REQ-021, REQ-067).** Reuse
`ActivityScreen`: group review and production runs by day with prompt snippet, bot name, a named
status chip (queued/leased/running, waiting_input/waiting_takeover, completed/failed/cancelled —
§8.9), duration and a link back to the starting message. Working runs sort to the top and update in
place; a failed export gets one sentence of cause plus one of what to do and a working Retry, never a
bare spinner. BoardUI table/chip, `Button`, `IconButton`, `Tooltip`; surfaces
`bg-background-primary-default`/`bg-background-secondary-default`, `border-separator-border`, status
text + icon. Existing RPCs: `runsList`, `runsRetry`, `runsPause`, `runsResume`, `threadsHead`.

**E. Asset library, brief board and client link — `/app/artifacts` (REQ-036), `/app/board`
(REQ-038, feature-flagged), `/p/:slugToken` (REQ-037).** Reuse `ArtifactsGrid` as a versioned asset
library: name, type icon, size, owner bot, updated time, filter by type/bot, search by name, version
chain and "open in thread". Replace its raw `<select>`/`<input>` with the installed BoardUI `Select`
and `Input` to honour the one-system rule; keep the bounded frame and untrusted-byte rules. Retitle the
flag-gated board columns as brief → in progress → review → delivered via `boardsGet`, `ticketsCreate`,
`ticketsMove`, `ticketsComment`; a 404 keeps the route absent, not locked (§8.6). Publish a compiled
client review at `/p/:slugToken` with no chrome beyond a small robot-cat footer; unknown, revoked and
expired tokens render one identical "not available" document — same words, shape and timing. Register
RPCs: `artifactsListSpace`, `artifactsGet`, `artifactsListVersions`, `artifactsRemove`; transport
(CONFIRMED): `GET /api/v1/pages/shared/:token`.

## 3. Accessibility, i18n and RTL

- Every visible string goes through `t(locale, key)` (English source; fr/he translations; a missing
  key falls back to English, never a blank or the raw key). No string in JSX.
- `DirectionProvider` (`@/components/foundations/direction/direction`) gets an explicit BCP 47 locale;
  the boot script writes `lang`/`dir` together. Logical utilities throughout (`ms`/`me`, `ps`/`pe`,
  `text-start`/`text-end`, `border-s`/`border-e`). Mirror the rail, drawer motion and board columns
  via `useDirection()`; flip arrows. Client codes, version numbers, dates, URLs and OTP digits stay
  LTR in `<bdi>`/`dir="auto"`, formatted with `Intl`. Measured geometry and artwork are never mirrored.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px `.boma-target`;
  token-driven visible focus; full keyboard path (sign-off, takeover/release, attach, retry/pause,
  board move, version select); status is text + icon, never colour alone; 360 px width;
  `prefers-reduced-motion`; no animation on streaming content.
- The sign-off decision is announced once via the polite live region — the transcript is not a live
  region, so streaming tokens are not read aloud. The uniform unavailable screen is announced in
  identical text — no leak through the accessibility tree. Error copy is one sentence of what happened
  plus one of what to do; no client name, fee, stack trace or provider text on screen (correlation id
  behind "details"); the robot-cat mark stays unchanged.

## 4. Playwright tests (REQ id in each name)

```
test("REQ-001 the brief composer sends one optimistic message that is not duplicated", ...)
test("REQ-007 a pending sign-off card names client, deliverable, version, scope and takes focus", ...)
test("REQ-017 approving a release answers the card exactly once with an attributable confirmation", ...)
test("REQ-021 a failed export shows a plain cause and a working retry action", ...)
test("REQ-026 the design canvas panel moves from starting to running with a named status", ...)
test("REQ-028 takeover and release are keyboard-operable and update the panel status", ...)
test("REQ-011 the activity view lists working review runs and links to the starting message", ...)
test("REQ-012 every run status renders as text plus icon in light, dark and RTL", ...)
test("REQ-036 the asset library selects a version and opens it in the originating thread", ...)
test("REQ-038 the brief board moves a card by keyboard and is absent when the flag is off", ...)
test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)
test("REQ-057 Hebrew RTL mirrors the board and axe reports zero critical or serious findings", ...)
```

Behaviour without a REQ yet (sign-off framing, client-visible marker, day grouping) must be assigned an
id by the owner before implementation per the traceability gate — do not invent one in a test name.
Goldens under `spec/golden/ui/` stay byte-exact; a new seeded state means a new `REQ-<id>-<slug>.json`,
never a click path.

## 5. Best first slice

**Change A — the creative sign-off card.** GUESS, recommended: smallest, highest-value step, and the
only one that prevents a wrong deliverable/version being released through an ambiguous confirmation.
`threadsAnswer`, `Button`, `Badge` and `Tooltip` are installed; no new RPC, component or data model is
needed (the `CONFLICT` guard in `message-blocks.tsx` owns the double-answer case). Test-first: write
`REQ-007 a pending sign-off card names client, deliverable, version, scope and takes focus`, extend
`spec/golden/ui/REQ-007-pending-card.json`, then ship only the dominant consequence line, the explicit
"Release logo v3 to <client>" label, the client/version attribution, the text+icon client-visible
marker, keyboard operability and the polite announcement. Defer (B)–(E) until the owner assigns missing
REQ ids and approves scope.

**Decision needed from the owner**: confirm the sector framing, and whether REQ-038's feature flag is
in scope for this deployment before change (E) is scheduled.

Provenance: REQ ids from `spec/01-requirements.md`, RPC names from `apps/web/lib/api/rpc.ts`, the two
components (`thread/message-blocks.tsx`, `artifacts/artifacts-grid.tsx`) and rules from `docs/UI.md` /
`spec/08-ui-ux.md` — all CONFIRMED readings. Persona and jobs are a GUESS (no design-agencies sector in
the spec). No product code was modified.
