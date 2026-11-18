# UI proposal 100 — urban planning and development review

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #100. Read-only on product code;
only this file was written. Grounding read: `docs/UI.md`, `spec/08-ui-ux.md`, `spec/01-requirements.md`
(REQ ids), `apps/web/lib/api/rpc.ts` (RPC names) and two components: `shell/app-shell.tsx` and
`board/board-screen.tsx`. Charter held: Boma Bot only — no upstream brand, code, wording or assets;
BoardUI semantic tokens only, never raw palette or hex, classes merged with `cx()`; one design system,
no second; no SaaS (self-hosted on the department's machine — parcel records, zoning determinations,
demographic layers and public comments never leave the premises); robot-cat identity unchanged. Distinct
from #034 (buildings) and #043 (corridors): this is land use only — parcels, districts, overlays,
development review and the public realm.

## 1. Persona and top jobs-to-be-done

**Persona — Amal, a long-range planning and development-review lead at a municipal planning
department.** She works in a multilingual planning office (English, French, Arabic), alternating
between a desk console and a tablet at a site visit or a planning-commission hearing on a poor link.
Boma Bot runs on the city's own server: parcel geometry, zoning analysis, FAR/setback calculations,
demographic layers and public-comment summaries never leave the department. A zoning determination
issued for the wrong parcel, or a permit condition released to the wrong applicant, is a legal and
civic consequence — ambiguity is worse than latency.

Top jobs:
1. **Monitor** — see which zoning-check, site-suitability, FAR/setback and comment-summary runs are
   working, stalled or failed without hunting through a transcript; retry only failures.
2. **Decide** — review a bot-proposed zoning determination, permit condition or plan amendment and
   be certain of the exact parcel, ordinance section, effect and sequence before approving; never
   decide by colour alone.
3. **Record** — log a site observation (parcel id, measured FAR, height, setback, photo) from the
   tablet, attaching a note without losing the draft to a stray Enter.
4. **Triage** — move development cases (intake → review → hearing → decision) on a board, each
   naming its case key, assignee bot and last comment, with attached background runs visible.
5. **Publish** — keep versioned comprehensive plans, zoning maps and staff reports, and share a
   public-hearing notice or plan summary with residents who have no account.

## 2. Five concrete changes

All new copy is catalogued through `t(locale, key)` (`lib/i18n.ts`); nothing is hardcoded in JSX. Changes reuse vendored
BoardUI components and the `docs/UI.md` token map; bot values are untrusted text, never HTML. No new RPC, install or data model.

**A. Parcel and district context strip — `/app/:botId` thread header.** A compact, read-only band
above the transcript with chips for "parcel", "zoning district", "overlay" and "plan area", each with
a `Tooltip` for detail; calculated values (FAR, height, coverage) use `chart-1`…`chart-5` tokens
**plus** a printed number, never colour alone. Components: `Chip`/`Badge`
(`components/base/badges/chip.tsx`), `Tooltip`, `IconButton` (icons from `@remixicon/react` passed as
references). Tokens: surface `bg-background-secondary-default`, hairline `border-separator-border`,
text `text-text-primary`/`text-text-secondary`, icon `text-foreground-icon-secondary`, type
`text-caption-1-semibold`, joined with `cx()`. Existing RPCs: `botsGet`, `threadsHead`, `threadsMessages`.

**B. Zoning and permit approval card — `/app/:botId` pending card (`REQ-007`/`REQ-017`).** Make the
"what will happen" sentence dominant: exact parcel, ordinance section, FAR/height effect and the
sequence, plus a distinct confirm label ("Approve PERM-204 · Parcel 12-045" — not just "Approve").
Legally significant determinations use `border-border-error-default` + `text-text-error-primary`
**plus** a leading icon and the words "High impact" — never colour alone. Components: BoardUI
`Button`, `Badge`, `Tooltip`; card surface `bg-background-secondary-default`, `rounded-3xl`,
`border-separator-border`, focus `focus-visible:ring-border-focus-ring`. RPC `threadsAnswer` (already
present, exactly-once effect, with the `CONFLICT` guard already handled for double answers).
Announced once via the polite live region; the transcript stays readable and other cases reachable
(spec §8.9).

**C. Site-note composer — `/app/:botId` composer (`REQ-001`).** Extend the composer with an "attach
site note/photo" `IconButton` (`RiFileTextLine`) offering a parcel id, a measurement (FAR, setback,
height) or a site photo as a reply-style chip, reusing the existing `ReplyContext` banner and the
installed `file-upload` component. Keep Enter-to-send / Shift+Enter-newline exactly as today and keep
send disabled while an approval is pending (`awaitingApproval`). Component: BoardUI `IconButton` +
`Tooltip`; focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`, 44 px `.boma-target`,
icon `text-foreground-icon-primary`. RPC `threadsSend`; the attachment enters the same `value` state,
so no new transport. Parcel ids and ordinance citations stay LTR in `<bdi>`; measurements are
formatted with `Intl` and held as integers, never floats.

**D. Development-review triage — `/app` Activity view (`REQ-011`/`REQ-012`/`REQ-067`).** Extend
`ActivityScreen` with day grouping and named status chips (`queued`, `leased`, `running`,
`waiting_input`, `waiting_takeover`, `completed`, `failed`, `cancelled`) using `runStatusKey` as text
+ icon; pause/resume queued runs, retry only `failed`. Working runs sort to the top and update in
place; no decorative spinner. BoardUI `Button`, `Chip`; surfaces `bg-background-primary-default` and
`bg-background-secondary-default`, hairline `border-separator-border`, error row
`border-border-error-default` + `text-text-error-primary` + `text-foreground-icon-error`. RPCs:
`runsList`, `runsPause`, `runsResume`, `runsRetry`, `threadsHead`. Scheduled (background) runs appear
here, not in the transcript.

**E. Review board, plan library and public-hearing page — `/app/board` (`REQ-038`),
`/app/artifacts` (`REQ-036`) and `/p/:slugToken` (`REQ-037`).** Retitle the four flag-gated columns of
`board-screen.tsx` as intake → review → hearing → decision; each card shows case key, title, assignee
bot and last comment, and the detail dialog shows attached runs. Replace the raw `<input>`/`<select>`
there with the installed BoardUI `Input`/`Select` to honour the one-system rule. Components: installed
`Chip`/`Badge`, `Button`, `Input`, `Select`; surfaces `bg-background-primary-default`/
`bg-background-secondary-default`, `border-separator-border`, status text + icon. RPCs: `boardsGet`,
`ticketsCreate`, `ticketsMove`, `ticketsComment`, `botsList`. In the library, show versioned plans,
zoning maps and staff reports with a version selector, download and "open in thread" via
`artifactsListSpace`, `artifactsGet`, `artifactsListVersions`, reusing the bounded frame and
untrusted-byte rules of `artifact-viewer.tsx`; keep the robot-cat empty state. When the flag is off
the board route is absent, not locked. A compiled hearing notice or plan summary can be published
through `/p/:slugToken` with no session or product chrome beyond a small robot-cat footer; unknown,
revoked and expired tokens show one identical "not available" screen — same wording and timing — with
`noindex`, `no-referrer` and its own CSP.

## 3. Accessibility, i18n and RTL

- All visible strings go through `t(locale, key)` (English source; French/Hebrew translations; a
  missing key falls back to English, never a blank or the key). No string in JSX.
- `DirectionProvider` sets locale/direction; the boot script writes `lang`/`dir` together. Use logical
  utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Mirror the attach
  affordance, board columns and drawer motion via `useDirection()`; keep parcel ids, ordinance
  citations, zoning codes, URLs, dates and numbers LTR in `<bdi>`/`dir="auto"`, formatted with `Intl`
  (integer units for measurements, no floats).
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets via
  `.boma-target` for tablet/site use; token-driven visible focus; full keyboard path (approval
  decision, attach, retry/pause/resume, board moves via the focus-trapped dialog, version select);
  run status is text + icon, never colour alone (spec §8.9 vocabulary); 360 px width;
  `prefers-reduced-motion`; no animation on streaming content.
- The high-impact marker is text + icon, and the zoning/permit decision is announced once via the
  polite live region — the transcript is not a live region, so streaming tokens are not read aloud.
  FAR/height charts carry a printed value, never colour as the sole signal. Error copy is one sentence
  of what happened plus one of what to do; no parcel ids, case ids, stack traces or provider text on
  screen (correlation id behind "details" only). The robot-cat mark stays unchanged.

## 4. Playwright tests (REQ ids in the name)

```
test("REQ-001 the site-note composer sends one optimistic message that is not duplicated", ...)
test("REQ-007 a pending zoning card names parcel, ordinance, effect and sequence and does not block reading", ...)
test("REQ-011 the activity view groups planning runs by day and links back to the starting message", ...)
test("REQ-012 every run status renders as text plus icon in light, dark and RTL", ...)
test("REQ-017 answering the permit card reopens the composer and answers exactly once", ...)
test("REQ-021 a failed site-suitability run shows a plain cause and a working retry action", ...)
test("REQ-036 the plan library filters by type and bot and shows the version chain", ...)
test("REQ-038 the review board moves a case by keyboard and hides when the flag is off", ...)
test("REQ-037 the shared hearing notice renders without chrome; an invalid token shows the uniform screen", ...)
test("REQ-057 the planning screens mirror for Hebrew with a clean axe run", ...)
```
Behaviour above without a REQ yet (parcel strip, attach-note chip, high-impact marker, day grouping)
must be assigned a REQ id by the owner before implementation per the traceability gate — do not invent
one in test names.

## 5. Highest-impact first slice

**Change B — the zoning and permit approval card.** It is the cheapest and the only change that
directly reduces the risk of a determination or permit condition released to the wrong parcel through
an ambiguous confirmation: `threadsAnswer`, `Button` and `Badge` are installed, the semantic tokens
exist, and no new RPC, component install or data model is needed (`message-blocks.tsx` already owns the
`CONFLICT` guard). Test-first: write `REQ-007 a pending zoning card names parcel, ordinance, effect and
sequence` and extend `spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant effect
sentence, the explicit confirm label, the parcel attribution, the text+icon high-impact marker,
keyboard operability and the polite announcement. Defer (A), (C), (D), (E) until the owner assigns REQ
ids and approves.

Provenance: REQ ids verified against `spec/01-requirements.md`, RPC names from `apps/web/lib/api/rpc.ts`,
component structure from `app-shell.tsx`/`board-screen.tsx`, design rules from `docs/UI.md` and
`spec/08-ui-ux.md` — all CONFIRMED readings; the persona and jobs are a GUESS. No product code modified.
