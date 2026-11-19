# 0012 — app/app/[botId]/page.tsx — reliability (v0.4.0)

Worker #012 · module `apps/web/app/app/[botId]/page.tsx` · lens: reliability · READ-ONLY.
Anchors: `spec/08-ui-ux.md §8.1/§8.3`, REQ-001/002/005/009/020/021 · golden
`spec/golden/ui/REQ-001-send-message.json` · ADR-013.

## What the route is today (CONFIRMED)

`page.tsx` is a 36-line async server component: it awaits `params` (a `Promise` in this
Next version), renders `<AppShell><ThreadScreen botId={botId} /></AppShell>`, and pins
`generateStaticParams()` to `bot`/`bot_golden`. Production deep links for any other id never
run it: the Rust static handler (`resolve_prefix_fallback`, `crates/api/src/router.rs`) maps
`/app/<id>/` → `app/index.html`, where `components/shell/app-entry.tsx` dispatches via
`botIdFromPath()`. The route is the dev path plus two export artifacts; `ThreadScreen` owns
load, live stream, send, reactions, snooze.

## Fragility findings (CONFIRMED unless noted)

1. **SSR params unvalidated.** `params` is awaited and `botId` flows raw into the client
   tree and `<h1>{bot?.name ?? botId}</h1>`. No length/charset/emptiness check, no
   `notFound()` for an obviously bad id, no explicit `dynamic`/`dynamicParams` — export
   behavior rides this Next version's defaults (AGENTS.md warns: this is not the Next.js you
   know; the rules live in `node_modules/next/dist/docs/`, not in training data).
2. **Malformed percent-encoding crashes the shell — no boundary anywhere.**
   `lib/bot-directory.ts:60` `botIdFromPath()` calls `decodeURIComponent` with **no**
   `try/catch` (sibling `lib/loc.ts` `match()` catches and returns `null`). `/app/%/` throws
   `URIError` inside `useSyncExternalStore`'s `getSnapshot` in `AppEntry` — an uncaught
   **render** error. No `error.tsx`/`global-error.tsx` exists under `apps/web/app/` (`find`
   confirmed). Rust's `web_fallback` can only answer `404.html` for unresolved paths. Blank
   page, no retry.
3. **Error boundary shape missing at both levels.** `ThreadScreen`'s internal `Phase` machine
   covers **RPC** failure (loading/ready/error + retry) but nothing catches render-time throws
   in the room or in the `AppEntry` dispatch above it. The segment has no `error.tsx`; the root
   has no `global-error.tsx` (only `not-found.tsx`). REQ-021 ("failures are user-legible") is
   met for API errors, violated for render errors.
4. **Two code paths for the same URL; the id is sourced twice.** Prod: regex +
   `decodeURIComponent` in the browser; dev / pinned files: server `params`.
   `ThreadScreen.load()` uses `botIdFromPath() ?? botId`, but `send()` passes the prop to
   `threadsSend`. If the two diverge, messages go to one bot while the transcript shows
   another. `clientNonce` is `web-${Date.now()}-${Math.random()}` — fine, not `randomUUID()`.
5. **Golden pins hardcoded, export path untested.** Playwright (`webServer: npm run dev`)
   replays REQ-* on dev; `out/app/bot_golden/index.html` is never exercised, and nothing asserts
   the export emits exactly the declared pins. The "dev refuses unknown params" comment is
   unverified too.
6. **Optimistic row collision (REQ-002 seam).** Optimistic send uses
   `seq = lastSeq.current + 1`; a server renumbering doubles rows and reorders the `<ul>` the
   goldens count. (Architecture lens 0011 covers stream gap detection; the boundary correctness
   depends on it.)
7. **Adjacent, out of scope — flag for the group-thread proposer:** unknown
   `/app/g/<groupId>/` links also fall through to `app/index.html`, where `AppEntry`'s
   `^\/app\/([^/]+)\/?$` cannot match two segments → renders `ThreadsHome`, never the group
   screen. Same dispatch fragility, one route family over.

## Hardening for v0.4.0 (in order)

- **H1. Validate at the page boundary.** One shared `parseBotId()`: trim, cap ~128 chars,
  reject `/ \ %`/control chars, else opaque-pass. Invalid → `notFound()`. Thin server page.
- **H2. Pin export semantics explicitly.** `export const dynamic = "force-static"` and
  `dynamicParams = false` — confirm names/legality in `node_modules/next/dist/docs/` first.
  Loud at build time, never silent at runtime.
- **H3. One id source in the client.** `ThreadScreen` derives the id once from
  `botIdFromPath()` for load **and** send; the prop is an SSR hint. Fix `botIdFromPath()` to
  `try/catch` the decode (mirror `lib/loc.ts`) — kills the throw class at the source,
  zero behavior change for valid ids.
- **H4. Add the two missing boundaries.** `app/app/[botId]/error.tsx` (client, catalogue
  copy, `role="alert"`, focusable retry → `reset()`) and `app/global-error.tsx` (root,
  `<html><body>`). Copy differs from the in-log `thread.error.load` panel; keep §8.3 composer
  guidance.
- **H5. Test the export path, not only dev.** CI: `next build`, assert
  `out/app/{bot,bot_golden}/index.html` + shell marker; Playwright project (opt-in via
  `BOMA_EXPORT_E2E=1`) serving the Rust binary (`BOMA_WEB_ROOT`), running REQ-001/REQ-005 on
  `/app/bot_golden/`.

## Test plan (REQ-named, per tools/trace_check)

New `apps/web/tests/bot-route.spec.ts` (Playwright) + a node export-assertion script:

- `REQ-001 bot route renders the shell and one row at /app/bot_golden/` — `role="log"`
  visible, exactly 1 `listitem` (the golden REQ-001 seed).
- `REQ-001 invalid bot id renders the not-found screen, not a blank page` — `/app/%2e%2e/`,
  overlong id → `*` not-found surface (H1).
- `REQ-002 send issues unique nonces per attempt` — intercept `threads.send`; two sends,
  distinct `clientNonce` (post-H3: UUID-shaped).
- `REQ-005 ordered history survives reload of the bot route` — goto, reload, `seq` ascending,
  no duplicate rows.
- `REQ-020 malformed percent-encoding does not crash the shell` — `/app/%/`,
  `/app/%E0%A4%A/` → not-found or home with a focusable control; fails today (GUESS).
- `REQ-021 load failure shows legible error and retry recovers` — existing in-log path,
  kept green.
- `REQ-021 render crash is contained by the route error boundary` — unit test on
  `error.tsx` with a throwing child: `role="alert"` + `reset()` invoked. (GUESS: RTL-style
  unit test; Playwright can't force a render crash without a test-only flag.)
- `REQ-057 a11y bot route and boundary states pass axe AA` — extend `a11y.spec.ts`.
- Export assertion (node, CI): `out/app/bot{,_golden}/index.html` exist + shell marker —
  gates H2/H5.
- REQ-017 regression: approval card still gates the composer (unchanged, stay green).

## Risks

- **R1** `dynamic`/`dynamicParams` names/semantics are this-Next-version specific; a wrong
  flag breaks `next build` or silently drops the export. Read `node_modules/next/dist/docs/`
  first; build the export in CI before merge.
- **R2** `botIdFromPath` is shared by the whole `/app` family (AppEntry, ThreadsHome, panels);
  goldens + axe must stay green across `a11y.spec.ts` ROUTES.
- **R3** Two error surfaces can double-render; retry semantics differ (`reload` token vs
  `reset()`). Distinct copy + tests mitigate.
- **R4** Unknown ids rendering not-found instead of the home→dispatch is an externally visible
  change → `docs/PARITY.md` + owner approval before shipping (GUESS: approved; ADR-013 intent).
- **R5** Export e2e lane needs the Rust binary and export together; flake risk on
  `BOMA_WEB_ROOT`; keep non-blocking until stable, then required.
- **R6** `crypto.randomUUID` needs a secure context (loopback OK); feature-detect with a
  fallback to the current nonce shape.
- **R7** Boundary components add bytes; measured against the budget in `docs/PLAN.md`.

## Non-goals / notes

No changes to `generateStaticParams` pins, the ADR-013 fallback, the wire contract, or the
live stream (0011/U3); no new deps; no spec edits. Read this Next version's docs before
implementing. CONFIRMED = read this session; test outcomes, R4/R6 are GUESS until run.
