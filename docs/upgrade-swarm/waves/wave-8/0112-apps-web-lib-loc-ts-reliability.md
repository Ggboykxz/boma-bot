# 0112 — `lib/loc.ts` reliability proposal (v0.4.0)

Module: `apps/web/lib/loc.ts`
Lens: reliability
Spec anchor: ADR-013 (static export, one shell per screen family), REQ-036 (`/app/artifacts/<id>`),
REQ-037/shared-page token path, `spec/08-ui-ux.md` §8.8 (indistinguishable "not available" document).

Status of claims: "Current behavior" is CONFIRMED by reading `loc.ts`, its two consumers
(`shared-page-screen.tsx`, `artifact-viewer.tsx`) and `lib/i18n.ts` this session; proposals and risks
are GUESS (design inference, not implemented or tested).

## Scope note

The brief's "invalid locale / direction inference" examples are owned elsewhere, not here:
locale validation and `direction()` live in `lib/i18n.ts` (`isSupported`, `languageOf`, `direction`),
and the client/server snapshot split lives in `components/providers/app-providers.tsx` (see 0074).
`loc.ts` is a *path* reader. This proposal maps those failure categories onto what this module
actually does — malformed input, source/snapshot inference, and the null fallback — and explicitly
recommends that locale/direction logic NOT be absorbed into this helper.

## Current behavior (CONFIRMED)

- `match(prefix, path)` builds `^<escaped-prefix>([^/]+)/?$`, returns the decoded segment or `null`.
- `decodeURIComponent` is wrapped in `try/catch`: malformed percent-encoding fails closed to `null`.
- `escape()` neutralises all regex metacharacters in `prefix`; the regex is rebuilt on every call.
- `pathSegment(prefix)` is the only window-guarded variant (`typeof window === "undefined"` → `""`).
  It is exported but unused: both consumers call `match(..., window.location.pathname)` directly.
- `locSubscribe` is a no-op (`() => () => {}`); it never emits. Consumers pass it to
  `useSyncExternalStore` with a client snapshot reading `window.location.pathname` and a server
  snapshot `() => null`.

## Failure modes

1. **Invalid input (analog of "invalid locale").** A malformed segment (`%`, `%ZZ`, lone surrogate)
   decodes badly and is caught — good — but the failure is silent, so a bad deep link looks like a
   legitimate empty state with no diagnostic. A segment containing an encoded slash (`%2F`) or `..`
   decodes to a value with path-like characters; the token consumer re-encodes it on fetch (safe),
   but the artifact id is passed straight to `artifactsGet({ artifactId })` — whether the RPC layer
   re-encodes or parameterises it is unverified (GUESS). There is no length cap: an arbitrarily long
   segment reaches `decodeURIComponent` and the component state.

2. **Source / "direction" inference (which path is authoritative).** `locSubscribe` never fires, so
   `useSyncExternalStore` cannot re-read after client-side navigation. A `router.push` from
   `/p/tokenA` to `/p/tokenB`, or a back/forward `popstate`, leaves the old token/id rendered — a
   stale-data reliability bug, not a crash. The consumers also bypass the window guard: if a
   `getSnapshot` is ever invoked in a non-DOM test/renderer, `window.location.pathname` throws.
   Conversely, `pathSegment` is dead code, so the one SSR-safe reader is unused. Locale/direction
   inference is *not* performed here — adding it would duplicate `app-providers.tsx` (0074) and risk
   two disagreeing sources of truth.

3. **Fallback.** `null` is a reasonable fail-closed fallback (empty/unavailable state, §8.8), but it
   conflates three distinct cases — no match, malformed encoding, and (via the no-op subscriber) a
   stale-but-non-null read. Because the regex requires exactly one segment, `/p/a/b` and `/p//`
   correctly fall back to `null`; trailing `/` is accepted. The fallback is therefore correct in
   shape but unobservable in cause.

## Concrete hardening for v0.4.0 (GUESS)

1. **Make the subscriber real.** Replace the no-op with a listener set plus `popstate`/`hashchange`
   registration and an app-wide route-change event that the shell dispatches after navigation;
   cleanup removes listeners. *Risk:* adding listeners changes global behavior and needs a real
   navigation test (token A → B via link, then back) to prove re-render; patching `history.pushState`
   is riskier and should be a last resort.

2. **Route consumers through `pathSegment`.** Change both consumers' client snapshot to
   `pathSegment(prefix)` so the window guard is actually used. *Risk:* none behaviorally on the
   client; add a non-DOM unit test asserting `match`/`pathSegment` return `null` without `window`.

3. **Validate and bound the decoded segment.** Trim to a sane max length (e.g. 512) and reject
   decoded values containing `/`, `\`, or control characters before returning; keep fail-closed
   `null`. *Risk:* a legitimate id containing an encoded slash would now be rejected — confirm the
   id/token alphabet in the spec before shipping, or this becomes a DEVIATION.

4. **Add dev-only diagnostics on decode failure.** In the `catch`, emit a `console.warn` under a
   dev flag before returning `null`, so malformed links are distinguishable from "no match".
   *Risk:* extra console output can trip strict console-error assertions — emit only in dev and
   assert the warning in a dedicated test.

5. **Memoise the compiled regex.** Cache `RegExp` per prefix in a module `Map` instead of
   reconstructing each call. *Risk:* low; the pattern is linear (`[^/]+`, no catastrophic
   backtracking), so this is hygiene, not a ReDoS fix — do not claim a security gain.

6. **Keep locale/direction out.** Add a comment stating this helper returns an opaque path token
   only; validation of `locale` (`isSupported`) and `dir` (`direction()`) stays in `i18n.ts` and the
   provider. *Risk:* none; prevents future duplication and the two-source-of-truth bug of 0074.

## Constraints

- Preserve the fail-closed `null` contract and the trailing-slash behaviour; both are parity-visible.
- Keep `getServerSnapshot` (`() => null`) constant so hydration stays deterministic; never read
  `window` during render.
- Any externally visible change to which URLs resolve (validation cap, encoded-slash rejection) is a
  DEVIATION — record in `docs/PARITY.md` and await approval before shipping.
