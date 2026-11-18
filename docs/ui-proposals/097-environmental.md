# UI proposal 097 — environmental services (monitoring, compliance and remediation)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #097 — environmental services.
Read-only on product code; this file is the only write. Grounding (CONFIRMED): `docs/UI.md`,
`spec/08-ui-ux.md`, `spec/01-requirements.md` (REQ ids), `lib/api/rpc.ts`, `lib/use-translation.ts`
and `lib/run-status.ts`, plus two components read in full: `components/thread/message-blocks.tsx`
(`CardBlock`, `threadsAnswer`, single-answer guard) and `components/artifacts/artifacts-grid.tsx`
(filters, version chain, untrusted text). Other screen paths exist by directory listing; no upstream
brand, code, asset, wording or capture consulted or copied.

Charter (CONFIRMED): Boma Bot only; fresh design (ADR-015); owner-supplied robot-cat is the sole
identity art; no upstream brand/code/assets/copy; BoardUI semantic tokens only — no raw palette or
hex, every class list via `cx()`; one design system, no second library or lookalike; self-hosted —
no SaaS/cloud and no reading, sample, site coordinate, permit, client or report leaving the premises.

## 1. Persona and top jobs-to-be-done

**Persona — "the environmental compliance lead"**: an EHS / environmental-services supervisor at a
consultancy or industrial site, working in English, French and Arabic across a site-office display and
a field handheld. Bots ingest sensor and sampling data, watch permit limits and prepare exceedance
responses and regulator filings on the organisation's own network. She is measured on traceable
decisions: a discharge notice, setpoint or permit filing against the wrong outfall or on a colour cue
is a reportable breach and a chain-of-custody gap — ambiguity is worse than latency. The robot-cat mark
is the only identity art; no agency seal, lab or client logo is borrowed.

Top jobs:
1. **See the live picture** — active exceedances, parameter-vs-limit, open samples and pending permit
   actions in one strip above the transcript, without hunting the rail.
2. **Authorise response** — review a bot-proposed exceedance response, setpoint change, remediation
   dispatch or permit filing; know site, parameter, reading and limit, window and consequence before
   one click; never decide by colour alone.
3. **Keep chain of custody** — move samples, inspections and remediation tickets from logged through
   collected and analysed to verified/closed, with the history intact.
4. **Name run state** — state every sampling, calibration, exceedance-check and reporting run out loud
   with the §8.9 vocabulary; retry only failures, pause/resume queued work.
5. **Record and share** — keep monitoring logs, lab results, calibration certificates, impact
   assessments, permits and audit reports versioned; publish a compiled compliance summary by link.

## 2. Five concrete UI changes

All copy is a new key via `useTranslation()` → `t(locale, key)` (`lib/i18n.ts`); English source, fr/he
translations, fallback to English — nothing hardcoded. Class lists via `cx()`; colours ride a
`docs/UI.md` token. Bot/user values (readings, sample ids, site names, lab output) are untrusted text,
never HTML.

**(a) Monitoring and exceedance status strip — thread `/app/:botId` header.**
A read-only band above the transcript: named chips for active exceedances, parameter-vs-limit, open
samples and pending permit actions, plus a trend using `chart-1`…`chart-5` (+ `-active`). BoardUI
`Chip`/`Badge`, `Tooltip`, `IconButton`; surface `bg-background-secondary-default`, hairline
`border-separator-border`, text `text-text-secondary`/`text-text-primary`, icons
`text-foreground-icon-secondary`, type `text-caption-1-semibold`. RPCs (CONFIRMED): `botsGet`,
`threadsHead`, `threadsMessages`. Keep readings/site ids out of logs; chart geometry stays LTR.

**(b) Exceedance-response / remediation approval card — thread pending card (REQ-007/REQ-017).**
Extend `CardBlock` in `message-blocks.tsx`: make "what will happen" dominant with site, parameter,
reading and limit, window, and a distinct confirm label ("Issue discharge notice for Outfall 2", not
"Approve"). High-impact actions (discharge notice, setpoint, permit filing) pair
`border-border-error-default` + `text-text-error-primary` with an icon and the word "Regulatory
impact" — never colour alone. Keep the `threadsAnswer` single-answer/`CONFLICT` guard honest. BoardUI
`Button`, `Badge`, `Tooltip`; card `bg-background-primary-default rounded-3xl
border-border-button-default`, type `text-body-medium`/`text-caption-1-semibold`, focus
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`. Transcript stays readable, other threads
reachable, focus lands on the card; announce once via the polite live region (§8.9), not the transcript.
RPCs (CONFIRMED): `threadsAnswer`, `threadsReact`.

**(c) Sampling and remediation board — `/app/board` (REQ-038, feature-flagged).**
Reuse `BoardScreen`: columns read logged / collected / analysed / verified, with ticket key, site,
sample id, assignee bot and last comment; assigning queues bot work here, not in chat. Status is
text + icon, never colour alone. Prefer installed BoardUI `Input`/`Select` over raw fields via `cx()`;
tokens `bg-background-primary-default`, `border-border-button-default`, `text-text-primary`/
`text-text-secondary`, focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`. RPCs
(CONFIRMED): `boardsGet`, `botsList`, `ticketsCreate`, `ticketsMove`, `ticketsComment`. A 404 keeps
the route absent, not locked (§8.6).

**(d) Monitoring and compliance-run triage — activity view (REQ-011/REQ-012/REQ-064).**
Follow `ActivityScreen`: group sampling, calibration, exceedance-check and reporting runs by day with
prompt snippet, bot name, named status chip, duration and a link to the source; use the exact §8.9
vocabulary (`runStatusKey` text + icon). Working runs sort to top and update in place; failures get a
plain cause and retry, never a decorative spinner; skeletons while loading. BoardUI `Button`, `Chip`,
`table`; `bg-background-primary-default`, `border-separator-border`, `text-text-primary`/
`text-text-secondary`, error row `border-border-error-default` + `text-text-error-primary`. RPCs
(CONFIRMED): `runsList`, `runsRetry`, `runsPause`, `runsResume`, `notificationsStatus`. Exceedance
arrivals use the existing notifications policy surface.

**(e) Environmental records library and shared compliance report — `/app/artifacts` and
`/p/:slugToken` (REQ-036/REQ-037).**
Reuse `ArtifactsGrid`: name monitoring logs, lab results with chain-of-custody, calibration
certificates, impact assessments, permits and audit reports; filter by type/bot, search, show the
version chain and "open in thread". Content is untrusted — render as data, never HTML; keep the
robot-cat empty state. BoardUI `Input`, `Select`, `Chip`, `Button`; tokens
`text-text-primary`/`text-text-secondary`/`text-text-tertiary`, `bg-background-primary-default`,
`border-border-button-default`. RPCs (CONFIRMED): `artifactsListSpace`, `artifactsGet`,
`artifactsListVersions`. Publish a compiled compliance summary to a regulator/auditor with no session
and no chrome beyond a small robot-cat footer; unknown/revoked/expired tokens render one identical
"not available" screen — same words, shape and timing. `noindex`/`no-referrer` and its own CSP stay
backend concerns; transport (CONFIRMED): `GET /api/v1/pages/shared/{token}`.

## 3. Accessibility

WCAG 2.2 AA: zero critical/serious axe findings in light, dark and RTL; keyboard-only path for every
primary flow (approve/decline, board moves, pause/resume, retry, filters); visible
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target`; 360 px width;
`prefers-reduced-motion`; no animation on streaming content. Exceedance severity, custody state and
run status are text + icon (§8.9) — no colour-only signal. Focus lands on a pending card; the
unavailable screen is announced in identical text — no accessibility-tree leak. Tables carry
`<caption>`/`<th scope>`; error copy is one sentence of what happened plus one of what to do — no
reading, sample id, coordinate, stack trace or provider text (correlation id behind "details").

## 4. i18n and RTL

English source catalogue, fr/he translations; missing keys fall back to English. `DirectionProvider`
(`@/components/foundations/direction/direction`) gets an explicit BCP 47 locale; the boot script writes
`lang` and `dir` together. RTL mirrors the rail, board columns and drawer motion via `useDirection()`;
glyphs flip. Logical utilities (`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`).
Sample ids, parameter codes, readings, timestamps, URLs and OTP digits stay LTR in `<bdi>`/`dir="auto"`;
measured chart geometry is never mirrored.

## 5. Playwright tests (REQ id in each name)

- `test("REQ-001 sending a monitoring instruction creates one optimistic message that is not duplicated", ...)`
- `test("REQ-007 the exceedance card names site, parameter, reading, limit, window and consequence and takes focus without blocking reading", ...)`
- `test("REQ-017 regulatory-impact and deny answer the card exactly once with an attributable confirmation", ...)`
- `test("REQ-038 the sampling board shows text-and-icon custody columns and moves a ticket by keyboard", ...)`
- `test("REQ-038 the board route is absent, not locked, when the flag returns 404", ...)`
- `test("REQ-011 the activity view groups sampling and compliance runs by day and links to the source", ...)`
- `test("REQ-012 every compliance run status renders a distinct named text-and-icon chip", ...)`
- `test("REQ-036 the environmental records library filters by type and bot and shows the version chain", ...)`
- `test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)`
- `test("REQ-057 Hebrew RTL mirrors the board and axe reports zero critical or serious findings", ...)`

Goldens under `spec/golden/ui/` stay byte-exact; a new state gets a new `REQ-<id>-<slug>.json`
asserting seeded state, not a click path. New behaviour without a REQ (status strip, regulatory-impact
marker, day grouping) needs an owner-assigned id before code — do not invent one in a test name.

## 6. Best first slice

**Change (b), the exceedance-response / remediation approval card** — GUESS, recommended. Smallest,
highest-value step: one screen, no new transport (`threadsAnswer`/`threadsReact` exist), no new
component (BoardUI `Button`/`Badge`/`Tooltip` installed), no token re-tune, and an existing golden
(`spec/golden/ui/REQ-007-pending-card.json`) to extend. It prevents a discharge notice, setpoint or
permit filing being confirmed on a colour cue or against the wrong outfall. Test-first: write
`REQ-017 regulatory-impact and deny answer the card exactly once ...`, extend the REQ-007 golden,
then ship only the dominant consequence line, the explicit "Issue discharge notice for Outfall 2"
label and the text + icon regulatory-impact marker. Defer (a), (c)–(e) until REQ ids are assigned.

**Decision needed**: confirm the environmental-services framing and whether REQ-038's feature-flagged
board is in scope before (c) is scheduled.

Provenance: REQ ids from `spec/01-requirements.md`; RPCs from `lib/api/rpc.ts`; i18n/run vocabulary
from `lib/use-translation.ts`/`lib/run-status.ts`; structure from the two components read; rules from
`docs/UI.md`/`spec/08-ui-ux.md` — all CONFIRMED. Persona, jobs and changes are GUESS. Code untouched.
