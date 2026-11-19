# 0082 · `apps/web/components/shell/app-shell.tsx` · LENS=reliability · v0.4.0 proposal

Worker #082 · module `apps/web/components/shell/app-shell.tsx` · READ-ONLY (proposal only; no code here).
Line refs against `before-m5` (web package `0.3.0-rc5`).

## Scope / what the file does

98-line `"use client"` frame for the whole `/app` subtree (seven pages import it). It renders a
`min-h-dvh` column: a `<header>` with one-nav-item `<nav aria-label>` plus `ThemeToggle`, then a
max-w-5xl body row of `<BotRail/>` (sibling) and `<main>{children}</main>`. It owns no data, no gate,
no error boundary; it is the last static frame every screen and the rail hang off.
CONFIRMED from `app-shell.tsx`, `bot-rail.tsx`, `app-providers.tsx`, `app/app/*/page.tsx`,
`lib/api/rpc.ts`, `lib/use-translation.ts`, `spec/08-ui-ux.md §8.1/§8.3`; runtime impact is GUESS
until the tests in "Success criteria" exist.

## Failure modes

1. **Unhandled child errors take the entire shell down.** There is no `error.tsx` or
   `global-error.tsx` anywhere under `app/`. A render throw in `children` (or in the `BotRail`
   sibling) bubbles to Next's root default error UI: header, primary nav, rail and the landmarks the
   goldens assert are all replaced by a generic page. Under ADR-013's static export there is no
   server route to render a friendly fallback, so recovery is "hard reload and hope". A rail failure
   should not blank a healthy thread, and a thread failure should not remove the navigation.
2. **Rail failures are swallowed or unhandled.** `BotRail`'s `groupsList`/`botsList` use
   `.catch(() => setGroups([]))` / `setBots([])` — a 401, 500 or timeout is indistinguishable from
   "no groups/bots", with no message and no staleness marker. `void refreshRoster()` (line 89) has
   no `.catch`: a rejected refresh is an unhandled rejection. When `roster === null` it silently
   falls back to the pre-meta `directory` cache (lines 168–183), but nothing says the list may be
   stale. `useSyncExternalStore` snapshot skew between `serverRoster`/`serverBots` and the client
   can produce a hydration mismatch that is again resolved by discarding the shell.
3. **Auth gate is outside the shell and races its children.** The gate is `UnauthorizedRedirect` in
   `AppProviders` (`app-providers.tsx:67-84`): an effect calls `setUnauthorizedHandler`, and any RPC
   `401` (`rpc.ts:260-262`) calls it and `router.replace('/sign-in?next=…')`. Child effects (rail and
   screen RPCs) run *before* parent effects in React, so an early fast 401 can arrive while the global
   handler is still `null` → no redirect, signed-out visitor stuck in a broken shell. The handler is
   module-global, "last one wins", and its cleanup sets it to `null`; a second provider mount or
   StrictMode remount can unregister a live handler. Once a redirect is in flight nothing stops the
   rail/screen from issuing more RPCs (each 401 calls `replace` again), and the full shell chrome is
   painted before auth is known — unavoidable on a static export, but it can be made non-interactive
   and quiet instead of looking authoritative. `next` is same-origin by construction, but only
   `/sign*|/forgot|/reset` paths suppress the handler; keep `next` validation at the auth screens.
4. **Smaller self-inflicted faults.** `ThemeToggle` (lines 75–85) stores its 500 ms timeout nowhere:
   unmount leaks a `setTimeout`, and two quick toggles let the first timer strip
   `theme-transitioning` mid-second-transition (visual tear, no reduced-motion check). `aria-current`
   uses exact equality against `/app/`, so every deep link (`/app/<bot>/`, `/app/g/<id>/`,
   `/app/activity/`) shows no current item. `children?: ReactNode` renders an empty `<main>` when
   absent, which reads as a broken screen rather than a known empty state.

## Concrete hardening (v0.4.0) — each with its risk

- **H1. Segment error boundary at `app/app/error.tsx` (client).** `reset()` + a translated,
  token-only panel (`text-text-primary`, `bg-background-*-default`, composite type keys, BoardUI
  `Button`), mounted by the existing `/app` segment layout so the document shell survives.
  *Risk:* a boundary cannot catch a throw from the layout that renders it; rendering a second
  header in the fallback creates duplicate landmarks. Mitigate: keep the fallback landmark-free,
  assert one `main` with axe, and verify static-export error behaviour (GUESS: client throws are
  caught; server-render errors during `next build` are not — confirm before relying on it).
- **H2. Two independent narrow boundaries around the two slots.** Wrap `<BotRail/>` and
  `<main>{children}</main>` separately so each fails soft: rail → one-line "navigation unavailable,
  reload" link; screen → error panel with the rail still usable. Keep them as tiny class boundaries
  (or a BoardUI equivalent if one is installed), `componentDidCatch` reports a stable event without
  error text/PII; `reset()` remounts the failed slot only.
  *Risk:* a class boundary adds a few lines and a failing slot can loop if its cause persists —
  give `reset()` a retry cap and log once per error identity; do not expose `error.message` to users.
- **H3. Make rail degradation honest.** Give `refreshRoster` a never-rejecting contract or attach
  `.catch()` at the call site; after a short grace period show a translated, `aria-live="polite"`
  notice when both roster and directory are empty/failed; mark directory fallback data as possibly
  stale. Keep existing `AbortController` cleanup on unmount.
  *Risk:* notice flicker on slow links and golden drift — delay it, reuse existing `shell.rail.*`
  keys, replay rail goldens in light/dark and RTL.
- **H4. Register the 401 handler before children mount.** Move registration out of the parent effect
  into a synchronous module/provider-render registration in `rpc.ts`/`AppProviders` (stable callback
  reading `window` at call time), keep the auth-path opt-out, and make the redirect idempotent via a
  single in-flight flag that `call()` checks to suppress further 401 RPCs during the redirect.
  *Risk:* module-scope code must never read `window` during SSR; sign-in pages must still own their
  inline errors. Test a fast 401 arriving before child effects complete.
- **H5. Tame `ThemeToggle` and nav state.** Keep the timeout id in a ref, clear it on unmount and on
  re-toggle, skip the transition class under `prefers-reduced-motion`, and derive `aria-current` from
  a path-prefix check so deep links mark the Threads item.
  *Risk:* prefix matching must not mark `/app/activity` as Threads — enumerate the known app
  prefixes or reuse the route classifier from worker #079.

## Explicitly rejected / deferred

- Turning the shell into an auth gate (server redirect, session probe) — contradicts ADR-013 static
  export and belongs to `/` + `AppProviders`; H4 fixes the race, not the responsibility boundary.
- A data-fetching rewrite of the rail (React Query/SWR) — larger dependency; H3 is enough locally.
- Changing the landmark/class contract (`boma-focus`, `boma-target`), BoardUI tokens, or copy.

## Success criteria

Playwright/unit: a throwing child leaves header + rail + single `main` intact and `reset()` recovers;
a throwing rail leaves the screen usable; a delayed/failed `groupsList`/`botsList`/`refreshRoster`
produces one honest notice and no unhandled rejection; a 401 before effect registration still
redirects once, with no redirect storm; double theme toggle and unmount leave no stray timer; axe AA
clean (360 px, light/dark, RTL), UI goldens and bundle budget from `docs/PLAN.md` unchanged.

CONFIRMED: file reads above plus absence of any `error.tsx`/`global-error.tsx` under `apps/web/app`.
GUESS: actual 401-before-effect timing, static-export error-boundary behaviour, golden landmark
assertions and golden sensitivity to the rail notice — verify before implementation.
