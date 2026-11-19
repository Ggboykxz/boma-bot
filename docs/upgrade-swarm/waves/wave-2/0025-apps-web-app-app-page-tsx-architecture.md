# 0025 — app/app/page.tsx — architecture (v0.4.0)

Worker #025 · module `apps/web/app/app/page.tsx` · lens: architecture · READ-ONLY.
Proposals only; no code in this change.

## What the file does

`page.tsx` is a 16-line server wrapper (`<AppShell><AppEntry /></AppShell>`) for `/app`.
The real behavior lives one level down:

- **`AppEntry`** (`components/shell/app-entry.tsx`, `"use client"`) is the static-export
  dispatcher (ADR-013). It reads `botIdFromPath()` via `useSyncExternalStore` with a no-op
  subscribe. First paint is always `ThreadsHome`; the thread screen takes over post-hydration.
- **`ThreadsHome`** is a placeholder: one `text-headline-medium` heading + one
  `shell.placeholder` paragraph. No bot list, no data, nothing the rail shows.
- The home says "look at the rail" while the rail (`bot-rail.tsx`) is the *only* place with
  the roster (`useSyncExternalStore(subscribeRoster, …)` + `lib/bot-directory`), groups, and
  the activity entry (REQ-011).
- **Dispatch gap.** `botIdFromPath` matches `^/app/([^/]+)/?$`; anything else under `/app`
  (e.g. `/app/<garbage>/<x>`) silently renders `ThreadsHome`, never `NotFoundScreen`.
- **Naming.** `ThreadsHomePage`/`ThreadsHome` vs spec §8.1's `/app` ("Shell with the selected
  bot's thread") — three names, none saying "after-login landing"; the post-login decision
  point is split across this route and `LandingScreen` (`/`).

## Structure today vs. after v0.4.0

After: `/app` = "landing after login" — a real home screen inside the shell (rail intact),
not a placeholder and not a hidden 404. `page.tsx` stays a thin server wrapper; a new
`components/shell/app-home.tsx` owns the screen; `AppEntry` dispatches home / thread /
not-found explicitly.

BoardUI constraints that hold throughout: semantic tokens only
(`bg-background-secondary-default`, `text-text-primary/-secondary`,
`border-separator-border`, `text-foreground-icon-*`), composite type utilities
(`text-headline-medium`, `text-title-3-semibold`, `text-caption-1-semibold`), `rounded-3xl`
panels with `border-border-button-default`, `cx()` for merging, `@remixicon/react` icons as
component references, `boma-focus` focus ring, RTL-logical properties, dark mode via tokens.

## Proposed upgrades for v0.4.0 (in dependency order)

### U1. Promote `ThreadsHome` to a real `AppHome` screen (bot list + rail entries)
Replace the placeholder with the after-login landing: a heading, then the same roster data
the rail uses (`useSyncExternalStore(subscribeRoster, listRoster, serverRoster)` +
`refreshRoster()` on mount — no new wire contract needed, `roster` RPC already exists),
rendered as a list of bot rows (name, unread `rounded-full` count badge, snoozed shelf)
plus a "New bot" CTA (reuse `BotCreateForm`'s trigger styling, not a lookalike) and an
"Activity" entry linking to `/app/activity/` (rail already links there; the home makes it
discoverable). Rows are `Link`s to `/app/<botId>/`. Empty state mirrors the rail's
`shell.rail.bots.empty` copy. Skeleton rows shaped like messages while `roster === null`.
- Risk: duplication of rail logic drifts (two lists to keep in step); goldens count rows in
  the rail, not the home, but axe/WCAG must hold both. Mitigate: extract one `BotRow`
  component shared by rail and home (rail passes `current`, home never does); no second
  data path.

### U2. Make `AppEntry` an explicit three-way dispatch (home / thread / not-found)
`botIdFromPath` returns `null` for home *and* for unknown junk — that ambiguity is the 404
bug. Change the dispatcher to a pure `classifyAppPath(): "home" | { botId: string } | "not-found"`
(two-segment group paths `/app/g/<id>/` route to their own static page and never reach this
shell; truly unknown shapes render `NotFoundScreen`, which already exists and is one
heading + one way back). Keep `useSyncExternalStore(…, serverSnapshot = "home")` so first
paint is deterministic and matches SSR.
- Risk: static-export parity — the Rust static handler decides which paths reach this shell
  (ADR-013); a misclassified path could 404 where v0.3.0 showed home. Mitigate: table-driven
  unit test of `classifyAppPath` against the ADR-013 fallback contract; goldens + Playwright
  for `/app/`, `/app/<bot>/`, `/app/g/<id>/`, `/app/nope/x`.

### U3. Ship-shape the after-login redirect instead of double-deciding
`LandingScreen` (`/`) already probes `botsList` and `router.replace`s to `/app/` or
`/sign-in`. With a real home, add the missing piece: `/app` should deep-link to the most
recent thread when one exists (if the roster reports `lastOpenedBotId` — GUESS: it may not
today; check the roster view) behind a preference, defaulting to the home. If the wire does
not carry it, do **not** invent it — keep manual selection. Either way, the home must stay
the canonical first paint (no redirect loops, no flash).
- Risk: auto-redirect AMBUSHES the landing the user wants (spec §8.1 says `/app` is the
  threads home; silently jumping into a thread is a behavior DEVIATION). Mitigate: ship U3
  only as an opt-in setting, or drop it; default stays "always land on home". Needs owner
  sign-off if it changes default behavior.

### U4. Fold home state into a small `useAppHome` module; align naming with the spec
`ThreadsHomePage`/`ThreadsHome` → `AppHomePage`/`AppHome` (file stays
`components/shell/app-home.tsx`), with a `useAppHome()` hook owning `roster === null`
(loading), `refreshRoster()` on mount, and focus management (heading focusable with
`tabIndex={-1}` so keyboard users land on something meaningful after hydration, matching
`main` landmark in `AppShell`). All copy moves to the i18n catalogue (there are already
EN/FR/HE entries for `shell.nav.threads` and `shell.placeholder` — replace them, keeping
RTL mirrored). Delete `shell.placeholder` or repurpose it; no hardcoded strings.
- Risk: rename breaks import sites and i18n goldens that assert catalogue keys. Mitigate:
  one PR, `grep` for the old keys, catalogue diff reviewed by hand; French/Hebrew strings
  updated in the same commit.

### U5. Reduced-motion + scroll contract for the home list
The rail animates rows (`animate-row-slide-in`, `animate-number-fade`); the home list must
respect `prefers-reduced-motion` (no entrance animation, or opacity-only) and use the
`ScrollFade`/`useScrollFade` recipe from `@/components/foundations/scroll-fade/scroll-fade`
for its internal scroll region — not a hand-built blur. Home stays inside `AppShell`'s
`main` (no new scroll container unless the bot list exceeds the viewport).
- Risk: ScrollFade overlay mis-layered (opacity on parent breaks blur at zoom) or motion
  leaks past the reduced-motion media query. Mitigate: reuse the registry item verbatim;
  Playwright asserts `prefers-reduced-motion` emulation and scroll-top/reset behavior in
  light and dark.

## Execution order & gate

U1 → U2 (one v0.4.0 theme: real landing + explicit dispatch) → U4 (naming/i18n, same PR or
immediately after) → U5 (polish, gated on U1). U3 is optional and owner-gated. Every step:
UI goldens green, Playwright REQ suite green (shell REQ-001/011 family), axe AA clean at
360 px + light/dark + RTL sample, bundle budget (docs/PLAN.md) not exceeded, `page.tsx`
diff stays ≤ a thin server wrapper.

## What not to touch

`AppShell` landmarks (goldens assert them); ADR-013 static-export fallback and
`generateStaticParams` in `app/app/[botId]/page.tsx`; the `useSyncExternalStore` server
snapshot pattern (deterministic first paint); `bot-directory` vs `roster` split semantics;
`boma-target`/`boma-focus` class contract; spec §8.1 route table (no new routes).

CONFIRMED by reading source (page, app-entry, threads-home, app-shell, bot-rail,
landing-screen, bot-directory, spec §8.1/§8.3, i18n.ts). Risk notes GUESS until
goldens/Playwright replay; roster `lastOpenedBotId` unverified (GUESS).
