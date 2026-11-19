# 0085 — `apps/web/components/shell/bot-rail.tsx` — architecture lens (v0.4.0)

Scope: the 329-line `"use client"` rail (spec §8.3). Read-only review; nothing edited. Sister files read:
`app-shell.tsx` (sole mount), `components/shell/bot-create-form.tsx` (REQ-004 create entry),
`components/shell/acp-detected-row.tsx` (REQ-061 detected executors), `lib/roster.ts` and
`lib/bot-directory.ts` (two `useSyncExternalStore` stores), `lib/api/rpc.ts` (`bots.create`,
`bots.detectedExecutors`, `bots.connectAcp`, `groups.*`, `threads.unsnooze`), `tests/bot-create.spec.ts`
(REQ-004/REQ-061), spec/01 REQ-004/REQ-061, spec/08 §8.3. Claims: CONFIRMED = read here; GUESS = inferred.
Constraint for every item: keep BoardUI semantic tokens (`text-text-*`, `bg-background-*`,
`border-border-*`, composite type utilities), `cx()`, and one design system — no raw palette/hex.

## Structure today (CONFIRMED)

One component owns: two external stores, two `useEffect` fetchers (keyed on `pathname` and on
`creating`/`bots`), an inline group-create form, a snoozed shelf, three `<nav>` landmarks, and all
active-state checks. `active` is recomputed ad hoc with `pathname ===` in four places plus one
`startsWith`; there is no shared nav model. First paint takes a dual path (`roster === null` renders
the client `directory`, then the server roster), and groups fall back to `[]` on any fetch error.
The two create entries are implemented differently: `BotCreateForm` uses BoardUI `Input`/`Button`/
`IconButton`, while the inline group form uses a raw `<input>`/`<button>` and raw class strings.
`AcpDetectedRow` picks its attach target silently as `roster?.find(!archived && !snoozed)` and keeps
`attached` as a local flag; it renders nothing both when detection finds nothing and when it fails.

## Upgrades for v0.4.0

### 1. One nav model, extracted children, one data hook
Introduce `hooks/use-bot-rail.ts` for the two stores, group fetch and mutations, plus a typed
`RailItem` (`kind: bot | group | activity | executor`, `href`, `label`, `active`) built once from
pathname. Render `RosterRow`, `group-list`, `snoozed-shelf` and the two create entries as presentational
children under `components/shell/`; props down, no context — the external stores already avoid a
provider. Risks: (a) behavior-preserving refactor only: landmark/role map, UI goldens and axe stay
green; (b) static export (ADR-013) means `active` must come from `usePathname`, not route params —
`botIdFromPath` is not a substitute here; (c) over-splitting adds indirection to a single small rail;
(d) hook tests need a mocked RPC layer (none exists for this module today).

### 2. REQ-061: honest detection state and an explicit attach target
Give detection a discriminated state (`loading | found | none | error`) with a retry, so "detection
failed" is no longer indistinguishable from "no executors found". Replace the silent
`roster?.find(...)` target with the bot currently open (`botIdFromPath(pathname)`) labelled in the
row, or a small picker when several candidates exist; place the row inside the nav model as a
"local agents" section, not an unplaced sibling between bots and snoozed. Derive `attached` from
server state after `refreshRoster()` instead of local state, so it survives remount and roster change.
Risks: (a) new catalogue keys across locales (fallback applies); (b) a target label/picker changes
the DOM — update the `REQ-061` Playwright case; (c) keep the spec invariant: no attachment without
an explicit click and nothing attached on detection alone; (d) remote deployments must still render
nothing when no executor exists.

### 3. Converge the two create entries (REQ-004 / REQ-061)
Port the inline group form to the BoardUI primitives `BotCreateForm` already uses (`Input`, `Button`,
`IconButton`, `cx()`), removing the raw `<input>`/`<button>` and manual class strings. When REQ-061
finds an executor but there is no eligible bot, route its dead-end "no-bot" note to the existing
"New bot" create entry instead of leaving the operator stranded. Risks: (a) markup change can shift
the axe/role map — verify keyboard-only create for both forms; (b) keep blank-name rejection before
the RPC call (pinned by the `REQ-004` cases) and the failure-keeps-form-open behavior; (c) keep
"New bot" and "New group" as distinct actions — do not collapse them into one ambiguous control;
(d) confirm `IconButton`'s `icon={RiAddLine}` API before swapping; no second design system.

### 4. Single source of truth for roster/directory and groups
Replace the `roster === null ? directory : regular` double render with an explicit view state
(`pending | live`), keep the last good group list on fetch error instead of resetting to `[]`, add
an `AbortController` + timeout to the two effects, and reconcile the optimistic group row's
`createdAt`/`ownerBotId` from the server reply rather than synthesizing `""/first member`. Risks:
(a) changing first paint must not break the offline case where a 503 roster still shows "New bot";
(b) new error/timeout states need catalogue keys and an accessible announcement; (c) merging the
server row must not drop it if a list refetch races the create; (d) keep `animate-row-slide-in`/
`animate-number-fade` and `prefers-reduced-motion`.

### 5. Nav chrome: RTL-aware disclosure and a11y wiring
Use `IconButton` + `cx()` for the snoozed/new-group disclosure controls, give the snoozed toggle an
`aria-controls` pointing at a stable scoped id, and mirror `RiArrowUpSLine`/`RiArrowDownSLine` under
`dir=rtl` (they are physical glyphs). Keep every current token and composite type utility. Risks:
(a) `aria-controls` ids must be unique across mounted rail instances — scope them; (b) verify the
chevron direction and `boma-focus` ring at 360 px in RTL; (c) adding landmarks can move golden
assertions — re-run landmark checks; (d) do not hardcode dark-mode colors.

## Non-goals / verification

No spec edit, no new BoardUI Pro dependency without a license, no second state library/provider, no
space switcher/sections/status-dot/footer build-out beyond §8.3 parity unless owned by an ADR, and
no change to `app-shell.tsx`'s mount. Verify with `tests/bot-create.spec.ts` (REQ-004 and REQ-061),
the axe suite, UI goldens and an unchanged BoardUI token/role map; record outcomes CONFIRMED or
GUESS in `docs/STATE.md`.
