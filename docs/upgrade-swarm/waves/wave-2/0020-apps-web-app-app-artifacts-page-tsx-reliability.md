# 0020 — `apps/web/app/app/artifacts/page.tsx` — Reliability (v0.4.0)

Scope: `app/app/artifacts/page.tsx` (thin server wrapper, REQ-036, spec §8.5) + the real surface it
mounts, `components/artifacts/artifacts-grid.tsx` ("use client") + `artifacts-shared.ts` +
`lib/api/rpc.ts#call`. Read-only; no code changed. Tokens cited are those the screen already uses.

## Today (CONFIRMED by reading)

`page.tsx` is 16 lines: `AppShell` + `ArtifactsGrid`. All behavior lives in the client grid:
mount `useEffect` fires `Promise.allSettled([artifactsListSpace, botsList])` behind one
`AbortController`; `listSpace` failure → whole-screen `error`; `bots` failure is swallowed silently.
Filters (query, type, bot) are pure client-side over the full array. `rpc.call` is a bare `fetch` —
**no timeout**; only unmount aborts; 401 → global `unauthorizedHandler`; else `RpcError{code,status}`.
`artifacts.listSpace` returns the latest version of **every** family in the space with **no
pagination, cursor or limit** (`ArtifactListSpaceReply{artifacts: ArtifactView[]}`).

## Failure modes

1. **List load failure, no way back.** `listSpace` rejects → `role="alert"` paragraph and nothing
   else: no retry button, no `reloadToken`, no focus move. The only recovery is editing the URL or a
   full reload. `call()` has no timeout, so a half-open connection (captive portal, dead VPN) leaves
   the skeleton `aria-busy` forever — failure never surfaces at all.
2. **Partial failure is invisible.** `botsList` failing is silently tolerated: `botName()` then
   falls back to the raw bot **id** (or the artifact renders with an internal identifier where a
   name was expected). Not user-fatal, but it leaks internal ids into the UI and gives the user no
   signal that bot attribution is degraded.
3. **Search glitches.** The filter is fine for small sets, but: `query.trim()` is evaluated twice per
   artifact per keystroke; matching is substring `toLowerCase().includes` on `name` only — no
   description, no id, no bot name (spec §8.5 says "search by name", so scoped correctly, but the
   affordance suggests more). No debounce needed at small n; at large n every keystroke re-filters
   and re-renders the whole grid synchronously. No `aria-live` announcement of result count; a
   screen-reader user typing gets zero feedback that the grid changed.
4. **Empty states, one for all.** A single `{t("artifacts.empty")}` covers three distinct realities:
   (a) space has no artifacts at all (onboarding moment — should say so and point at how artifacts
   appear), (b) filters/search exclude everything (should offer "clear filters"), (c) list loaded
   but every artifact is unrenderable. `filtered.length === 0` can't tell them apart. Empty state is
   a bare `<p>` — no icon, no action, not announced as a region.
5. **Big tenants.** `listSpace` is unbounded: the server ships one row per artifact **family**, the
   client holds them all, renders all, and recomputes `filtered`, `usedBots` (Set spread) and
   `botName()` (O(bots) `find` per card) on every render. At 500–5000 families this is multi-hundred-
   KB payloads, long main-thread filters per keystroke, and a DOM that never virtualizes. Grid also
   has no `content-visibility`/pagination fallback. `clip(name, 64)` slices by UTF-16 code units —
   can split a surrogate pair and emit a lone half-emoji; comment claims "never more than max bytes
   of UTF-8", which the code does not do (it counts UTF-16 units, not bytes).
6. **Stale grid.** No refetch on focus/visibility, no polling, no invalidation after a viewer-page
   delete. Navigate `/app/artifacts` → viewer → delete → back: the grid can show the deleted item if
   the client component's state survives the transition. No "last updated" affordance.
7. **Error copy / a11y gaps.** Error screen: no retry, no focus management; loading skeletons are
   not `aria-hidden` and there is no visually-hidden `role="status"` announcement (skeletons are
   silent to AT). `aria-busy` is set on the loading section but not cleared via a phase change that
   AT can perceive. Search `<label>` wraps the input (good) yet also duplicates `aria-label` on the
   same input (redundant). Selects rely on `aria-label` only — acceptable, but focus styling uses
   `boma-focus`/`focus-visible:outline` instead of the BoardUI ring recipe
   (`outline-none focus-visible:ring-2 focus-visible:ring-border-focus-ring`), drifting from the
   system. `time dateTime` is correctly set (keep). Type bucket regex
   `MIME_TEXT = /^text\/|json|xml|csv|markdown/` has no anchors on the alternation — matches
   "application/json" (intended) but also "application/vnd.openxmlformats-…markdown…"; acceptable,
   should be tightened to `(^|[+/;])json` style. Invalid `createdAt` → `Date.parse` NaN → renders the
   raw ISO string (okay), but `new Date(NaN).toLocaleDateString` would be worse if shortDate changed.
8. **401 mid-load.** 401 fires `unauthorizedHandler` globally, but the catch still sets `error`;
   if the handler doesn't navigate, the user lands on a dead-end error screen with a code they
   can't act on.

## Hardening (v0.4.0)

- **Timeout + retry on the load path.** In `rpc.call` (or at this site), compose the unmount signal
  with `AbortSignal.timeout(10–15s)` via `AbortSignal.any`; on timeout/abort-by-timeout → `error`
  phase **with a retry button** (same pattern as other screens: `reloadToken` state re-fires the
  effect; `RiRefreshLine`). Make the error panel a focusable region (`tabIndex={-1}`, focus the
  heading once per transition into error).
- **Bots failure ≠ silent.** Keep `allSettled`, but when `bots` rejects show an inline dismissible
  notice ("Bot names unavailable — showing ids") and fall back to a neutral label instead of the raw
  id; do not leak internal ids. Optionally collapse the bot filter to "all" when the roster is
  missing.
- **Split the three empty states.** Distinguish `!artifacts.length` (onboarding copy) from
  `artifacts.length && !filtered.length` ("No artifacts match these filters" + a "Clear filters"
  button that resets query/type/bot). Give the region `role="status"` or `aria-live="polite"` for the
  result count.
- **Big-tenant path.** (1) Ask the contract for a bounded/paginated `listSpace` (cursor + limit,
  server-side type/bot/name filter) — spec change, needs owner sign-off; until then: (2) cap render
  with client pagination or `content-visibility: auto` + `contain-intrinsic-size` on cards;
  (3) `useMemo` for `filtered`/`usedBots`/bot-name map, precompute `botName` via `Map` not per-card
  `find`; (4) hoist `trim().toLowerCase()` once per keystroke; (5) fix `clip` to slice on code
  points (`[...text].slice(0, max)`) and correct its doc comment. Consider search across
  description as a separate, explicit decision.
- **Freshness.** Refetch on `focus`/`visibilitychange` throttled ≥30s with in-flight dedupe, plus a
  manual header refresh button; invalidate on route return if the grid kept state. Show
  `aria-live="polite"` "Updated HH:MM" caption (`text-text-tertiary text-caption-1-semibold`).
- **A11y / semantics.** `aria-hidden` on all skeletons; add visually-hidden `role="status"`
  ("Loading artifacts…"); drop the redundant input `aria-label` (the wrapping `<label>` suffices) or
  the label, not both; unify focus rings to the BoardUI recipe; announce filter result counts via a
  polite live region; keep icon+text (never color-only); verify `text-text-tertiary` empty copy
  ≥4.5:1 in light and dark; honor `prefers-reduced-motion` (`animate-row-grow-in` should be gated);
  keep `<ul>/<li>` semantics and `<time dateTime>`.
- **Error taxonomy.** Map `RpcError.code` (FORBIDDEN/UNAVAILABLE/TIMEOUT/…) to translation keys;
  stop one-string-fits-all; on 401 let the global handler redirect — don't also render `error`.
- **Contract/tests.** Playwright: loading → ready; error → retry → ready; 401 redirect; offline;
  empty × 3 shapes; 1k-row seed for filter responsiveness; axe scan; keyboard path
  (Tab through search → type → bot → card links); grid with bots call failing.

## Risks

- Contract change for paginated `listSpace` is the only real fix for big tenants and is an API
  decision — DEVIATION-level, needs the owner; client-side caps/pagination are safe interim steps.
- `AbortSignal.any` needs a modern baseline (already used by evergreen browsers; verify the app's
  browserslist target).
- Refetch-on-focus can double-render and cause visible flicker on slow links — mitigate with
  keep-previous-data rendering (no skeleton flash on revalidate) and ≥30s throttle.
- Clear-filters button and live-region copy add i18n keys — must land in every locale or fall back
  cleanly.
- Grid re-architecture (memoization is safe; pagination changes tab order and focus behavior) —
  pagination must be reachable and announced (`aria-label` on nav, `aria-current` on page).
- `clip` change alters visible truncation for astral-plane names — golden/screenshot churn possible.
- Retry button + focus-on-error shifts Playwright selectors; existing REQ-036 coverage must be
  re-checked, not weakened.
- No behavior change is proposed to the viewer route or to `artifacts.*` semantics; untrusted-content
  rules (never render artifact HTML in the app document) are untouched.
