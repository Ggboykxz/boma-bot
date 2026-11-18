# UI proposal 082 — fashion (collections, samples and wholesale)

Status: PROPOSAL. Sector framing, persona and treatments are GUESS (a sector lens, not in `spec/`);
file, component, token, RPC and REQ references are CONFIRMED (read this session). This file is the only write.

Sources (CONFIRMED): `docs/UI.md`, `spec/08-ui-ux.md`, `spec/01-requirements.md`,
`apps/web/lib/api/rpc.ts` (RPC names), `apps/web/lib/i18n.ts`, `assets/brand/boma-bot-head.svg`,
`apps/web/components/thread/message-blocks.tsx` (`CardBlock`) and `thread/thread-composer.tsx`.
Charter (CONFIRMED, `docs/UI.md` + AGENTS.md): fresh design (ADR-015), the owner-supplied robot-cat as
the sole identity; no upstream brand, code, assets or copy; BoardUI semantic tokens only — no raw
palette class or hex literal, every class list merged with `cx()`; one design system, no second library
or hand-built lookalike; self-host only (no SaaS/cloud) — sketches, unreleased looks, line sheets and
supplier terms never leave the studio.

## 1. Persona and top jobs

**Persona — "the collection lead"**: head of collections at an independent fashion label running Boma
Bot on the studio's own machine. She directs bots to draft lookbook copy, track sample sets, chase
suppliers and prepare wholesale line sheets; she reviews on a calibrated desktop and a phone backstage,
with English, French, Arabic and Hebrew colleagues. Unreleased garments, sketches and buyer terms never
leave the studio. She is measured on judgment, not speed: sending the wrong colourway to a buyer, or
signing off on a red/green cue, is a real loss — ambiguity, including a pretty swatch, is not evidence.
The robot-cat mark is the sole identity art; no label, foundry, marketplace or payment logo, no borrowed copy.

Top jobs:
1. **Approve a look** — know the exact collection, look number, garment, colourway and destination
   before a look or line sheet is published; never decide by colour alone.
2. **Track samples** — see the status of each sample set and size run from first pattern to fitting,
   with a status the lead can name out loud (§8.9 vocabulary).
3. **Direct from anywhere** — send a brief, attach a sketch or moodboard, and reply in-thread without
   losing the draft.
4. **Hand off a sample** — pass a fitting or tech-pack task between design, sampling and production
   bots with an attributable record.
5. **Publish and share** — keep a versioned asset library and release a compiled lookbook by link
   with no login, never exposing another label's or another buyer's work.

## 2. Five concrete UI changes

All new copy is a `lib/i18n.ts` key via `useTranslation` / `t(locale, key)`; no string in JSX. Every
class list goes through `cx()`; every colour rides the `docs/UI.md` token map. Bot- and supplier-produced
bytes are untrusted text or images, never markup; keep buyer names, prices and supplier ids out of logs
and out of another bot's view. Where `CardBlock` uses raw `<button>`/`<input>`, new work uses installed
BoardUI `Button`/`IconButton`/`Input` — no second design system, no lookalike.

**A. Look-release approval card — thread `/app/:botId` (REQ-007, REQ-017).** Extend `CardBlock` in
`message-blocks.tsx`: make the "what will happen" sentence dominant — collection, look number, garment,
colourway, size run and destination — with an explicit confirm label ("Publish look 12, SS drop, to the
wholesale line sheet", not just "Approve"). A high-impact release pairs `border-border-error-default`
+ `text-text-error-primary` with a leading icon and the words "Buyer-visible" — never colour alone; a
colourway is always its i18n *name* in text, never a palette swatch or hex. Replace raw buttons with
BoardUI `Button`, plus `Badge`/`Tooltip`; card `bg-background-secondary-default`, `rounded-3xl`,
`border-separator-border`, text `text-text-primary`/`text-text-secondary`, focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`. Existing RPC `threadsAnswer` (the
`CONFLICT` guard owns a double answer); `threadsReact` for reactions. Transcript stays readable, other
threads reachable, focus lands on the card; announce once via the polite live region (§8.9).

**B. Sample-status strip — `/app/:botId` thread header (REQ-007, REQ-012).** A compact read-only band
above the transcript with named chips for "In sampling", "At fitting", "Awaiting buyer", "Rework"; each
`Tooltip` (`components/base/tooltip/tooltip.tsx`) expands detail on focus, not hover alone. BoardUI
`Badge`/`Chip` (`components/base/badges/chip.tsx`), `Tooltip`, `IconButton`; surfaces
`bg-background-secondary-default`, hairline `border-separator-border`, text
`text-text-secondary`/`text-text-primary`, icons `text-foreground-icon-secondary`, type
`text-caption-1-semibold`. Status is text + icon, not colour (§8.9 vocabulary stays exact). Existing
RPCs `botsGet`, `threadsHead`, `threadsMessages`.

**C. Collection-aware composer — `/app/:botId` composer (REQ-001, REQ-008).** Extend `ThreadComposer`
with an "attach sketch" `IconButton` (`RiImageLine`) using the installed `file-upload` component and a
`Tooltip`; keep `@bot` mention, reply chip, Enter-to-send / Shift+Enter-newline, stop while a run is
active, and send disabled while `awaitingApproval`. BoardUI `Button`/`IconButton`, icons from
`@remixicon/react` as references, `text-foreground-icon-primary`, 44 px `.boma-target`, token focus
ring. Existing RPC `threadsSend`; the attachment enters the same draft state, so no new transport.
Look numbers, SKUs, dates and buyer codes stay LTR in `<bdi>` / `dir="auto"`.

**D. Sample handover in the group thread — `/app/g/:groupId` (REQ-010, REQ-067, REQ-068).** Reuse
`group-thread-screen.tsx` to show a dismissal-free banner naming outgoing and incoming bot with a named
status chip (e.g. design → sampling for a fitting); keep every message's bot attribution. BoardUI
`Button`, `Badge`, `Avatar` (`components/base/avatar/avatar.tsx`); surfaces
`bg-background-secondary-default`, `border-separator-border`, `text-text-secondary`, with
`text-text-error-primary` + text + icon for a failed handover. Existing RPCs `groupsGet`,
`groupsMessages`, `groupsReassign` (`runsPause`/`runsResume` if a run must pause).

**E. Lookbook library, sample board and buyer link — `/app/artifacts` (REQ-036), `/app/board`
(REQ-038, feature-flagged), `/p/:slugToken` (REQ-037).** Reuse `ArtifactsGrid` as a versioned look/tech-pack
library: name, type icon, size, owner bot, updated time, filter by type/bot, search by name, version chain
and "open in thread"; replace its raw `<select>`/`<input>` with installed BoardUI `Select`/`Input`, keep
the bounded frame and untrusted-byte rules. Retitle the flag-gated board columns to sketch → sampling →
fitting → approved via `boardsGet`, `ticketsCreate`, `ticketsMove`, `ticketsComment`; flag off keeps the
route absent, not locked (§8.6). Publish a compiled lookbook at `/p/:slugToken` with no chrome beyond a
small robot-cat footer; unknown, revoked and expired tokens render one identical "not available" document
— same words, shape and timing. Existing RPCs `artifactsListSpace`, `artifactsGet`,
`artifactsListVersions`, `artifactsRemove`.

## 3. Accessibility, i18n and RTL

- Every visible string goes through `t(locale, key)` (English source; fr/he translations; a missing key
  falls back to English, never blank or the raw key). No string in JSX.
- `DirectionProvider` (`@/components/foundations/direction/direction`) gets an explicit BCP 47 locale;
  the boot script writes `lang`/`dir` together. Logical utilities throughout (`ms`/`me`, `ps`/`pe`,
  `text-start`/`text-end`, `border-s`/`border-e`). Mirror the rail, drawer motion, board columns and
  handover arrows via `useDirection()`; keep SKUs, look numbers, prices, dates and URLs LTR in `<bdi>` /
  `dir="auto"`, formatted with `Intl`. Measured geometry and artwork are never mirrored.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px `.boma-target`;
  token-driven visible focus; full keyboard path (release decision, handover, attach, version select,
  board move); status and colourway are text + icon/name, never colour alone; 360 px width;
  `prefers-reduced-motion`; no animation on streaming content.
- The release decision is announced once via the polite live region — the transcript is not a live region,
  so streaming tokens are not read aloud. The unavailable screen uses identical text — no accessibility-tree
  leak. Error copy: one sentence of what happened plus one of what to do; no buyer name, price, stack trace
  or provider text on screen (correlation id behind "details").

## 4. Playwright tests (REQ id in each name)

```
test("REQ-001 the collection composer sends one optimistic message that is not duplicated", ...)
test("REQ-007 a pending look-release card names collection, look, garment, colourway and destination", ...)
test("REQ-017 approving a look-release answers the card exactly once with an attributable confirmation", ...)
test("REQ-008 a colourway is conveyed by its i18n name and never by colour alone", ...)
test("REQ-009 the sample-status strip marks read/unread and snooze brings a set back on time", ...)
test("REQ-011 the activity view lists working sample runs and links to the starting message", ...)
test("REQ-012 every run status renders as text plus icon in light, dark and RTL", ...)
test("REQ-010 the group thread attributes each fitting message and reassigns by keyboard alone", ...)
test("REQ-036 the lookbook library selects a version and opens it in the originating thread", ...)
test("REQ-038 the sample board moves a card by keyboard and is absent when the flag is off", ...)
test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)
test("REQ-057 Hebrew RTL mirrors the rail and board and axe reports zero critical or serious findings", ...)
```

Behaviour without a REQ yet (sample-status strip, buyer-visible marker, day grouping) must be assigned an
id by the owner before implementation — do not invent one in a test name (traceability gate). Goldens
under `spec/golden/ui/` stay byte-exact; a new seeded state means a new `REQ-<id>-<slug>.json`, never a
click path.

## 5. Best first slice

**Change A — the look-release approval card.** GUESS, recommended: smallest, highest-value step, and the
only one that prevents the wrong look, colourway or version reaching a buyer through an ambiguous
confirmation. `threadsAnswer`, `Button`, `Badge` and `Tooltip` are already installed; no new RPC,
component or data model is needed (the `CONFLICT` guard in `message-blocks.tsx` owns the double-answer
case). Test-first: write `REQ-007 a pending look-release card names collection, look, garment,
colourway and destination`, extend `spec/golden/ui/REQ-007-pending-card.json`, then ship only the
dominant consequence line, the explicit "Publish look 12 …" label, the attribution, the text+icon
buyer-visible marker, keyboard operability and the polite announcement. Defer (B)–(E) until the owner
assigns missing REQ ids and approves scope.

**Decision needed from the owner**: confirm the sector framing and whether REQ-038's feature flag is in scope before change (E).

Provenance: REQ ids from `spec/01-requirements.md`, RPC names from `apps/web/lib/api/rpc.ts`, the two
components, `lib/i18n.ts`, the brand asset and rules from `docs/UI.md`/`spec/08-ui-ux.md` — all CONFIRMED.
Persona and jobs are a GUESS (no fashion sector in the spec). No product code was modified.
