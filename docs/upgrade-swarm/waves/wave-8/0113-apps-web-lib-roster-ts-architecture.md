# 0113 — apps/web/lib/roster.ts — architecture (v0.4.0)

Worker #113 · module `apps/web/lib/roster.ts` · lens: architecture · READ-ONLY proposal.
Line refs against `before-m5`. No product code edited; no wire/API change proposed.

## What the file does

46-line `"use client"` module: the server roster as a hand-rolled external store for
`useSyncExternalStore`. Module state = `listeners: Set` + `roster: readonly RosterBotView[] | null`
(L13-14) plus a frozen `SERVER_ROSTER = []` SSR snapshot (L15). Four exports: `listRoster()` (client
snapshot, `null` until first answer, L18-20), `serverRoster()` (empty SSR snapshot, L23-25),
`subscribeRoster()` (L28-33), `refreshRoster()` (L36-46) which calls `botsList()` from
`lib/api/rpc.ts`, assigns `reply.bots`, notifies listeners and swallows every error.

Confirmed consumers (4): `components/shell/bot-rail.tsx` (all four functions, L19/L85/L89/L158),
`components/thread/thread-screen.tsx` (refresh, L32/L258),
`components/shell/bot-create-form.tsx` (refresh, L11/L43),
`components/shell/acp-detected-row.tsx` (all four, L7/L21/L35/L44).

## Architecture findings (v0.3.0)

1. **Hidden process-global singleton.** `roster` and `listeners` are mutable module globals: the
   store cannot be instantiated per test, per space or per SSR request; tests must reset hidden
   state. The transport (`botsList`) is imported and called in-module, so cache policy and RPC are
   inseparable.
2. **No state model.** The snapshot is only `data | null`; "never fetched" and "fetch failed" are
   indistinguishable (both `null`), so no consumer can render a retry/error state — the store's
   type is too narrow for the behaviour the shell needs (see #0086 FM1).
3. **Unvalidated boundary.** `reply.bots` is trusted verbatim (L39). Unlike `rpc.ts`, which
   normalizes messages/blocks/reactions (L288-391, L467-482), the roster reply is a bare cast: a
   malformed/partial reply poisons the store (`bots: undefined` → crash in a consumer `.filter()`).
4. **Domain rules leak into views.** `regular`/`snoozed` filtering (bot-rail L161-162), "first
   non-archived, non-snoozed bot" (acp-detected-row L35) and snooze lookup by thread (thread-screen
   L125) re-derive REQ-009 rules from raw `RosterBotView` in three separate places.
5. **No cache identity or lifecycle.** The store is implicitly scoped by RPC's module-global
   `knownSpaceId` (rpc.ts L191-211) but has no invalidation hook, no concurrency control and no
   cancellation; concurrent `refreshRoster()` callers race (last response wins) and nothing can
   react to a space/auth change.
6. **Duplicated store plumbing.** `lib/bot-directory.ts` (L17-49) reimplements the same
   listeners/snapshot/notify trio with different pre-load semantics (empty array vs `null`) — two
   hand-rolled stores, no shared primitive.

## Proposed upgrades (v0.4.0)

### U1. Extract a shared external-store primitive (findings 1, 6)
Add `lib/external-store.ts` with `createExternalStore<T>(initial, serverSnapshot)` returning
`getSnapshot`, `getServerSnapshot`, `subscribe` and `set` (notify only when the value changes by
`Object.is`, so an unchanged reply does not rerender every subscriber). Rebuild roster.ts and
bot-directory.ts on it; keep their public exports byte-for-byte compatible.
- Risk: two migrations widen the blast radius, and the stores disagree on the pre-load snapshot
  (`null` vs `EMPTY`). Mitigate: pass pre-load/server snapshots as arguments and keep thin re-export
  wrappers so no consumer import path changes; replay the shell goldens.

### U2. Inject the fetcher; keep a wired default singleton (finding 1)
`createRosterStore(fetchBots: (signal?: AbortSignal) => Promise<BotsListReply>)` plus a default
instance wired to `botsList`; `listRoster`/`refreshRoster` etc. become delegating wrappers. Tests
inject a fake fetcher with no network and no module-reset hacks.
- Risk: indirection in a 46-line module and a widened export surface. Mitigate: keep the existing
  named exports as the public API and leave all four call sites untouched.

### U3. Model the state and validate the reply (findings 2, 3)
Add an additive `getRosterState(): { status: "idle" | "loading" | "ready" | "error"; data:
readonly RosterBotView[]; error?: RpcError }`, with `data` always an array. Add
`normalizeRoster(reply)` that drops malformed entries, mirroring `normalizeMessage`. Keep
`listRoster()` returning `data.length ? data : null` until consumers migrate.
- Risk: changing `listRoster()`'s return type forces all four consumers plus `useSyncExternalStore`
  selectors to update, and a new visible error state is a DEVIATION. Mitigate: land additively
  first, keep last-good-data-on-error behaviour, and record any UI change in `docs/PARITY.md`.

### U4. Centralize REQ-009 projections as pure selectors (finding 4)
Export `selectRegular`, `selectSnoozed`, `findByThreadId`, `findByBotId` and `firstNavigableBot` from
roster.ts; have bot-rail, thread-screen and acp-detected-row consume them so one place owns
archived/snoozed semantics, testable without React.
- Risk: cross-module coordination (#0086 bot-rail, thread-screen workers) and any ordering change
  alters row order/shelf contents in goldens. Mitigate: keep selectors pure with ordering identical
  to today's `.filter()` calls; table-driven unit tests plus golden replay.

### U5. Cache identity, de-dup and cancellation (finding 5)
Hold one in-flight promise and an `AbortController` in the store: `refreshRoster()` returns the
in-flight promise when one exists and cancels it on a superseding call; stamp results with
`rpc.getSpaceId()` and expose `invalidateRoster()` for callers to invoke on a space switch. Keep
last-good data on error (no behaviour change).
- Risk: coupling roster to RPC space state; rpc.ts has no space-change subscription today, so a
  full invalidation hook is broader than this module. Mitigate: ship de-dup + abort inside the store
  first (no new RPC API), leave invalidation as an explicit caller action, verify no timing
  regression with fake-timer Playwright tests.

## Execution order & gate

U2 → U1 (structural, no UI change) → U3 (state model, gated on consumer migration + DEVIATION
check) → U4 (selectors, coordinated with #0086) → U5 (lifecycle) last. Each step: unit suites green,
shell/thread Playwright green, axe 2.2 AA clean, UI goldens green, JS bundle budget (docs/PLAN.md)
unchanged.

## What not to touch

The `botsList` wire contract and `RosterBotView` field names (REQ-009); the `useSyncExternalStore`
pattern itself; last-good-data-on-error semantics; BoardUI tokens/types; rpc.ts session/space header
rules.

CONFIRMED by reading `roster.ts`, `bot-directory.ts`, `lib/api/rpc.ts` (L90-103, L191-211,
L288-391, L422-425), the four consumers and sample #0086. GUESS: that a shared store primitive will
not change golden-visible timing, and that a space-change hook can be added without touching
rpc.ts — verify with /ui-verify and golden replay before implementation.
