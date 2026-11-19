# 0065 — `apps/web/components/board/board-screen.tsx` — architecture lens (v0.4.0)

Scope: the 406-line `"use client"` screen for REQ-038 (spec §8.6). Read-only review; nothing edited.
Sister files read: `app/app/board/page.tsx` (server delegate), `lib/api/rpc.ts` (`boards.get`,
`tickets.create|move|comment`, `runs.list`, `TicketView`), `lib/use-translation.ts`, `components/board/`
(this file only — no `Dialog` primitive installed), `tests/board.spec.ts` (3 REQ-038 cases),
`docs/upgrade-swarm/waves/wave-2/0021` and `0022`. Claims: CONFIRMED = read here; GUESS = inferred.
Constraint for every item below: keep BoardUI semantic tokens (`text-text-*`, `bg-background-*`,
`border-border-*`, `text-foreground-icon-*`, composite type utilities) and `cx()` — no raw palette,
no second design system, no change to the `page.tsx` delegate.

## Structure today (CONFIRMED)

One component owns six concerns: a 3-value `Phase`, two divergent fetch paths (aborting mount effect
vs signal-less `load()`), three mutations, the assignee filter, the column grid, and a hand-built
modal. `selected` is a copied `TicketView`; the dialog uses a raw `<textarea>` while the create form
uses BoardUI `Input`/`Textarea`/`Select`. The flag gate is only a client probe of `boards.get`.

## Upgrades

### 1. Extract one typed `useBoard` hook and presentational children
Move all RPC/state wiring into `hooks/use-board.ts`: one fetch path, one `AbortController` with a
timeout, and fold `ticketsCreate`/`ticketsMove`/`ticketsComment` replies into state instead of a full
`load()` reload. Split rendering into `board-column`, `ticket-card`, `ticket-detail-dialog` and
`create-ticket-form` under `components/board/`; props down, no context or new state library. The hook
exports `phase`, `tickets`, `bots`, `filterBot`, actions and `selectedId` (never a copied ticket).
Risks: (a) behavior-preserving refactor only — DOM/role map, REQ-038 Playwright cases and axe must
stay green; (b) merge-on-mutation can break key ordering — re-sort on merge; (c) over-splitting
into providers adds indirection on a COULD-flagged surface; (d) hook tests need a mocked RPC layer.

### 2. Discriminated state, derive selection from the list
Replace `Phase` with a union (`loading | ready | absent | error | forbidden`) and keep action state
(`busy`, `commenting`, `actionError`) separate; this removes the silent "500 → empty ready board"
and gives `error` a retry seam. Derive the dialog ticket as `tickets.find(id === selectedId)` so
counts/`updatedAt` cannot drift, and close the dialog if the ticket vanishes. Risks: (a) new
`board.error.load` / `board.retry` / gone-ticket keys across 14 locales (fallback applies); (b)
error/forbidden panels need an accessible focus+announcement path — re-run axe; (c) never apply the
assignee filter to the open dialog, or filtering hides the selected ticket; (d) HTTP-shape changes
are owned by the Rust gate (wave-2 #1), not this file — keep the client probe as defense in depth.

### 3. Replace the hand-rolled modal with a BoardUI primitive
Install the BoardUI Dialog/modal component via the MCP server (or `npx boardui@latest add`) and use
it for the detail panel; swap the raw `<textarea>` for the existing BoardUI `Textarea`, and merge
class strings with `cx()`. Keep focus-in on open, add focus restore to the invoking card, Escape and
backdrop behavior from the primitive (WCAG modal pattern). Risks: (a) no Dialog primitive is
installed today — confirm availability/license (Pro needs an activated key; never vendored without
checking redistribution terms); (b) markup change can shift the axe/role map — verify keyboard-only
create/move/comment; (c) portal placement under RTL needs `dir` propagated; (d) if no primitive fits,
keep the hand-built one but still fix focus restore and use BoardUI `Textarea` — do not invent a
second design system.

### 4. One shared column model, honest unknowns and no N+1
Centralize `COLUMNS`/`parseColumn` in a small `board-model.ts` imported by card, dialog, filter and
hooks, so column labels/order have a single source of truth. Parse into `Column | "unknown"` and
render a visible "needs triage" bucket instead of silently misfiling to `backlog`; surface "last
comment" via a `lastCommentPreview` field on `TicketView` rather than per-card comment calls.
Risks: (a) an unknown bucket is new product surface — catalogue keys + empty-state decision, possibly
an ADR/DEVIATION note if spec §8.6 fixes exactly four columns; (b) `lastCommentPreview` widens the
wire contract (rpc/wire/rpc_surface/OpenAPI regeneration, owner sign-off, additive not DEVIATION);
(c) keep `xl:grid-cols-4` with logical styles and verify `dir=rtl`; (d) preserve REQ-057 stagger and
reduced-motion. Full comments list + run status stays sequenced per wave-2 #4.

### 5. Wrap the scroll region with the BoardUI scroll-fade recipe
The root uses `overflow-y-auto`; adopt the `scroll-fade` registry item (`ScrollFade` +
`useScrollFade`) instead of a hard clip, keeping the header outside the scroll region. Keep every
current token and composite type utility as-is. Risks: (a) a translucent overlay parent creates a
backdrop root and can break blur — apply opacity to layers, not the parent; (b) verify top/reset,
light/dark, 100% zoom and keyboard scrolling; (c) extra blur layers cost frames on low-end devices —
honor `prefers-reduced-motion`; (d) do not blur the whole content area.

## Non-goals / verification

No drag-and-drop, realtime sync, BoardUI Pro dependency without a license, spec edit, or flag-default
change. Verify with the REQ-038 Playwright spec (flag on/off), the axe suite, goldens, and an unchanged
BoardUI token/role map; mark outcomes CONFIRMED or GUESS in `docs/STATE.md`.
