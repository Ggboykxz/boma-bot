# 0111 — apps/web/lib/loc.ts — architecture (v0.4.0)

- Target: `before-m5` (v0.3.0-rc5), module `apps/web/lib/loc.ts` (37 lines).
- Read-only review; no code edited. Sister files read for the consumer contract:
  `components/shared-page/shared-page-screen.tsx` (:5, :143, :217–220),
  `components/artifacts/artifact-viewer.tsx` (:25, :57–67, :80–106),
  `lib/i18n.ts` (:1–20, `Locale`/`Direction`, `SUPPORTED_LOCALES`),
  `AGENTS.md` (BoardUI RTL + i18n rules), `apps/web/AGENTS.md` (Next.js breaking-change warning).

## What this module owns today

Despite the "locale helper" label in the swarm ticket, `loc.ts` is a **location** helper: it reads one
opaque path segment from `window.location.pathname`. Locale/direction belong to `lib/i18n.ts`
(`Locale` union, `SUPPORTED_LOCALES`, `CATALOGUES`, English fallback) and must stay there — this
module has no string or direction concern today and should not absorb one.

The file exposes three surfaces:

- `match(prefix, path)` — the only real logic: escape the prefix, build `^<prefix>([^/]+)/?$`,
  execute once, `decodeURIComponent` the captured segment, return `null` on no match or bad escape
  (`:22–33`). Pure, testable, and already the seam both consumers use.
- `pathSegment(prefix)` — a convenience wrapper that reads `window` then delegates to `match`
  (`:12–16`). **No consumer imports it** (grep: only its own declaration). It is a dead export.
- `locSubscribe` — an alias of `subscribe = () => () => {}` (`:10`, `:19`), i.e. a no-op external
  store. Both consumers pass it to `useSyncExternalStore` with a `() => null` server snapshot
  (`shared-page-screen.tsx:143`, `artifact-viewer.tsx:67`).

The architecture is sound in one respect: a pure parser is separated from the environment read, so
SSR and tests can use `match` directly. The debt is that the "subscription" is fake, so the
`useSyncExternalStore` contract is only honoured by accident (the snapshot is re-read on unrelated
re-renders, never on navigation).

## Helper structure in v0.4.0

Keep a single module with a single job and a clear three-tier shape:

1. **Core (pure):** `match(prefix, path): PathSegment | null` — the only place that parses a path.
2. **Environment adapter (browser):** one `readSegment(prefix)` that guards `window`, plus a real
   `subscribe(onChange)` tied to the History API.
3. **React adapter (hook):** one `usePathSegment(prefix)` that owns the
   `useSyncExternalStore` wiring and the shared `serverSnapshot`, so no component inlines
   `() => null` or imports `locSubscribe` by hand.

`pathSegment` and `locSubscribe` become internal/removed; `match` stays exported for tests. The
boundary with `i18n.ts` stays explicit: if v0.4.0 adds locale-prefixed routes (`/ar/p/…`), parse the
locale in the i18n layer and pass the *remainder* path to this helper — never teach `match` about
`SUPPORTED_LOCALES`.

## Proposals (concrete upgrades, with risks)

1. **Make the subscription real, and give consumers a hook.** Replace the no-op `subscribe` with a
   listener set notified on `popstate` (and a `pushState`/`replaceState` shim or Next.js route event),
   and export `usePathSegment(prefix)` so `useSyncExternalStore` lives in one place. Risks:
   (a) a no-op was chosen deliberately because ADR-013 static export has one shell per family —
   confirm whether client-side `router.push` can keep the same component mounted; if it can, the
   current code really is stale, otherwise this is hygiene, not a bug; (b) monkey-patching History is
   global side-effect territory — prefer Next's `usePathname()` as the change signal if it reflects
   the real path on static export (GUESS, verify in the pinned Next docs before coding); (c) keep the
   SSR snapshot `null` so first paint stays the empty state and hydration stays deterministic.

2. **Collapse the three exports into the tiered API above.** Delete dead `pathSegment`, keep `match`
   as the pure core, and migrate both consumers to `usePathSegment("/p/")` /
   `usePathSegment("/app/artifacts/")`. Risks: (a) this is an internal refactor, not a public API —
   there is no external contract, but the two imports must change in the same commit; (b) removing
   `locSubscribe` breaks any unseen dynamic import (grep found none — re-run before deleting);
   (c) do not change `match`'s `(prefix, path)` order or `null` semantics while moving it, or the
   empty-state tests drift silently.

3. **Harden `match` without changing its contract.** Replace dynamic `new RegExp(escape(prefix))`
   with `path.startsWith(prefix)` + slice + explicit checks: exactly one non-empty segment, optional
   single trailing slash, reject embedded `/`, encode-only `%2F`, `.`/`..`, and control characters;
   decode once inside a `try`. Cap segment length (~512) to bound abuse. Return a branded
   `PathSegment` newtype so "already decoded" is a type-level fact. Risks: (a) narrowing the accepted
   set is externally visible if any real link uses an odd encoding — pin the current accepted set
   with a table-driven test and replay the shared-page/artifact goldens before tightening; (b) the
   escape helper may become dead — remove it in the same step; (c) branded types add a cast at the RPC
   boundary — keep the cast in one adapter, not in components.

4. **Centralise the SSR/empty-state contract.** Export one `NO_SEGMENT_SERVER_SNAPSHOT` (or have the
   hook own it) instead of two inline `() => null` closures; document in the module header that the
   first client render is the empty state and that both screens must render their unavailable/
   "pick one" state from `null`. Risks: (a) a shared *function* constant must stay stable by identity
   or `useSyncExternalStore` will re-subscribe — freeze it at module scope; (b) if a future consumer
   supplies the client snapshot as the server snapshot it will hydrate with a real token and mismatch
   — a lint/test assertion on the hook's third argument prevents that.

5. **Pin the parser with traceable tests and keep the i18n boundary clean.** Add `loc.test.ts`
   covering: both trailing-slash forms, malformed `%` sequences, `%2F`, double slashes, empty
   segment, non-matching prefix, and SSR (`window` absent); if a REQ id owns this behaviour, name the
   test with it (trace_check counts declarations, not comments). Add a comment/link to ADR-013 and a
   sentence stating that locale parsing lives in `lib/i18n.ts`. Risks: (a) a property/fuzz pass over
   `(prefix, path)` must first record current behaviour — otherwise a test can lock in a bug;
   (b) if locale-prefixed routes land later, a naive prefix match would treat `ar` as a token —
   keep the prefix argument explicit and locale-agnostic; (c) tests must not import `window` at
   module load in Node.

## Explicit non-goals

No locale/direction logic in this module (no `Locale`, no `t()`, no `dir`); no change to the public
URL shapes `/p/<token>` and `/app/artifacts/<id>` (ADR-013) or to the server RPC contracts; no new
dependency; no second router/state layer. This helper only reads an opaque path segment and reports
`null` honestly when it cannot.

## Evidence

CONFIRMED — read this session: `loc.ts` :1–37 (three exports, no-op `subscribe`, `RegExp` core,
`decodeURIComponent` guard); `shared-page-screen.tsx` :5, :143, :217–220; `artifact-viewer.tsx`
:25, :57–67, :80–106 (no other `match`/`pathSegment` importers found by grep); `i18n.ts` :1–20;
both AGENTS.md surfaces (RTL/i18n rules).

GUESS — whether `router.push` can re-render the same `SharedPageScreen`/`ArtifactViewer` instance
without remount (decides if proposal 1 fixes a real bug); whether pinned Next `usePathname()` returns
the full runtime path on this static export; whether any shipped link relies on the currently accepted
edge encodings (verify against `spec/golden/ui` and captures before proposal 3).
