# 0081 — apps/web/components/shell/app-shell.tsx — architecture (v0.4.0)

Worker #081 · module `apps/web/components/shell/app-shell.tsx` · lens: architecture · READ-ONLY.
Proposals only; no code in this change. Line refs against `before-m5`.

## What the file does

`AppShell` (98 lines, `"use client"`) is the `/app` frame: a `<header>` holding one primary-nav
`Link` plus an inline `ThemeToggle`, then a flex body that stacks `BotRail` over `<main>{children}</main>`
on mobile and places them side by side at `lg` (`min-h-dvh`, `max-w-5xl`, `gap-6`/`lg:gap-8`).
Cross-cutting providers do **not** live here: `app/layout.tsx:71` wraps everything in
`AppProviders` → `DirectionProvider` + `UnauthorizedRedirect` (`components/providers/app-providers.tsx`).
Theme is a second, separate path: the pre-paint `BOOT_SCRIPT` in `layout.tsx:44-50` reads
`boma-theme`/`boma-locale` and writes `.dark`/`lang`/`dir` on `<html>`, while `ThemeToggle` here
writes the same DOM and storage directly and freezes transitions via `.theme-transitioning`
for a hardcoded 500 ms (`globals.css:81-84`). One concern, three owners.

## Composition problem (v0.3.0)

1. **Structure and behaviour share one client island.** Marking the whole shell `"use client"` pulls
   the static header/body structure, and therefore every `children` screen, into the client bundle.
   Only the toggle and the rail are genuinely interactive.
2. **No theme provider.** Theme is observable only by reading `document`/`localStorage`. That is
   untestable in isolation, duplicated in the boot script, and leaves no place to subscribe to a
   theme change — a view component (`ThemeToggle`) is acting as the store.
3. **"Current route" has three matching rules.** Header uses `pathname === item.href` (line 47),
   so the single `/app/` link loses `aria-current` on every thread/group/activity path; `BotRail`
   uses `===` for bots (`:188`) but `startsWith` for activity (`:321`); `AppEntry` parses the path
   itself. Active-state policy is fragmented across the tree.
4. **Landmark ownership is implicit.** `AppShell` emits the sole `<main>` (`:58`), but
   `NotFoundScreen` emits its own (see #0079). The rail adds three more `<nav>` landmarks before
   `main` in DOM order with no skip link, so keyboard users tab through the whole rail first.
5. **RTL intent is asserted in a comment, not enforced.** The rail is called "left" (`:28`) yet
   `flex-row` mirrors it; the header toggle relies on `ms-auto` (`:92`). Direction is a provider
   concern and deserves a test, not a docstring.

BoardUI constraints hold throughout: keep semantic tokens (`text-text-primary/-secondary/-tertiary`,
`bg-background-secondary-default/-hover`, `border-separator-border`, `text-foreground-icon-*`),
composite types (`text-body-medium`, `text-caption-1-semibold`), `cx()` for merges, Remixicon
component refs, logical `ms`/`ps`, dark mode via `.dark` tokens only, `boma-focus`/`boma-target`.

## Proposed upgrades (v0.4.0)

### U1. Move cross-cutting state into root providers; make the shell structural
Add a thin `ThemeProvider` to `AppProviders` (root) exposing `{theme, toggle}` from a single
`lib/theme.ts` store (`get`/`set`/`subscribe`, `STORAGE_KEY`, `TRANSITION_MS` shared with the boot
script). `ThemeToggle` becomes a consumer with no `document`/`localStorage` writes. Keep
`DirectionProvider` and `UnauthorizedRedirect` where they are.
- Risk: moving the provider changes hydration timing and could reintroduce a theme flash on the
  static export. Mitigate: keep the boot script as the pre-paint source of truth, have the provider
  read it on mount via `useSyncExternalStore` (same pattern as `AppProviders`), and verify
  no-flash in light/dark before removing any inline logic.

### U2. Split the client boundary
Keep `AppShell` as a thin server-renderable structure and extract only the interactive pieces
(`ThemeToggle`, already-client `BotRail`) as islands; pass `children` through untouched. Under the
static export (ADR-013) the win is smaller than a dynamic app, so frame this as clarity, not speed.
- Risk: a "server" shell in a statically exported app still ships as HTML — do not claim bundle
  savings that cannot be measured. Mitigate: record the actual JS delta in docs/PLAN.md; if flat,
  keep the split solely for separation of concerns and drop any performance wording.

### U3. One route-matching policy
Introduce a pure `lib/nav.ts` with `isActive(href, pathname)` (segment-aware: `/app/` active for
`/app/<bot>/` and `/app/g/<id>/`) and use it in `AppShell`, `BotRail`, and `AppEntry`. Keep the
`boma-target`/`boma-current` class contract unchanged.
- Risk: goldens pin exact `aria-current` attributes; broadening the header match changes first-paint
  state on deep links. Mitigate: table-driven unit tests for `/app/`, `/app/<id>/`, `/app/g/<id>/`,
  `/app/activity/`, and replay the shell goldens before merging.

### U4. Explicit landmark composition + skip link
Document that `AppShell` owns the **only** `<main>`, add a visually-hidden "skip to content" link as
the first focusable node targeting `#main-content`, and give rail `<nav>`s stable labels that do not
collide with the header nav. Coordinate with #0079 so `NotFoundScreen` renders *inside* this main.
- Risk: a new focusable node and anchor change the tab order and axe/golden snapshots; a duplicated
  `id` if a screen also uses `main-content`. Mitigate: `id` set once on `main`, assert a single
  `main` and one skip target with axe, and check the 360 px and RTL tab paths.

### U5. Test-first and direction coverage
Add unit tests for `lib/theme.ts` and `lib/nav.ts`, plus Playwright coverage for toggle persistence,
active nav on each route shape, skip-link reachability, and `dir=rtl` ordering of rail/main/toggle.
- Risk: over-extraction (three tiny modules) and flaky theme tests around the 500 ms freeze.
  Mitigate: keep modules co-located and exported for tests; fake timers for the freeze, assert
  final class only, and do not touch `ThemeToggle`'s copy or icon swap.

## Execution order & gate

U1 → U3 (policy, shared by shell and rail) → U4 (landmarks, gated on #0079 placement) → U2 (may be
dropped if unmeasurable) → U5 tests throughout. Each step: unit suites green, Playwright shell suite
green, axe 2.2 AA clean (single `main`, 360 px, light/dark, RTL), UI goldens green, JS bundle
budget (docs/PLAN.md) unchanged.

## What not to touch

ADR-013 static fallback; `BOOT_SCRIPT` semantics (pre-paint theme/locale); `DirectionProvider` and
`UnauthorizedRedirect` behaviour; `BotRail` data flow (`useSyncExternalStore`, roster/directory);
BoardUI semantic tokens/composite types; `not-found-screen.tsx` copy; spec §8.1/§8.3 route table;
`boma-focus`/`boma-target` class contract.

CONFIRMED by reading `app-shell.tsx`, `bot-rail.tsx`, `app/layout.tsx`, `app-providers.tsx`,
`globals.css:81-84`, sample #0079. GUESS: bundle delta from U2 and golden `aria-current` assertions —
verify with /ui-verify and golden replay before implementation.
