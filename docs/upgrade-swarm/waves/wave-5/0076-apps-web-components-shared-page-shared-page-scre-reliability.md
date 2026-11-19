# 0076 — `apps/web/components/shared-page/shared-page-screen.tsx` — LENS=reliability — v0.4.0 proposal

Scope: the client screen mounted by `app/p/[slugToken]/page.tsx` (a 21-line static-export shell,
ADR-013). Contract: REQ-037, spec/08-ui-ux.md §8.8, goldens `REQ-037-shared-page{,-invalid}.json`.
Claims below: CONFIRMED = read in this tree; GUESS = needs a running-app check.

## Current behavior (read, not run)

- Token read via `useSyncExternalStore(locSubscribe, () => pathToken(), () => null)`; `locSubscribe`
  is a no-op subscribe (`lib/loc.ts`): it never notifies on navigation.
- One `fetch` with `accept: application/json`, `credentials: omit`, `AbortController` aborted on
  unmount/token change. Any `!response.ok` and any thrown error → `phase = "unavailable"`.
  Three phases only: `loading | unavailable | ready`; no client timeout, no retry.
- Backend (`crates/api/src/public.rs::shared_page`) hashes the token into a lookup; unknown,
  revoked and malformed tokens share one 404 body and headers (matching §8.8). Rate limit is
  120/min/IP → JSON **429**. A DB error is deliberately a **500**, with the comment "an outage
  must not masquerade as a missing page." JSON branch sets noindex/no-referrer/no-store; the
  HTML branch additionally carries `SHARED_PAGE_CSP` (`connect-src 'none'`, `img-src data: https:`).
- Backend JSON uses `title: source.get("title").cloned().unwrap_or(Value::Null)` and
  `blocks: ...unwrap_or_else(json!([]))` — a valid page with no title yields JSON `null`.
- `SharedPage` is a blind `as SharedPage` cast; `Block` calls `.slice/.trim/.reduce/.length`
  directly on every field. No `error.tsx` next to `/p/[slugToken]/page.tsx`.

## Failure modes

1. **Expired / revoked / unknown token (CONFIRMED).** 404 → the uniform "Page not available"
   document (one heading, no nav/form/button) — correct and must stay. But *429, 5xx, offline and
   hung requests land in that same document*, whose copy says "This link does not match a page that
   exists… revoked, or it may never have existed." This makes an outage or a rate limit claim the
   link never existed — the exact mislabel the backend comment forbids. No retry, no telemetry:
   transient failures are invisible and indistinguishable to operators.

2. **Fetch failure (CONFIRMED / GUESS timing).** No timeout: a connection accepted but never answered
   leaves the skeleton with `aria-busy="true"` spinning forever. `token === null` takes `return`
   before any fetch and leaves `phase="loading"` forever (e.g. `/p/` with no segment, or the screen
   mounted off-route) — an infinite skeleton. A cached HTML/proxy interstitial with status 200 makes
   `response.json()` throw → "link not found" (no content-type check). `document.title = body.title`
   is unvalidated (`null` → the literal title "null") and never restored on unmount. Because the
   subscribe is a no-op, a shell reused for a second token never re-runs the effect: stale page and
   stale title persist under the new URL (low today because ADR-013 full-loads each `/p/*`; GUESS
   that any client-side navigation would expose it). `phase/page` are not reset on token change.

3. **Embed / render breakage (CONFIRMED).** The blind cast means a malformed hand-written payload or
   a backend DTO change throws in render: non-array `blocks`/`columns`/`rows`/`points`, non-string
   `text`/`href`/`src`, `row[cellIndex]` on a null row. With no boundary, a throw blanks the screen
   into Next's default error UI (which carries chrome — a §8.8 break) instead of the uniform document.
   Chart: `Number` values are unchecked, so `NaN`/`Infinity` yield `height = NaN/Infinity` and broken
   SVG geometry; `points.reduce` on a non-array throws. Image: `safeImage` validates base64 charset
   only, not validity, and has no `onError` — a bad `data:` payload or a 404 `https:` image shows a
   broken icon plus alt with no fallback; no intrinsic dimensions → CLS; no `referrerPolicy` on the
   `<img>` despite REQ-037's `no-referrer` (the document header covers the top level, not subresources).
   `safeLink`/`safeImage` remain sound as a second wall — keep them.

## Proposed hardening for v0.4.0 (concrete, with risks)

1. **Runtime guard, same fallback.** Add `parseSharedPage(raw): SharedPage | null` (hand-written type
   guard, no new dependency): require string `title`, array `blocks`, and correct field types/caps;
   coerce non-finite chart values to 0, drop NaN/Infinity labels safely, cap `data:` image length.
   On failure → `unavailable`. *Risk:* over-strict checks could reject a valid golden; validate
   against both REQ-037 goldens and the backend's `title: null` default (treat null title as "").
2. **Classify internally, render identically.** Distinguish `invalid` (404/410), `transient`
   (429/5xx/offline/timeout) and `malformed` (bad JSON/shape) in a state variable, but render the
   *same* `UnavailableScreen` for all three — preserve the golden's DOM byte-for-byte and never
   split unknown vs revoked (enumeration oracle). Only `transient` gets one silent retry with a short
   backoff (respect `Retry-After` if ever sent) plus a client telemetry breadcrumb. *Risk:* a retry
   button would add `role=button` to the unavailable document and could read as a validity oracle —
   prefer silent retry with no extra UI. Cap retries to one so 429 is not amplified.
3. **Client timeout + cleanup.** `AbortSignal.any([controller.signal, AbortSignal.timeout(10_000)])`
   (fallback: `setTimeout(() => controller.abort(), 10_000)`, cleared in cleanup, with a
   timeout-vs-unmount flag so unmount never sets state). Add a content-type check before `.json()`.
   *Risk:* `AbortSignal.timeout`/`any` availability on target browsers — feature-detect and fall back.
4. **Kill the infinite skeleton.** On mount with `token === null`, set `unavailable` (same document)
   and emit telemetry; gate on a `mounted` flag so the SSR `getServerSnapshot = null` never flashes
   it. Set `phase="loading"` and `page=null` on token change to avoid stale content; save/restore
   `document.title` in an effect cleanup. *Risk:* a slow nav could flash the skeleton — acceptable.
5. **Local error boundary.** Extract `UnavailableScreen` and use it both for phase and in a boundary
   wrapping `<SharedPageScreen>` (or `app/p/[slugToken]/error.tsx`) so render throws degrade to the
   uniform document, not Next's chrome-bearing default. Do not use the default error page.
   *Risk:* boundaries do not catch effect/handler throws — keep the fetch `try/catch`; log in dev so
   the guard in (1) is not masked.
6. **Image hygiene.** Per-image `onError` → plain alt text (never re-render another `<img>`), add
   `decoding="async"`, intrinsic `width`/`height` or `aspect-ratio`, `referrerPolicy="no-referrer"`.
   *Risk:* verify the static shell's CSP allows `connect-src 'self'` and `img-src data: https:`; the
   backend `SHARED_PAGE_CSP` has `connect-src 'none'` and must NOT be applied to the JS shell or
   every valid page breaks (CONFIRMED in source, GUESS which headers the shell actually receives).
7. **Tests (Playwright, names carry REQ-037).** Mock the endpoint: (a) 404 and 410 → byte-identical
   document (main + 1 heading, no nav/form/button); (b) 429/500/offline/hung → uniform document,
   no "does not exist" copy for transient, one retry only; (c) 200 with `blocks: null`, `title: null`,
   NaN values, broken `data:` image → no blank screen; (d) forced render throw → boundary fallback;
   (e) `/p/` with no token → no infinite skeleton; (f) CSP header check on the shell.
   *Risk:* route-mock brittleness — reuse the existing fixture style.

## Risks & rollout

Highest-value, client-only: (1), (3), (4), (5), (6) — ship and verify first. (2) must not alter the
unavailable document's shape or wording (golden + §8.8). A backend typed error code would make (2)
cleaner, but changing the JSON response shape is a DEVIATION — classify by status only for v0.4.0.
Any user-visible copy change needs spec-owner sign-off; flag in docs/PARITY.md and docs/STATE.md.
