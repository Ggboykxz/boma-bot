# UI proposal 028 — food and beverage (batch, cold-chain and supplier desk)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #028 — food and beverage.
Read-only on product code; this file is the only write. Grounding read this session (CONFIRMED):
`docs/UI.md`, `spec/08-ui-ux.md`, `apps/web/lib/api/rpc.ts` (RPC names), and two components —
`apps/web/components/thread/thread-composer.tsx` (`ThreadComposer.awaitingApproval`) and
`apps/web/components/artifacts/artifacts-grid.tsx` (`ArtifactsGrid`). REQ ids and golden names
checked against `docs/UI.md` and `spec/golden/ui/`. No upstream code, brand, assets or wording used.

Charter (CONFIRMED): Boma Bot only, fresh design (ADR-015), the owner-supplied robot-cat as the sole
identity; no upstream brand/code/assets/copy; BoardUI semantic tokens only — no raw palette class or
hex literal, classes merged with `cx()`; one design system, no second library or lookalike; self-hosted
only (no SaaS — no batch, recipe, supplier or customer data leaving the operator's machine).

## 1. Persona and top jobs-to-be-done

**Persona — "the QA and production coordinator"**: a quality manager at a food/beverage maker who runs
Boma Bot on the company's own machine. She watches batches from raw ingredient to released lot, chases
cold-chain excursions, reviews supplier and formulation changes, and keeps certificates, labels and
spec sheets audit-ready. Staff work in English, French and Arabic. An approval on a colour cue or an
untraceable lot is a recall and a real loss, so legible state and an explicit "what will happen" beat a
fast screen. No supplier, regulator or certification logo is borrowed — the robot-cat is the only art.

Top jobs:
1. **Track** — name every batch/lot run out loud (working / paused / terminal vocabulary, spec §8.9),
   never by colour alone.
2. **Authorise** — approve a bot-drafted supplier, ingredient or label change knowing exactly what is
   released and to whom, before it takes effect.
3. **Trace** — tie a certificate of analysis, spec sheet or label to its originating run and version,
   without exposing internals.
4. **Handle excursions** — spot a cold-chain or delayed-shipment failure, read a plain-language cause,
   then retry, reassign or take over the legacy ERP when manual entry is needed.
5. **Inform** — publish a supplier-facing lot/spec status page by link, with no login and no leakage of
   unrelated batches.

## 2. Five concrete UI changes

All copy is a new `lib/i18n.ts` key via `useTranslation` / `t(locale, key)`; no string in JSX. Every
class list goes through `cx()`; every colour rides the `docs/UI.md` token map.

**(a) Lot/batch context strip — `/app/:botId` thread header.** A read-only band above the transcript
naming the lot stage (received / in process / awaiting release / released) with a `Tooltip` carrying
batch code, expiry and next QA check. Components: `Badge`/`Chip`, `Tooltip`, `IconButton` (icons from
`@remixicon/react` as references). Tokens: `bg-background-secondary-default`, `border-separator-border`,
`text-text-primary`/`text-text-secondary`, `text-foreground-icon-secondary`, `text-caption-1-semibold`.
RPCs (CONFIRMED): `botsGet`, `threadsHead`, `runsList`. Bot values are untrusted text, never HTML;
batch codes and dates stay LTR in `<bdi>`.

**(b) Supplier / ingredient approval card — `/app/:botId` pending card (REQ-007, REQ-017).** Make the
"what will happen" sentence dominant: exact supplier, ingredient, lot and effect, with an explicit
confirm label ("Release Lot L-204 to supplier X", not just "Approve"). Allergen/recall-risk changes
pair `border-border-error-default` + `text-text-error-primary` with an icon and the words "High impact"
— never colour alone. Components: BoardUI `Button`, `Badge`, `Tooltip`; card `bg-background-primary-default`,
`rounded-3xl`, `border-border-button-default`, focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`.
RPCs (CONFIRMED): `threadsAnswer` (exact-once, keeps the `CONFLICT` guard), `threadsReact`. Reuse
`ThreadComposer`'s `awaitingApproval` prop so the transcript stays readable and other threads reachable;
announce the card once through the polite live region (§8.9), not the transcript.

**(c) Cold-chain exception ledger and ERP takeover — `/app` activity + `/app/:botId` right panel
(REQ-011, REQ-012, REQ-021, REQ-068, REQ-026/028/029).** Follow `ActivityScreen`: a failed/excursion row
shows one sentence of cause plus one of what to do, a working `runsRetry`, and `runsPause`/`runsResume`
and a Reassign action for a delegated check; correlation id behind a "details" `Tooltip`, never a stack
trace or supplier id. In the computer panel name each state (starting, running, awaiting takeover,
released) and say "Leaving this screen pauses nothing — the handoff record stays open"; takeover and
release are explicit and attributable. Components: `Button`, `Badge`, installed spinner/skeleton; text
status + icon via `lib/run-status.ts`. RPCs (CONFIRMED): `runsList`, `runsRetry`, `runsPause`,
`runsResume`, `groupsReassign`, `computerStatus`, `computerTakeover`, `computerRelease`,
`computerOpenScreen`. Mirror drawer motion in RTL; leave physical screen coordinates untouched.

**(d) Compliance documents library — `/app/artifacts` (REQ-036).** Reuse `ArtifactsGrid`: certificates
of analysis, spec sheets, labels and HACCP records with type/bot filters and name search; the viewer shows
the version chain, download and "open in thread". Artifact content is untrusted — render as data, never
HTML in the document. Keep the robot-cat empty state when no documents exist. Components: `Input`,
`Select`, `Chip`, `Button`; `bg-background-full`, `bg-background-primary-default`,
`border-border-button-default`, `text-text-primary`/`text-text-secondary`/`text-text-tertiary`,
`text-foreground-icon-secondary`. RPCs (CONFIRMED): `artifactsListSpace`, `artifactsGet`,
`artifactsListVersions`, `botsList`.

**(e) Production lot board and supplier-facing spec page — `/app/board` (REQ-038) and `/p/:slugToken`
(REQ-037).** Reuse `BoardScreen`: flag-gated `COLUMNS` read as received / in process / in QC / released,
each card showing lot key, title, assignee bot and last comment; the named detail panel shows attached QA
runs. With the flag off the route is absent, not locked (§8.6). The shared page carries no product chrome
beyond a small robot-cat footer; unknown, revoked and expired tokens render one identical "not available"
document — same words, shape and timing. Components: `Button`, `Input`, `Select`, `Chip`, `Tooltip`;
`bg-background-full`, `border-separator-border`, text + icon status. RPCs (CONFIRMED): `boardsGet`,
`botsList`, `ticketsCreate`, `ticketsMove`, `ticketsComment`; transport `GET /api/v1/pages/shared/{token}`.

## 3. Accessibility

WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; a keyboard-only path for
every primary flow (release, retry, reassign, takeover, board move, version select); visible
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`; ≥44 px `.boma-target`; 360 px width;
`prefers-reduced-motion` honoured; no animation on streaming content. Lot status and high-impact markers
are text + icon using the exact §8.9 vocabulary (queued/leased/running, waiting_input/waiting_takeover,
completed/failed/cancelled). Tables carry `<caption>` and `<th scope>`; focus lands on a pending card;
the uniform unavailable screen is announced in identical text — no leak through the accessibility tree.
Error copy is one sentence of what happened plus one of what to do — no lot id, batch data, stack trace or
provider text on screen (correlation id behind "details" only).

## 4. i18n and RTL

English is the source catalogue, fr/he translations, missing keys fall back to English — never blank or
the raw key. `DirectionProvider` (`@/components/foundations/direction/direction`) gets an explicit BCP 47
locale; the boot script writes `lang` and `dir` together. RTL mirrors the board columns, the rail and the
drawer motion via `useDirection()`; navigation arrows flip. Logical utilities throughout (`ms`/`me`,
`ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`). Lot keys, batch codes, dates, money, URLs and
OTP digits stay LTR in `<bdi>`/`dir="auto"`; money in integer minor units via `Intl` (ADR-023).

## 5. Playwright tests (REQ id in each name)

- `test("REQ-001 the production composer sends one optimistic message that is not duplicated", ...)`.
- `test("REQ-007 a pending supplier-release card takes focus and does not block reading", ...)`.
- `test("REQ-009 the cold-chain divider moves and the snooze shelf returns an alert on time", ...)`.
- `test("REQ-011 the activity ledger names each batch run state with a plain caption", ...)`.
- `test("REQ-012 every run status renders a distinct named chip with the right terminal action", ...)`.
- `test("REQ-017 a high-impact allergen release names supplier, lot, consequence and answers once", ...)`.
- `test("REQ-021 a cold-chain excursion shows a plain-language cause and a working retry", ...)`.
- `test("REQ-026 the ERP handoff panel names starting, running, takeover and released states", ...)`.
- `test("REQ-028 takeover and release announce their status through the polite live region", ...)`.
- `test("REQ-036 the compliance-document viewer selects a version and opens it in its thread", ...)`.
- `test("REQ-038 the lot board moves a ticket by keyboard, absent when the flag returns 404", ...)`.
- `test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)`.
- `test("REQ-057 Hebrew RTL mirrors the board and axe reports zero critical or serious findings", ...)`.

New behaviour without a REQ yet (lot context strip, high-impact allergen marker, excursion caption) must
be assigned an id by the owner before code — do not invent one in a test name per the traceability gate.

## 6. Best first slice

**Change (b), the supplier/ingredient approval card** — GUESS, recommended. Smallest and highest-value:
one screen, no new transport (`threadsAnswer`/`threadsReact` exist), no new component (BoardUI
`Button`/`Badge`/`Tooltip` are installed) and no token re-tune. It directly prevents a risky allergen or
supplier release being approved on a colour cue. Test-first: write `REQ-017 a high-impact allergen release
names supplier, lot, consequence and answers once`, extend `spec/golden/ui/REQ-007-pending-card.json`,
then ship only the dominant consequence line, the explicit confirm label, the text+icon high-impact
marker, keyboard operability and the polite announcement. Defer (a), (c), (d), (e) until the owner assigns
any missing REQ ids and approves scope.

Provenance: RPC names and the `CONFLICT` guard CONFIRMED from `apps/web/lib/api/rpc.ts`; component
structure (`ThreadComposer.awaitingApproval`, `ArtifactsGrid`) CONFIRMED by reading the files this session;
REQ ids and golden names CONFIRMED from `docs/UI.md` and `spec/golden/ui/`. The persona, its jobs and the
food/beverage mapping are GUESS (proposal only — no such sector or REQ exists in the spec). No product code
was modified.
