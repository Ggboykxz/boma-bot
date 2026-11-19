# 0054 — `apps/web/components/artifacts/artifacts-grid.tsx` — Reliability (v0.4.0)

Worker #054, wave 4, lens: reliability. Target: `apps/web/components/artifacts/artifacts-grid.tsx`
("use client"; REQ-036, spec §8.5) with adjacent reads `lib/api/rpc.ts#call/artifactsListSpace/botsList`,
`components/artifacts/artifacts-shared.ts`. Read-only; no product code changed. This brief is
grid-specific; the page wrapper (`app/app/artifacts/page.tsx`) is covered separately in brief 0020.
Claims: CONFIRMED = read in this tree; GUESS = inferred, needs verification.

## Today (CONFIRMED)

- One mount-only `useEffect([])` fires `Promise.allSettled([artifactsListSpace(signal), botsList(signal)])`
  behind a single `AbortController`; cleanup aborts only on unmount. Phases: `loading | ready | error`.
- `listResult` rejected → whole-screen `error` (`<p role="alert">`); `botsResult` rejected is silent and
  `botName()` falls back to the raw bot id. `listResult.value.artifacts` is assigned with no shape check.
- Filters (`query`, `type`, `bot`) are client-side over the full array; `filtered`, `usedBots` (Set spread)
  and per-card `bots.find` are recomputed on every render. `rpc.call` is a bare `fetch` with **no timeout**
  and no retry. `artifacts.listSpace` returns every family with no cursor/limit.

## Failure modes

1. **Unbounded list fetch.** `listSpace` is unbounded (one row per artifact family, no pagination). The
   grid holds, filters and renders all of them: at 500–5000 families that is a multi-hundred-KB payload,
   a long synchronous filter per keystroke, O(bots) `find` per card, and an unvirtualized DOM. No
   `content-visibility`, cap or page boundary. A malformed reply (`artifacts` not an array) would throw in
   `filter()` with no error boundary, blanking the shell (GUESS on exact payload thresholds).
2. **No retry / no timeout.** The `error` phase is one static alert paragraph — no Retry button, no
   `reloadToken`, no focus move, no error-code taxonomy (offline / 5xx / FORBIDDEN all look identical).
   Worse, `call()` has no timeout: a half-open connection (captive portal, dead VPN) leaves the skeleton
   `aria-busy` forever and failure never surfaces. `401` triggers the global handler but the catch still
   sets `error`, so a dead-end alert can render if navigation does not happen. Bots failure is swallowed.
3. **Stale grid.** Mount-only effect means no refetch on focus/visibility change, no polling, no
   "last updated", no invalidation after a delete on the viewer page. If router/client state survives a
   `/app/artifacts → viewer → delete → back` transition, a deleted artifact can still be shown (GUESS —
   depends on Next router caching; verify with Playwright). Filters also never signal freshness.
4. **Conflated empty states.** `filtered.length === 0` collapses three realities into one
   `t("artifacts.empty")`: (a) the space has no artifacts (onboarding), (b) filters/search exclude
   everything (needs "Clear filters"), (c) data loaded but every row is unrenderable. It is a bare `<p>`
   — no icon, no action, no `role="status"`/`aria-live` result-count announcement for screen readers.

## Hardening (v0.4.0)

- **Bound the fetch and the render.** Ask for a cursor/limited `artifacts.listSpace` (server-side
  type/bot/name filter) — contract change, owner sign-off. Until then, client mitigations: `useMemo`
  `filtered`/`usedBots`; a `Map` of bot id→name instead of per-card `find`; hoist `trim().toLowerCase()`
  once per keystroke; validate `Array.isArray(listResult.value.artifacts)` before setting state; add
  `content-visibility: auto` + `contain-intrinsic-size` to cards, or a client page cap.
- **Timeout + retry.** Compose the unmount signal with `AbortSignal.timeout(10–15s)` via `AbortSignal.any`;
  on timeout → `error` with a Retry button (`retryNonce` state re-fires the effect, `RiRefreshLine`).
  Focus the error heading on entry (`tabIndex={-1}`), announce via `role="alert"`. Add a dismissible
  inline notice when `botsList` fails and fall back to a neutral label, never the raw id. Map
  `RpcError.code`/status to translation keys; on 401 let the global handler own the redirect (do not
  also render `error`).
- **Freshness.** Refetch on `focus`/`visibilitychange` throttled ≥30s with in-flight dedupe, plus a
  manual header refresh; invalidate on route return; keep previous data during revalidation (no
  skeleton flash). Show `aria-live="polite"` "Updated HH:MM" (`text-text-tertiary text-caption-1-semibold`).
- **Split empty states.** Branch on `artifacts.length === 0` (onboarding copy) vs.
  `artifacts.length > 0 && filtered.length === 0` ("No artifacts match these filters" + Clear filters
  resetting query/type/bot); treat unrenderable-all as a distinct case. Wrap the region in
  `role="status"` and expose the result count.
- **A11y follow-through.** `aria-hidden` on skeletons + visually-hidden `role="status"` "Loading…";
  drop the redundant input `aria-label` (wrapping `<label>` suffices); unify focus to the BoardUI ring
  (`outline-none focus-visible:ring-2 focus-visible:ring-border-focus-ring`); gate `animate-row-grow-in`
  behind `prefers-reduced-motion`; keep `<ul>/<li>` and `<time dateTime>`.

## Risks

- Paginated `listSpace` is the only real fix for big tenants and is a DEVIATION (API change) — needs
  owner approval; client caps are a safe interim but shift tab order and need an announced pager.
- `AbortSignal.any`/`timeout` require an evergreen baseline — verify the app's browserslist target.
- Refetch-on-focus can double-render/flicker on slow links — mitigate with throttle + keep-previous-data.
- Retry button, Clear filters, live-region and error copy add i18n keys — must land in every locale or
  fall back cleanly; new selectors can break existing REQ-036 Playwright coverage (re-check, don't weaken).
- Empty-state and inline-notice changes are user-visible; note in PARITY.md. No viewer-route or
  `artifacts.*` semantics change; untrusted-content rules (never render artifact HTML in-app) untouched.

## Tests (Playwright, REQ-036)

loading→ready; timeout→error→retry→ready; 401 redirect; offline; bots-failure notice; empty ×3 shapes;
1k-row filter responsiveness; focus/visibility refetch; axe scan; keyboard path (search → selects → card
links). Mark CONFIRMED only after running against the seeded server.
