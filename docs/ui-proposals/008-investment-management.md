# UI proposal 008 — investment management (self-hosted portfolio desk)

Status: PROPOSAL (GUESS where the spec is silent). Sector specialist #008. Read-only on product code;
only this file is written. Grounding read this session: `docs/UI.md`, `spec/08-ui-ux.md`,
`apps/web/lib/api/rpc.ts`, and two components under `apps/web/components/`:
`thread/message-blocks.tsx` (`CardBlock`) and `activity/activity-screen.tsx`.
Charter held: Boma Bot only; no upstream/OpenBot brand, code, wording or assets; BoardUI semantic
tokens only (no raw palette/hex), classes merged with `cx()`; one design system; self-hosted, no
SaaS; robot-cat identity and product name unchanged.

## 1. Persona and top jobs

**Maya, portfolio manager at a small self-hosted asset-management desk.** Runs Boma Bot on the
firm's own machine — no cloud, no portfolio data leaving the building. Several monitoring and
reconciliation bots propose trades and chase data; she is accountable for every executed order and
works at 360–1280 px, often mid-market with interruptions. A wrong or ambiguous click is a
financial and compliance event, so an unintended action is worse than latency.

Top jobs:
1. **Monitor** — see which positions, open orders and stale quotes need attention without reading chat.
2. **Approve safely** — review a proposed trade and be certain of the exact instrument, side,
   notional and effect before committing; never approve by colour alone.
3. **Triage** — find which automated runs (price checks, reconciliations) are stuck, failed or
   waiting on her, without scrolling a transcript.
4. **Audit** — keep a legible, attributable record of what each bot did, for review and compliance.
5. **Report** — produce and verify a versioned end-of-day report and open it in the originating thread.

## 2. Five concrete changes

All new copy is catalogued through `t(locale, key)` (`lib/i18n.ts`); nothing is hardcoded in JSX.
All use BoardUI components already vendored in `apps/web/components/` and the token map in `docs/UI.md`.

**A. Position/watch strip on the thread — `/app/:botId` header (`components/thread/thread-screen.tsx`).**
A compact read-only band above the transcript: named chips for "open orders", "to review", "stale
quotes", each with a `Tooltip` for detail. Components: `Badge`/`Chip`, `Tooltip`, `IconButton`
(`text-foreground-icon-secondary` via `@remixicon/react`, passed as a reference). Tokens: surface
`bg-background-secondary-default`, hairline `border-separator-border`, text `text-text-primary`/
`text-text-secondary`, type `text-caption-1-semibold`, classes joined with `cx()`. Existing RPCs:
`botsGet`, `threadsHead`, `threadsMessages`. Bot-produced values are untrusted text, never HTML;
tickers and amounts stay LTR.

**B. Trade-approval card — `components/thread/message-blocks.tsx` (`CardBlock`, REQ-017/REQ-007).**
Make the "what will happen" sentence dominant: instrument, side, notional and effect, plus a
distinct confirm label ("Approve buy", not just "Approve"). High-impact trades use
`border-border-error-default` + `text-text-error-primary` **plus** a leading icon and the word
"High impact" — never colour alone. Components: `Button`, `Badge`, `Tooltip`; card surface
`bg-background-secondary-default`, `rounded-3xl`, `border-separator-border`, focus
`focus-visible:ring-border-focus-ring`. RPC: `threadsAnswer` (already present, exactly-once effect).
Announced once through the polite live region; the transcript stays readable and other threads
reachable (spec §8.9). No new transport.

**C. "Open work" triage on Activity — `components/activity/activity-screen.tsx` (REQ-011/012/067).**
Adapt the single run list into a reconciliation queue: group by named status, sort failing and
waiting to the top, add status filter `Tabs`, keep the existing pause/resume/retry actions.
Components: `Tabs` (`components/base/tabs`), `Button`, `Badge`/`chip` for text+icon status. Tokens:
`bg-background-primary-default`, `border-border-button-default`, `text-text-secondary`. RPCs:
`runsList`, `runsPause`, `runsResume`, `runsRetry`. Amounts and times formatted with `Intl`, LTR.

**D. Order lifecycle on the board — `/app/board` (`components/board/board-screen.tsx`, REQ-038).**
Reuse the four columns as proposed → approved → placed → settled, cards showing key, title,
assignee bot and last comment; a named detail panel shows the attached reconciliation runs. Change:
migrate the raw `<input>`/`<select>` to the already installed BoardUI `input` and `select`
(design rule: extend installed form components), keep the modal focus trap and Escape close.
Components: `Input`, `Select`, `Button`, `Badge`. Tokens: `text-text-primary`, `text-text-secondary`,
`border-border-button-default`, `bg-background-primary-default`, `text-start`. RPCs: `boardsGet`,
`ticketsCreate`, `ticketsMove`, `ticketsComment`.

**E. Audit/report shelf — `/app/artifacts` + notifications panel (REQ-036/064).**
Show versioned end-of-day reports in the artifact grid/viewer with version selector and "open in
thread"; add a per-bot policy exposing only `waiting`/`completed` alerts with a daily cap. Change:
use installed `Switch` and `Button` with a catalogued "what reaches the desk" sentence; no SaaS
destination by default (self-host only). Components: `Select`, `Button`, `Switch`, `Badge`. Tokens:
`bg-background-secondary-default`, `border-separator-border`, `text-text-secondary`. RPCs:
`artifactsListSpace`, `artifactsGet`, `artifactsListVersions`, `notificationsPolicyGet`,
`notificationsPolicySet`. Report content is untrusted: never inject artifact HTML into the document.

## 3. Accessibility, i18n and RTL

- Every visible string goes through `t(locale, key)` (English source, French/Hebrew translations;
  a missing key falls back to English, never a blank or the key). No string in JSX.
- `DirectionProvider` sets locale/direction; the boot script writes `lang`/`dir` together. Use
  logical utilities (`ms`/`me`, `ps`/`pe`, `text-start`, `border-s`/`border-e`) and mirror the
  rail/drawer motion. Tickers, ISINs, money, dates and URLs stay in explicit LTR regions with
  `<bdi>`/`dir="auto"`, formatted with `Intl`; money uses integer minor units, never floats.
- WCAG 2.2 AA gate: zero critical/serious axe findings in light, dark and RTL; 44 px targets via
  `.boma-target`; token-driven focus ring; full keyboard path (approval, filters, board move,
  policy); status is text + icon, never colour alone; 360 px width; `prefers-reduced-motion`.
- The high-impact marker is text + icon, and the approval effect is announced once via the polite
  live region — the transcript is not a live region, so streaming tokens are not read aloud
  (spec §8.9). Error copy is one sentence of what happened plus one of what to do; no ids, stack
  traces or provider text on screen (correlation id behind "details" only).

## 4. Playwright tests (REQ ids in the name)

```
test("REQ-001 the watch strip shows one optimistic message that is not duplicated", ...)
test("REQ-007 a pending trade card states its effect, takes focus and does not block other threads", ...)
test("REQ-011 activity sorts failed and waiting reconciliations to the top", ...)
test("REQ-012 every run status renders as text plus icon in light, dark and RTL", ...)
test("REQ-017 a high-impact trade names instrument, side and notional and answers exactly once", ...)
test("REQ-036 the report viewer selects a version and opens it in the originating thread", ...)
test("REQ-038 the order lifecycle moves a ticket and records a comment through the same RPCs", ...)
test("REQ-057 the desk screens mirror for Hebrew with a clean axe run", ...)
test("REQ-064 the desk policy enables only waiting and completed alerts with a daily cap", ...)
```
Behaviour above without a REQ yet (watch strip, high-impact marker) must be assigned a REQ id by
the owner before implementation per the traceability gate — do not invent one in test names.

## 5. Highest-impact first slice

**Change B — the trade-approval card.** It is the only change that directly reduces financial and
compliance risk, and the cheapest: `threadsAnswer`, `Button` and `Badge` are installed, the
semantic tokens exist, and no new RPC, component install or data model is needed. Test-first: write
`REQ-017 a high-impact trade names instrument, side and notional and answers exactly once` and
extend `spec/golden/ui/REQ-007-pending-card.json` first; ship only the dominant effect sentence,
the explicit confirm label, the text+icon high-impact marker, keyboard operability and the polite
announcement. Defer (A), (C), (D), (E) until the owner assigns REQ ids and approves.

Provenance: all claims marked CONFIRMED were read this session from `docs/UI.md`,
`spec/08-ui-ux.md`, `apps/web/lib/api/rpc.ts` and `message-blocks.tsx` / `activity-screen.tsx`;
the persona and its jobs are a GUESS (proposal only — no investment-management sector exists in
the spec). No product code was modified.
