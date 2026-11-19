# 0079 — apps/web/components/shell/app-entry.tsx — architecture (v0.4.0)

Worker #079 · module `apps/web/components/shell/app-entry.tsx` · lens: architecture · READ-ONLY.
Proposals only; no code in this change. Line refs against `before-m5`.

## What the file does

`AppEntry` is the 33-line `"use client"` dispatcher for the whole `/app` subtree under the static
export (ADR-013). The Rust static handler maps unknown `/app/*` paths to `app/index.html`, so the
route param is unavailable and the screen reads the URL via `useSyncExternalStore` with a no-op
`subscribe` and `botIdFromPath()` (`lib/bot-directory.ts:60-67`, regex `^/app/([^/]+)/?$`).
`botId === null` → `ThreadsHome`; otherwise → `<ThreadScreen botId>`. In dev the dynamic routes
(`[botId]/page.tsx`, `g/[groupId]/page.tsx`) answer directly and this dispatch never runs.

## The bug (brief 025, defect 0012/0014/0024 family)

`null` means two different things: "confirmed home" and "unrecognised path". Every shape the regex
does not match — `/app/g/<id>/`, `/app/foo/bar/`, `/app/%zz/` — silently renders `ThreadsHome`.
`NotFoundScreen` already exists (`components/shell/not-found-screen.tsx`) and is never reached from
this entry. Architecturally the dispatcher conflates classification with rendering and delegates
classification to a helper that answers one narrow question, so there is no place to express
"group" or "not found".

BoardUI constraints hold throughout: dispatch to existing BoardUI screens rather than hand-build
error UI; keep semantic tokens (`text-text-primary`, `text-text-secondary`, `bg-background-*`),
composite types (`text-title-2-medium`, `text-body-medium`), `ButtonLink` from
`@/components/base/buttons/button`, `cx()`, `@remixicon/react` refs, RTL, dark mode via tokens.

## Proposed upgrades (v0.4.0)

### U1. Pure, total path classifier (core fix)
Replace the `botIdFromPath`-driven branch with a pure `classifyAppPath(pathname): AppRoute` returning
a discriminated union: `{kind:"home"} | {kind:"thread", botId} | {kind:"group", groupId} |
{kind:"not-found"}`. Keep `useSyncExternalStore` but snapshot the union, not a nullable id. The
server snapshot must be explicit and deterministic (see U3). Colocate the classifier in a pure
`lib/app-route.ts` (`botIdFromPath` becomes a thin compatibility wrapper — it is shared by the rail
and `ThreadsHome`, so do not break it in the same PR).
- Risk: static-export parity — the Rust handler decides which paths reach this shell (ADR-013); a
  new classification could 404 where v0.3.0 showed home. Mitigate: table-driven unit tests for
  `/app/`, `/app/<id>/`, trailing slash, `/app/g/<id>/`, `/app/nope/x`, malformed `%`.

### U2. Route group paths to GroupThreadScreen, not home
`/app/g/<id>/` reaches this shell in the static export (non-pinned ids have no pre-rendered page);
the two-segment regex never matches it. Add the group arm to the classifier and dispatch to
`GroupThreadScreen` (mirror `app/app/g/[groupId]/page.tsx`), or — if group work is another wave —
to an explicit not-found state. No silent home either way.
- Risk: importing `GroupThreadScreen` widens this client bundle and couples the dispatcher to the
  group feature; in dev the dynamic route already handles it. Mitigate: lazy-load, verify `router
  .push`-in-export behavior (GUESS: full document load → fallback path), and gate on wave-2 #0024.

### U3. Separate "resolving" from "home" in the pre-hydration snapshot
The current server snapshot `null` forces first paint to `ThreadsHome` on every deep link, then
swaps post-hydration — wrong landmarks for a frame. Add a `{kind:"resolving"}` arm and render a
neutral, quiet shell-body skeleton for it; reserve `ThreadsHome` for confirmed `{kind:"home"}`.
This makes the union total and removes the home/not-found ambiguity at the source.
- Risk: SSR/hydration snapshot mismatch and goldens that pin `/app/` landmarks. Mitigate: keep the
  static server snapshot stable, skeleton `aria-hidden` with no tab stops, replay UI goldens for
  `/app/` and `/app/<bot>/` (pairs with wave-2 #0026 F6).

### U4. Render NotFoundScreen for truly unknown shapes
Wire `{kind:"not-found"}` to the existing `NotFoundScreen` (one heading, `ButtonLink` home) instead
of `ThreadsHome`. Keep it as the shell body or a top-level page deliberately — `NotFoundScreen`
emits its own `<main>`, so nesting it inside `AppShell`'s `main` creates duplicate landmarks.
- Risk: landmark/a11y regression and golden drift. Mitigate: decide placement once, assert a single
  `main` with axe, and do not change the screen's copy or BoardUI tokens.

### U5. Keep dispatch a thin renderer; test-first
After U1-U4 `AppEntry` should only read the union and switch — no path parsing, no data. Add a
unit suite for the classifier plus Playwright coverage for all four route shapes and the 360 px /
light+dark / RTL checks; no dispatch logic in the component.
- Risk: over-extraction adds a module for 4 lines. Mitigate: keep `lib/app-route.ts` tiny and
  co-located; if the team prefers, inline the pure function above `AppEntry` but still export it
  for tests.

## Execution order & gate

U1 → U2 + U4 (same theme: explicit not-found/group, no silent home) → U3 (snapshot, gated on goldens)
→ U5 tests throughout. Every step: classifier unit tests green, Playwright REQ-001/010/011 shell
suite green, axe AA clean (single `main`, 360 px, light/dark, RTL), UI goldens green, bundle budget
(docs/PLAN.md) unchanged, and `page.tsx` still a thin server wrapper.

## What not to touch

ADR-013 static fallback and `generateStaticParams` in `[botId]`/`[groupId]` routes; the
`useSyncExternalStore` server/client snapshot pattern (keep it deterministic); `bot-directory` vs
`roster` split semantics; `NotFoundScreen` copy/tokens; spec §8.1/§8.3 route table (no new routes);
`boma-focus`/`boma-target` class contract.

CONFIRMED by reading `app-entry.tsx`, `bot-directory.ts`, `app/app/page.tsx`, `[botId]`/`[groupId]`
pages, `not-found-screen.tsx`, briefs 025/0024/0026. GUESS: export `router.push` behavior, golden
landmark assertions, bundle impact of the group import — verify before implementation.
