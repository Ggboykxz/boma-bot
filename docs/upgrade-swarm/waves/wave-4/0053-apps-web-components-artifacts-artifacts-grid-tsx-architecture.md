# 0053 — apps/web/components/artifacts/artifacts-grid.tsx — architecture lens

- Target: `before-m5` (v0.3.0-rc5), module `apps/web/components/artifacts/artifacts-grid.tsx` (198 lines). Wave 4, worker 53.
- READ-ONLY review. **PROPOSAL only** — no product code, spec, golden or dependency changed by this worker.
- Lens: **architecture** — the fetch / search / filter / sort structure behind the grid. Focus REQ-036 (artifacts with versions, `spec/01-requirements.md` §"Artifacts, pages, boards"), `spec/08-ui-ux.md §8.5`, `spec/03-api-contract.md §3.1` (`artifacts.listSpace` takes `{}` — no query/filter/page).
- Markers: **CONFIRMED** = read from this repo this session; **GUESS** = recommendation needing owner decision (an externally visible change is a DEVIATION per `AGENTS.md`).

## The structure today (CONFIRMED)

`ArtifactsGrid` owns everything: fetch, a `loading|ready|error` phase machine, three filter states, derived `usedBots`, skeleton, toolbar and grid — one client component. That is accidental coupling, not a designed boundary; v0.4.0 should make the data pipeline deliberate while leaving the page (`app/app/artifacts/page.tsx`) a thin delegate.

- **Fetch** (lines 40–59): `Promise.allSettled([artifactsListSpace, botsList])` under an `AbortController`; abort-on-unmount is correct. `artifactsListSpace()` takes no arguments, so the full space list is fetched every mount — no server query, type/bot filter or pagination.
- **Search/filter** (lines 64–75): a single inline `artifacts.filter` over `name`, `typeBucket(mimeType)` and `botId`; case-insensitive, trimmed, O(n) per keystroke, **unmemoized** — recomputed on every render. `usedBots` (line 77) is derived the same way.
- **Sort**: there is **no `.sort()` anywhere** in the file, yet the header comment (lines 26–29) promises "latest version of every family" newest-first. REQ-036's acceptance ("latest is returned by default") is currently an **unverified assumption** about the server's row order — the comment and the behavior disagree.
- **State durability**: query/type/bot live only in React state — a filtered view is not shareable or reload-safe. `apps/web/lib/loc.ts` (`useSyncExternalStore` + `window.location`) is the house pattern for URL reads in the static export.
- **Hygiene**: `AuthShell` is imported (line 14) and never used. Error state is a bare `role="alert"` line with no retry; empty state is a bare `<p>`. Raw `<input type="search">` and two raw `<select>` stand in for installed BoardUI `components/base/input/input.tsx` and `components/base/select/select.tsx`.
- **Tokens are honest**: semantic `bg-background-*`, `text-text-*`, `border-border-*`, `text-foreground-icon-*`, composite type utilities, `@remixicon/react` refs, `boma-focus`. Keep all of this; use `cx()` from `@/utils/cx` for any new class merges.

## Concrete upgrades for v0.4.0 (each with a risk)

### 1. Add a real, deterministic sort and name it in the pipeline
Introduce `sortArtifacts(list)` doing `createdAt` desc with a stable `(id)` tie-break, applied to the selection result — not to server state. This closes the comment/behavior gap and makes REQ-036's "latest first" an explicit client guarantee instead of a server-order assumption.
- **Risk**: if the server later returns a meaningful order, a client re-sort could contradict it — pin the rule in a REQ-036 test and in the comment. Goldens that assert card order must be regenerated. Low cost, highest correctness win.

### 2. Extract the filter pipeline into a pure, memoized selector
Move lines 64–77 into `selectArtifacts(artifacts, { query, type, bot }, bots)` in `components/artifacts/artifacts-shared.ts` (already the shared shaping module). Call it through `useMemo` keyed on the three controls + `artifacts`. One function, one place to test trim/case/bucket semantics.
- **Risk**: must preserve exact semantics (`.trim().toLowerCase()`, `typeBucket`, `botId === bot`) or existing `artifacts.spec.ts` search/filter cases regress. Keep the case-insensitive/RTL-safe comparison of `name` unchanged; do not add locale collation silently.

### 3. Split fetch/phase from rendering via one typed resource hook
Extract lines 33–59 into `useSpaceArtifacts()` returning `{ phase, artifacts, bots, error, retry }` with the AbortController and `loading|ready|error` machine, so the view is a pure function of that state and the same hook can serve the viewer's fetch later. Add a `retry` token for a "Try again" action on error.
- **Risk**: abort semantics must stay byte-identical or stale responses set state after unmount; the network-down `TypeError` must funnel to generic `error`, not be mislabeled. A shared hook couples screens — a wording change for one can regress the other. Test the failure path explicitly.

### 4. Make search/filter state URL-backed and shareable
Persist `q`/`type`/`bot` to the query string via the existing `lib/loc` pattern (`useSyncExternalStore` + raw `window.location`), so "PDFs from bot X" is a deep link that survives reload and Back. Initialize React state from the URL, write on control change.
- **Risk (GUESS)**: `useSearchParams` in a static export needs a Suspense boundary under Next 16 — use the raw `window.location` read the viewer already uses, and verify against `next.config.ts` before building. Golden URLs in `tests/golden.spec.ts` may shift. Low data-exposure risk (ids/names already visible in the space).

### 5. BoardUI hygiene pass on the toolbar and states
Swap the raw search `<input>` for `components/base/input/input.tsx` and the two raw `<select>` for `components/base/select/select.tsx` (label/hint wiring, keyboard + RTL for free); keep the same `aria-label`s and focus order. Remove the dead `AuthShell` import. Give the skeleton an accessible name and the empty state a catalogue-backed "Clear filters" action shown only when a filter is active; give the result count an `aria-live="polite"` region. `botName` fallback becomes a catalogue "Unknown bot" key rather than a raw id.
- **Risk**: visible diff churn in goldens and axe; dropdown must not break the `combobox` roles the existing REQ-036 tests target. New `artifacts.*` keys need fr/he translations or a documented English fallback per `docs/UI.md`. Preserve every semantic token and composite type utility; no raw palette, no hex/oklch, no `dark:` literal, no Pro component.

## One REQ-named test to pin the structure

In `apps/web/tests/artifacts.spec.ts`, extend the existing REQ-036 cluster (do not weaken it):

```ts
test("REQ-036 the grid sorts newest first and clears filters", async ({ page }) => {
  await seedGrid(page); // seed two versions out of chronological server order
  await page.goto("/app/artifacts/?type=image");
  const names = page.locator("ul li a span.line-clamp-2");
  await expect(names.first()).toHaveText("newer.png"); // explicit client sort
  await page.getByRole("searchbox", { name: t("en", "artifacts.search") }).fill("zzz");
  await expect(page.getByText(t("en", "artifacts.empty"))).toBeVisible();
  await page.getByRole("button", { name: t("en", "artifacts.clear-filters") }).click();
  await expect(names.first()).toBeVisible();
});
```

No new golden needed (this asserts behavior over seeded state, not a click-only path). If upgrade 4 lands, assert the `?type=image` URL round-trip too.

## Explicit non-goals
No client-side authorization — the server refuses non-members, keep it. No rendering artifact bytes as markup. No server-side query/filter/pagination in this proposal: `artifacts.listSpace` keeps `{}` and that contract change is a separate, higher-risk slice (see 0019 proposal 2). No new screen beyond §8.5. No second design system.

## Sequencing
1 (sort) + the memoized selector from 2 and the dead-import removal from 5 first — zero contract change; then 3, then 5, then 4. Gate each step with web `lint`, `typecheck`, `build`, Playwright `artifacts`/axe clusters (light, dark, RTL, 360 px) and the REQ-036 trace. Update `docs/STATE.md` (CONFIRMED/GUESS) and `docs/UI.md`; no dependency change.

This document ran no build or test; no test-pass claim is made here.
