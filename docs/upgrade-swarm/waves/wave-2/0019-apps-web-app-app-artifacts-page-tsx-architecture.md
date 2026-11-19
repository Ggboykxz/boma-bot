# 0019 — apps/web/app/app/artifacts/page.tsx — architecture lens

- Target: `before-m5` (v0.3.0-rc5), module `apps/web/app/app/artifacts/page.tsx` (13 lines).
- READ-ONLY review. The page is a pure delegate: the substance, and the debt, live in `apps/web/components/artifacts/artifacts-grid.tsx` (196 lines) and `artifacts-shared.ts`.
- Spec anchors: `spec/08-ui-ux.md §8.5` (Artifacts: grid, filter by type/bot, search by name), `spec/01-requirements.md` REQ-036 (MUST, versions 1..N, members list/read/delete), `docs/UI.md` (catalogue rule, BoardUI semantic tokens), `spec/03-api-contract.md` §3.1 (`artifacts.listSpace` takes `{}` — no query, filter, or page).

## The page's role, today

`/app/artifacts` is the space's output shelf: the latest version of every family, as a card grid. The page itself is exactly what it should be — a server component wrapping `<AppShell><ArtifactsGrid /></AppShell>`. That shape is correct and should survive v0.4.0: routing and shell in `app/`, rendering in `components/`. Everything else was pushed into one client component, so the architecture is accidental rather than designed. The v0.4.0 work is to make the list/search/filter structure deliberate without fattening the page.

Confirmed observations (read this session):

- `ArtifactsGrid` is `"use client"` and monolithic: fetch (`artifactsListSpace` + `botsList` via `Promise.allSettled`), the `loading|ready|error` phase machine, search/type/bot filter state, derived `usedBots`, skeleton, and the grid in one file.
- `artifactsListSpace()` takes no arguments. **Every filter is client-side over the full space list** — no server-side query, type/bot filter, or pagination. O(n) per keystroke (unmemoized), unbounded fetch, and the contract gives the client no way to ask for page 2.
- The header comment claims "newest families first in the client", but there is **no `.sort()` anywhere** in the file; render order is whatever the server returned. Either the comment is stale or the ordering is an unverified server assumption — both are bugs waiting to fire.
- Dead import: `AuthShell` (`@/components/auth/auth-shell`) is imported and never used.
- Error state is a bare `role="alert"` line with **no retry**; empty state is a bare `<p>` with no guidance or "clear filters" action; skeleton sets `aria-busy` but has no accessible name.
- Raw `<select>` ×2 and a raw `<input type="search">` instead of the vendored BoardUI base components (`components/base/input`, `components/base/dropdown`, react-aria foundations, already the house pattern elsewhere).
- Filter state lives only in React state — not in the URL, so a filtered view is not shareable or restorable on reload. The codebase already has the `lib/loc` pattern (`useSyncExternalStore` + `window.location`) for reading the URL in the static export.
- Filtering/bots-derived state recomputes on every render; `botName` falls back to the raw bot id when the bot vanished from the roster (silent, acceptable, but undocumented).
- The page has **no `metadata` export**, and the subtree has no `loading.tsx`/`error.tsx` — consistent with the rest of `app/`, but the artifacts subtree also hosts the viewer (`/app/artifacts/<id>`), which reads its id from the URL at runtime under ADR-013; a route-level boundary would cover both.
- Tokens are otherwise honest: semantic `bg-background-*`, `text-text-*`, `border-border-*`, `text-foreground-icon-*`, composite type utilities, `cx`-free but consistent class merges. Keep this.

## Proposals (v0.4.0)

### 1. Keep the page thin; add `metadata` + route-level `loading.tsx`/`error.tsx` for the `artifacts/` segment
`export const metadata = { title: "Artifacts" }` (catalogue-backed where the project does titles — currently none do, so match whatever the v0.4.0 metadata convention lands on), plus `loading.tsx` (the existing skeleton, hoisted) and `error.tsx` (client boundary with retry) covering `/app/artifacts` and the viewer subtree. Risk: static export (ADR-013) means these are build-time shells; `error.tsx` only catches client-side throws below the boundary, not RPC failure — do not let it masquerade as one. Low risk; verify with the UI golden and axe gates.

### 2. Move search/filter/sort/page into the `artifacts.listSpace` contract
Extend the RPC input to `{ query?, type?, botId?, cursor?, limit? }` (server-side RLS-scoped query on `name`, `mime_type` bucket, `bot_id`; keyset cursor on `(created_at, id)`), and render with the vendored `components/base/pagination`. The grid becomes: debounced query → fetch page → render; client-side `typeBucket` stays only as a display hint. Risk: contract change in `spec/03-api-contract.md` + `spec/TRACE.md` (REQ-036 golden `artifact-list` needs a sibling), an index on `(space_id, created_at)`, and axe/golden churn. Highest value; land behind the REQ-036 gate in slices (additive input first, server filter second, cursor third). Interim: at minimum add the missing client-side `createdAt` desc sort and memoize `filtered` — one line, closes the comment/behavior mismatch.

### 3. Extract the duplicated fetch phase machine into one typed hook + retry
`useSpaceResource<T>(fetchers, deps)` in `lib/` (AbortController on unmount, reload token, `loading|ready|error` with the error carrying `RpcError` code), consumed by grid and viewer; error state gains a "Try again" button (`role="alert"` preserved) and the result count gets an `aria-live="polite"` region; skeleton gets `aria-label`. Risk: abort semantics must be byte-identical or set-state-after-unmount returns; shared hook couples screens (a change for one can regress the other's error wording). Test the network-down case explicitly (fetch TypeError must funnel to generic `error`, not `forbidden`).

### 4. Sync filter state to the URL (`?q=&type=&bot=`)
Extend `lib/loc` (or a sibling `lib/search-params.ts` on the same `useSyncExternalStore` shape) to read/write the query string; filters become deep-linkable and Back-button-friendly, which matters for "show me the PDFs from bot X" pasted into a thread. Risk: the static export cannot use `useSearchParams` without a Suspense boundary in Next 16 — use the raw `window.location` read like the viewer does, or wrap properly; golden URLs in `tests/golden.spec.ts` must be updated; low data-exposure risk (filters are names/ids already visible in the space). GUESS on exact Next 16 static-export interaction — verify against `next.config.ts` before building.

### 5. Component hygiene pass: dead import, BoardUI base components, honest empty state
Remove `AuthShell`; swap raw `<select>` for BoardUI `dropdown` and raw search `<input>` for BoardUI `input` (label/hint/error wiring, keyboard + RTL correctness for free); empty state gains an icon, catalogue copy, and a "Clear filters" action shown only when a filter is active; `botName` fallback renders "Unknown bot" (catalogue key) instead of a raw id. Risk: visible diff churn in goldens and axe; dropdown must keep the same `aria-label`s and focus order; the `artifacts.*` catalogue keys may need fr/he translations or documented English fallback per `docs/UI.md`.

## Explicit non-goals
No client-side authorization ("hide the grid for non-members") — the server refuses, keep it. No rendering artifact bytes as markup (viewer's bounded-frame rule stands). No BoardUI Pro components. No new screens beyond §8.5. No pagination UI before proposal 2's server support lands.

## Suggested sequencing
5 + the one-line sort/memo from 2 first (hygiene, zero contract change), then 1, then 3, then 4, then 2 in slices — each step gated by `lint`, `typecheck`, `test:a11y`, `test:ui-golden`, and the REQ-036 trace.
