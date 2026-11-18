# UI proposal 093 — pharmaceuticals (drug safety, quality and regulatory affairs)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #093 — pharmaceuticals.
Read-only on product code; this file is the only write. Grounding (CONFIRMED, this session):
`docs/UI.md`, `spec/08-ui-ux.md`, `spec/01-requirements.md` (REQ ids), `apps/web/lib/api/rpc.ts`
(RPCs) and two components — `thread/message-blocks.tsx` (`CardBlock`, the pending-card path,
raw `<button>`/`<input>` and the text+icon error row) and `thread/thread-composer.tsx`
(React Aria `TextField`/`TextArea`, BoardUI `Button`, `cx()`, `awaitingApproval` send gate,
Enter-to-send). No upstream/original brand, code, assets or wording was consulted or copied.

Charter (CONFIRMED): Boma Bot only, fresh design (ADR-015), owner-supplied robot-cat as sole
identity — kept in the product-mark slot and empty states. BoardUI semantic tokens only, no raw
palette class or hex literal, every class list merged with `cx()`; one design system, no second
library and no hand-built lookalike of an installed component; self-hosted on the company's own
machine — no SaaS/cloud destination by default, and no safety case, batch record, trial datum or
dossier leaves the premises; patient/subject identifiers are untrusted data, never markup.

## 1. Persona and top jobs-to-be-done

**Persona — "the safety and quality lead":** a pharmacovigilance / clinical-quality manager at a
pharmaceutical company, running Boma Bot on the company's own hardware. She directs bots to
triage adverse-event reports, check batch and stability records, draft deviation narratives and
assemble regulatory dossiers, reviewing on a workstation and a phone, in English, French, Arabic
and Hebrew. A missed serious signal or a batch released on a colour cue is a patient-safety and
compliance event, so ambiguity is worse than latency. The robot-cat mark is the only identity art;
a molecule image or certificate is user content, never brand art.

Top jobs:
1. **Triage a safety signal** — read a bot's one-screen case summary (event, seriousness, dose,
   dechallenge) without hunting through the rail; know what is still open.
2. **Release or hold a batch** — confirm what a proposed disposition will do before approving;
   never decide on colour alone, and keep an attributable record.
3. **Name the work** — state every deviation, stability and submission run out loud (§8.9); retry
   only failures, pause queued work.
4. **Capture a report** — log a case note or attach a protocol/lab file fast, without losing the
   draft to Enter or a card arrival, on a poor network.
5. **File and share** — keep dossier versions, publish a review page by link with no login, and
   never leak another study's or another company's data.

## 2. Five concrete UI changes

All new copy is a `lib/i18n.ts` key via `useTranslation()` → `t(locale, key)` (English source; fr/he
translations; a missing key falls back to English, never blank or raw key); no string in JSX. Every
class list goes through `cx()`; every colour rides the `docs/UI.md` token map; dark mode rides the
tokens. Bot-produced bytes are untrusted text or images, never markup.

**(a) Safety-signal triage card — thread `/app/:botId` (REQ-007, REQ-012).** Extend `CardBlock`
(`message-blocks.tsx`): render a bot-produced case summary as a named card with a seriousness chip
("Serious", "Non-serious", "Unassessed") using text + icon + token, never colour alone. A serious
signal pairs `border-border-error-default` + `text-text-error-primary` with `RiAlertLine` and the
word "Serious" — never colour alone. Replace the raw `<button>`/`<input>` in `CardBlock` with the
installed BoardUI `Button`/`Input` (one-system rule); surface `bg-background-secondary-default`,
`rounded-3xl`, `border-separator-border`, text `text-text-primary`/`text-text-secondary`, focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`, 44 px `.boma-target`. Existing RPCs:
`threadsMessages`, `threadsAnswer`, `threadsReact`. Announce once via the polite live region (§8.9);
the transcript is not a live region. Keep subject ids out of the DOM and logs where possible.

**(b) Consequence-first batch disposition card — thread `/app/:botId` (REQ-017, REQ-007).** Make
the "what will happen" sentence dominant — batch/lot, product, action (release / hold / rework) —
with an explicit label ("Place lot <id> on hold", not just "Approve"). A release or irreversible
disposition pairs `border-border-error-default` + `text-text-error-primary` with a leading icon and
"High impact" — never colour alone. Installed BoardUI `Button`/`Badge`/`Tooltip`, tokens
`bg-background-primary-default`, `border-border-button-default`, `text-text-primary`. Existing RPC:
`threadsAnswer` (the `CONFLICT` guard in `CardBlock` owns the double answer). The card takes focus
but does not block reading or other threads (spec §8.3); send waits while approval is pending
(`awaitingApproval` in `thread-composer.tsx`).

**(c) Deviation / OOS run triage — `/app` Activity view (REQ-011, REQ-012, REQ-021).** Group
deviation, stability and submission runs by day with prompt snippet, bot name and a named status
chip (queued/leased/running, waiting_input/waiting_takeover, completed/failed/cancelled — §8.9);
sort failing and waiting to the top, retry only `failed`, pause/resume queued work. Installed
BoardUI `Tabs` as status filter, `Badge`/`Chip`, `Button`; keep existing pause/resume/retry actions.
Surfaces `bg-background-primary-default`, hairline `border-separator-border`, text
`text-text-primary`/`text-text-secondary`. Existing RPCs: `runsList`, `runsRetry`, `runsPause`,
`runsResume`, `threadsHead`. Failures get one sentence of cause plus one of what to do, correlation
id behind "details" — never a stack trace, subject id or provider text.

**(d) Report-and-file composer — thread composer and transcript (REQ-001, REQ-002, REQ-036).**
Extend `ThreadComposer` with an "attach report" `IconButton` (`RiFileTextLine`) and a named file
`Chip`; keep the React Aria `TextField`/`TextArea`, `@bot` mention, reply chip, Enter-to-send /
Shift+Enter-newline, stop while a run is active, and send-disabled-on-empty. Block/rail views as
today, classes via `cx()`. Render attached protocols and lab reports as bounded file/image blocks
in the viewer frame with real `alt` — blob/data URL only, never app HTML. Installed BoardUI
`Button`/`IconButton`/`Chip`; surface `bg-background-primary-default`, icon
`text-foreground-icon-primary`, token focus ring. Existing RPCs: `threadsSend`, `threadsMessages`,
`artifactsGet`. Strip EXIF/GPS; never expose a subject identifier in DOM, `alt` or logs.

**(e) Dossier library and shared review link — `/app/artifacts` + `/p/:slugToken` (REQ-036,
REQ-037).** Reuse `ArtifactsGrid` as a versioned dossier register: filter by type/bot, search by
name, show the protocol → result → submission chain, "open in thread", and version selector; keep
the bounded frame and untrusted-byte rules. Replace any raw `<select>`/`<input>` with installed
BoardUI `Select`/`Input`. Existing RPCs: `artifactsListSpace`, `artifactsGet`,
`artifactsListVersions`, `artifactsRemove`. Publish a review page with no session and no chrome
beyond a small robot-cat footer; unknown, revoked and expired tokens render one identical
"not available" document — same words, shape and timing. `noindex`/`no-referrer` and its CSP stay
backend concerns.

## 3. Accessibility, i18n and RTL

- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; keyboard-only path
  for every primary flow (card decision, status filter, pause/retry, attach, version select);
  `focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target`; 360 px;
  `prefers-reduced-motion`; no animation on streaming content.
- Clinical seriousness and disposition are always text + icon + token, never colour alone; the
  card effect is announced once via the polite live region — the transcript is not a live region
  (§8.9), so streaming tokens are not read aloud.
- Every user-visible string goes through `t(locale, key)`; no hardcoded copy, English fallback.
  `DirectionProvider` (`@/components/foundations/direction/direction`) gets an explicit BCP 47
  locale; the boot script writes `lang`/`dir` together. RTL mirrors rail, board columns and drawer
  motion via `useDirection()`; arrows flip. Logical utilities (`ms`/`me`, `ps`/`pe`, `text-start`/
  `text-end`, `border-s`/`border-e`). Subject ids, lot numbers, dates, doses and URLs stay LTR in
  `<bdi>`/`dir="auto"` via `Intl`; a measurement's physical geometry is never mirrored.
- Self-host only: no SaaS destination, no clinical datum in a hover-only tooltip, nothing clinical
  written to logs. Error copy is one sentence of what happened plus one of what to do (§8.9).

## 4. Playwright tests (REQ id in each name)

- `test("REQ-007 a safety-signal card names seriousness, takes focus and does not block reading", ...)`
- `test("REQ-012 every run status renders a distinct text-plus-icon chip with the right terminal action", ...)`
- `test("REQ-017 a batch disposition names lot, product and action and the impact marker uses text+icon", ...)`
- `test("REQ-011 activity sorts failed and waiting deviation runs to the top", ...)`
- `test("REQ-021 a failed submission shows a plain-language cause and a working retry", ...)`
- `test("REQ-001 attaching a report sends one optimistic message that is not duplicated", ...)`
- `test("REQ-036 the dossier register filters by type and bot and shows the version chain", ...)`
- `test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)`
- `test("REQ-057 Hebrew RTL mirrors the rail and axe reports zero critical or serious findings", ...)`
- Goldens under `spec/golden/ui/` stay byte-exact; a new seeded state means a new
  `REQ-<id>-<slug>.json`, never a click path. Behaviour without a REQ yet (seriousness tiers, impact
  marker, attach button) needs an owner-assigned id before code — do not invent one in a test name.

## 5. Best first slice

**Change (a), the safety-signal triage card** — GUESS, recommended: one screen, no new transport
(`threadsMessages`/`threadsAnswer` already exist), no new component (BoardUI `Button`/`Badge`/
`Chip` are installed per `docs/UI.md`), no token re-tune. It directly reduces the risk of
misreading a serious signal on a colour cue. Test-first: write `REQ-007 a safety-signal card names
seriousness…`, extend `spec/golden/ui/REQ-007-pending-card.json`, add an axe assertion (light, dark,
RTL). Ship only the named seriousness chip, the text+icon serious marker, the Button/Input migration
in `CardBlock`, keyboard path and polite announcement. Defer (b)–(e) until the owner assigns the
missing REQ ids and approves scope.

Provenance: REQ ids from `spec/01-requirements.md`, RPCs from `apps/web/lib/api/rpc.ts`, the two
listed components and rules from `docs/UI.md`/`spec/08-ui-ux.md` — all CONFIRMED. Persona and jobs
are a GUESS (no pharmaceuticals sector in the spec). No product code was modified.
