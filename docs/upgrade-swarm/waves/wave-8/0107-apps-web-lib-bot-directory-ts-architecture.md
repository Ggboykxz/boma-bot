# 0107 — apps/web/lib/bot-directory.ts — architecture (v0.4.0)

Worker #107 · module `apps/web/lib/bot-directory.ts` · lens: architecture · READ-ONLY.
Proposals only; no code in this change. Line refs against `before-m5`.

## What the file does (structure today)

A 67-line module holding two unrelated concerns behind one filename:

1. **A client-only external store** for "bots the client has met": module-global
   `bots` + `listeners`, with `registerBot` (upsert-by-id), `listBots`, `subscribeBots`,
   and `serverBots` returning a shared `EMPTY` constant for deterministic SSR
   (the `useSyncExternalStore` trio). Written only from `thread-screen.tsx:111`
   after `botsGet`; read only from `bot-rail.tsx:86`, where it is the **fallback**
   shown while `roster === null` (`bot-rail.tsx:168-183`).
2. **A routing helper** `botIdFromPath` (`:60-67`): regex `^/app/([^/]+)/?$` over
   `window.location.pathname`, one-segment match, `decodeURIComponent`. Shared by
   `app-entry.tsx:26`, `thread-screen.tsx:103`, `bot-rail.tsx` path logic, `threads-home`.

**Stale contract (CONFIRMED).** The header says "the wire contract has no `bots.list`
procedure". It does: `botsList` exists (`lib/api/rpc.ts:422`, REQ-009) and a second,
authoritative external store `lib/roster.ts` fetches it. The directory is now a
*fallback layer*, not the roster. The two stores duplicate the listener/snapshot
boilerplate and differ in their empty state (`[]` vs `null`).

Consumers are stable; the seam is safe to reshape behind unchanged exports.

## Proposed upgrades (v0.4.0)

### U1. Split directory store from path parsing (single responsibility)
Move `botIdFromPath` out into a pure `lib/app-route.ts` and leave `bot-directory.ts`
as the met-bots fallback store only. Keep a thin `export { botIdFromPath }` shim from
the old path for one release — it is imported by at least four screens (`app-entry`,
`thread-screen`, `bot-rail`, `threads-home`), so break it in a later PR. This aligns
with wave-5 #0079's `classifyAppPath`.
- Risk: churn across four consumers and a possible re-export cycle if the route module
  ever imports the store. Mitigate: make `app-route.ts` dependency-free (pure strings),
  migrate imports in one mechanical PR, then drop the shim only after grep shows zero
  remaining `bot-directory` imports of the helper.

### U2. Reconcile the two external stores into one tiny factory
`roster.ts` and `bot-directory.ts` both implement `listeners` + notify + a server
snapshot. Extract a private `createExternalStore<T>(initial, serverSnapshot)` (or an
in-repo `lib/external-store.ts`) used by both, parameterising the `[]` vs `null`
initial/snapshot difference. Formalise the hierarchy in code: roster = authoritative,
directory = fallback used only until the roster answers.
- Risk: over-abstraction for ~20 lines, and a generic could erase the meaningful
  `null` ("not loaded") vs `[]` ("loaded, empty") distinction the rail depends on.
  Mitigate: keep the generic to listener + notify only; retain each store's own
  snapshot functions and types. No behavior change to the rail.

### U3. Make the singleton injectable for isolated tests
Today the store is module-global mutable state, so state leaks between unit tests and
cannot be seeded without mutating the live singleton. Export
`createBotDirectory(initial?: readonly BotSummary[])` returning the five operations,
and define the current exports as the default instance. Unit-test the upsert,
dedupe, rename, and notify paths against fresh instances.
- Risk: API surface grows and consumers might construct stray instances ("two
  directories"). Mitigate: keep the named singleton exports as the only documented
  entry point; export the factory as `@internal` for tests. Note SSR: module-global
  state is shared across requests on the server, but writes are client-only
  (`"use client"` in `thread-screen`), so keep `serverBots()` constant.

### U4. Rewrite the header comment and name the layer honestly
Replace the false "no `bots.list`" preamble with the real two-tier model and a
"when to use which" note (fallback vs roster), citing `api/rpc.ts` + `lib/roster.ts`.
Consider renaming the file to `lib/met-bots.ts` / `fallback-directory.ts` to match
its role, behind a re-export shim.
- Risk: doc-only change is safe, but a rename multiplies import churn; keep the file
  name in v0.4.0 and rename only if the team accepts the PR-level churn. Mark claims
  CONFIRMED from reads of `rpc.ts`/`roster.ts` per the docs policy.

### U5. Keep path parsing total and pure at the seam
As part of U1, make the extracted route helper a total function: never throw, return
`string | null` (or the discriminated `AppRoute` from #0079), and handle malformed
`%` ids. Keep it free of `window` by accepting `pathname` explicitly; the caller
supplies `window.location.pathname`.
- Risk: reliability worker #108 owns the `decodeURIComponent` crash fix — coordinate
  so the guard lands once. Preserve the `string | null` signature of the shim until
  all consumers move to `AppRoute`, or the shared callers break.

## Execution order & gate

U4 (free, no behavior) → U1 (split + shim) → U5 (guard, coordinate with #108) →
U2 (factory) → U3 (injectable singleton, tests). Every step: unit tests for upsert/
dedupe/notify and route shapes green, Playwright shell suite green, axe AA clean,
UI goldens green, bundle budget (docs/PLAN.md) unchanged, and no consumer behavior
change — the directory remains a fallback only.

## What not to touch

ADR-013 static fallback / `generateStaticParams`; the `useSyncExternalStore`
deterministic server snapshot; `bots.list` wire shape or REQ-009 semantics; the
`boma-focus`/`boma-target` class contract; `NotFoundScreen` copy/tokens; spec §8.1/§8.3
route table.

CONFIRMED by reading `bot-directory.ts`, `roster.ts`, `api/rpc.ts`, `bot-rail.tsx`,
`app-entry.tsx`, `thread-screen.tsx`, and briefs 107/108. GUESS: consumer preference
for a rename and the exact `AppRoute` shape from wave-5 #0079 — verify before coding.
