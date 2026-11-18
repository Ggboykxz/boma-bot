# UI proposal 074 — NGOs (humanitarian aid, development, charities, advocacy)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #074 — NGO lens. Read-only on
product code; this file is the only write.

Grounding (CONFIRMED, read this session): `docs/UI.md`, `spec/08-ui-ux.md` (§8.6, §8.8, §8.9),
`spec/01-requirements.md`, `spec/03-api-contract.md`, `apps/web/lib/api/rpc.ts`,
`apps/web/lib/use-translation.ts`, and two components: `apps/web/components/board/board-screen.tsx`
(feature-flagged case board, `role="dialog"` + hand-rolled focus trap, raw inputs, `bg-black/40`) and
`apps/web/components/thread/notifications-panel.tsx` (push policy, destinations, raw controls, two
hardcoded strings). No upstream brand, code, asset, wording or capture was consulted or copied; this
lens is distinct from #071 (government records) and #070 (physical security).

Charter (CONFIRMED): Boma Bot only; the owner-supplied robot-cat is the sole identity art. BoardUI
semantic tokens only — no raw palette class or hex/oklch literal — composite type utilities, class
lists merged with `cx()`, dark mode rides the tokens. One design system, no second library or
lookalike. Self-hosted on NGO infrastructure — **no SaaS and no beneficiary, case, donor or field
datum leaving the network**. Gaps close with already-installed BoardUI components.

## 1. Persona and persona jobs

**Persona — "the field coordination lead"**: an NGO programme manager running a self-hosted Boma Bot
on NGO hardware, coordinating caseworkers, volunteers and bots across case intake, grant tasks, field
reports and donor impact summaries. Desktop-first in an office, on a phone in the field; works in
English/French/Arabic. Measured on *do no harm*: attributable decisions, beneficiary confidentiality,
privacy-preserving reporting. An alert naming a beneficiary, an approval on a colour cue, or an impact
page leaking a case detail is a safeguarding incident — ambiguity is worse than latency. The robot-cat is the only identity art; no agency, donor or flag logo is borrowed.

Jobs:
1. **Triage cases and grant tasks** — see what is queued, active, awaiting a human or stuck, using codes
   not names, without scrolling a transcript.
2. **Approve beneficiary-affecting actions** — know the exact case code, target, amount and requester
   of a bot-proposed spend or data action before one click commits it; never decide by colour alone.
3. **Alert field teams safely** — send content-free waiting/failed alerts to volunteers, with a daily
   cap, so no push reveals a beneficiary, diagnosis or location.
4. **Keep field-report evidence** — file, version and retrieve reports, opening each in the thread where
   it was produced; never render report HTML and never put case data in logs or tooltips.
5. **Publish impact without exposing cases** — share a compiled donor/impact page by link with no
   session, as text, with one identical unavailable document for every bad token.

## 2. Five concrete UI changes

All new copy is a key in `lib/i18n.ts` via `useTranslation()` (`apps/web/lib/use-translation.ts`);
nothing is hardcoded in JSX. Every colour rides the `docs/UI.md` token map, type uses composite
utilities, class lists go through `cx()`, icons from `@remixicon/react` as component references.
Beneficiary names, donor details and field values are untrusted text/bytes, never HTML.

**(a) Case & grant task board — `/app/board` (`REQ-038`), grounded in `board-screen.tsx`.** Reuse the
four columns as case stages (intake → assessing → in progress → closed) and the ticket card as a case
or grant task; show a **case code**, title, assignee bot and comment count — never a beneficiary name.
404 = route absent, not a locked screen (spec §8.6). CONFIRMED gap: the create form uses raw
`<input>`/`<textarea>`/`<select>` and the dialog hand-rolls a focus trap with `bg-black/40` — replace
with installed BoardUI `Input`, `Select` and modal/drawer on `bg-background-full`/
`bg-background-primary-default` + `border-border-button-default`. Tokens: `text-text-primary`/
`text-text-secondary`/`text-text-tertiary`, `bg-background-secondary-default`,
`border-separator-border`, `text-text-error-primary` + icon. RPCs (CONFIRMED): `boardsGet`,
`ticketsCreate`, `ticketsMove`, `ticketsComment`, `botsList`.

**(b) Content-free field alerts — thread panel (`REQ-064`, `REQ-065`), grounded in
`notifications-panel.tsx`.** Keep the browser-gated subscribe flow, per-bot waiting/completed policy
and space-level Slack/Teams destinations, but enforce that a push carries no case or beneficiary
detail — only "a case is waiting" — and keep the server-side per-(member, bot) daily cap visible.
CONFIRMED gap: raw checkbox/number/select inputs plus hardcoded `aria-label="Remove destination …"`
and placeholder `"https://"` — swap raw controls for installed BoardUI `Checkbox`/`Select`/`Input`
and route both strings through `t()`. Keep `role="status"`/`role="alert"` and `aria-pressed`.
Tokens: `bg-background-secondary-default`, `border-separator-border`, `text-text-secondary`,
`text-text-error-primary`, focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`. RPCs
(CONFIRMED): `notificationsStatus/Subscribe/Unsubscribe/PolicyGet/PolicySet`,
`notificationsDestinationsList/Create/Delete`.

**(c) Beneficiary-affecting approval card — thread `/app/:botId` (`REQ-007`, `REQ-017`, `REQ-018`).**
Make the "what will happen" sentence dominant: exact case code, effect, amount in integer minor units
via `Intl`, and a named requester; the confirm label says the action ("Approve a spend of 40 credits
on case CS-114"), not just "Approve". An over-cap or high-impact action pairs
`border-border-error-default` + `text-text-error-primary` with an icon and "High impact" — never colour alone. Keep the
`threadsAnswer` double-answer guard and one-click decision. BoardUI `card`/`button`/`badge`; surface
`bg-background-primary-default rounded-3xl border-border-button-default`, type `text-body-medium`/
`text-caption-1-semibold`. The transcript stays readable, other threads reachable, focus lands on the
card; announce once in the polite live region (spec §8.9) — the transcript is not a live region.
RPCs (CONFIRMED): `threadsAnswer`, `threadsReact`. Golden `spec/golden/ui/REQ-007-pending-card.json`
to extend.

**(d) Versioned field-report library — `/app/artifacts` (`REQ-036`).** Grid newest first, filter by
type and owner bot, search by name; card shows version, case code, size, owner, date; viewer keeps
version selector, download, "open in thread", copy. Content is untrusted: never inject HTML (spec
§8.5). Tokens: `bg-background-primary-default`, `border-border-button-default`,
`bg-background-secondary-default`, `text-text-tertiary`, `text-text-error-primary`. RPCs (CONFIRMED):
`artifactsListSpace`, `artifactsGet`, `artifactsListVersions`. Labels via `t()`.

**(e) Public impact page — `/p/:slugToken` (`REQ-037`, `REQ-039`).** No session, no chrome beyond a
robot-cat footer; compile donor impact data as text/table/chart blocks, never markup, with
`safeLink`/`safeImage`. Unknown, revoked and expired tokens render **one** identical "not available"
document — same words, shape, timing, no case leak through the accessibility tree. `noindex`/
`no-referrer`; links LTR in `<bdi>`. Transport (CONFIRMED): `GET /api/v1/pages/shared/:token` with
`credentials: "omit"` (spec/03-api-contract.md §3). Tokens: `bg-background-full`, `border-border-table`,
`border-separator-border`, `text-text-primary`/`text-text-secondary`. No upstream wording reused.

## 3. Accessibility, i18n and RTL

WCAG 2.2 AA: zero critical/serious axe findings in light, dark and RTL; ≥44 px `.boma-target`;
token-driven focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`; a keyboard path for
every primary flow (board card/detail/state move, approval decision, alert toggle, destination
add/remove, version select); 360 px width; `prefers-reduced-motion` honoured; no animation on streaming
content. Case stage, run status and high-impact marker are text + icon, never colour alone. The dialog
gets a real BoardUI focus trap; tables carry `<caption>`/`<th scope="col">`. Error copy is one sentence
of what happened plus one of what to do — no case id, beneficiary name, donor detail, stack trace or
provider text (correlation id behind "details"). The unavailable page is announced in identical text.

English is the source catalogue, fr/he translations; a missing key falls back to English — never a
blank or a bare key. `DirectionProvider` (`@/components/foundations/direction/direction`) takes an
explicit BCP 47 locale; the boot script writes `lang` and `dir` together. Use logical utilities
(`ms`/`me`, `ps`/`pe`, `text-start`/`text-end`, `border-s`/`border-e`) and `useDirection()` to mirror
board columns, cards and drawer motion for `he` (`REQ-057`); preserve physical chart coordinates.
Case codes, grant refs, amounts, emails, URLs and timestamps stay LTR in `<bdi>`/`dir="auto"`; money
in integer minor units via `Intl`.

## 4. Playwright tests (REQ id in each name)

- `test("REQ-038 the board shows case codes, not beneficiary names, and is absent when flag is off", ...)`
- `test("REQ-038 the case dialog traps focus, closes on Escape and moves state by keyboard", ...)`
- `test("REQ-064 a waiting case produces a content-free alert with a working daily cap", ...)`
- `test("REQ-007 the approval card names case code, amount, effect and requester without blocking reading", ...)`
- `test("REQ-017 a high-impact spend is marked by text and icon and answers the card exactly once", ...)`
- `test("REQ-018 a spend above the cap shows the refusal reason and is not paid before approval", ...)`
- `test("REQ-036 the report library selects a version, opens its thread and never injects HTML", ...)`
- `test("REQ-037 unknown, revoked and expired tokens render the identical unavailable document", ...)`
- `test("REQ-057 Hebrew RTL mirrors the board columns and axe reports zero critical or serious findings", ...)`

Behaviour without a REQ yet (e.g. the "do no harm" copy rule) needs an owner-assigned id before code —
do not invent one in a test name. Goldens under `spec/golden/ui/` stay byte-exact; new states get a new
`REQ-<id>-<slug>.json`.

## 5. Best first slice

**Change (b), the content-free field alerts** — GUESS, recommended. Read this session; it already wires
all `notifications.*` RPCs with tokens and `t()` keys — no new transport, model, token re-tune, REQ id
or install. One contained change closes a safeguarding gap: keep pushes content-free under the daily
cap, swap raw checkbox/number/select for installed BoardUI `Checkbox`/`Select`/`Input`, move the two
hardcoded strings into the catalogue — all inside REQ-064/065. Test-first: write `REQ-064 a waiting
case produces a content-free alert with a working daily cap` before code. Defer (a), (c), (d), (e)
until scope is approved; (c) needs the existing REQ-007 golden extended.

**Decision needed**: confirm the NGO/humanitarian framing (distinct from #071/#070) and that REQ-064 covers the "content-free alert" copy rule before it is asserted.

Provenance: REQ ids CONFIRMED from `spec/01-requirements.md`; RPCs from `apps/web/lib/api/rpc.ts`;
endpoint from `spec/03-api-contract.md`; component structure CONFIRMED by reading `board-screen.tsx` and
`notifications-panel.tsx`; design rules CONFIRMED from `docs/UI.md` and `spec/08-ui-ux.md`. Persona,
jobs, mapping, changes and slice are GUESS (no NGO sector or REQ exists in the spec). No upstream asset
used; no product code modified.
