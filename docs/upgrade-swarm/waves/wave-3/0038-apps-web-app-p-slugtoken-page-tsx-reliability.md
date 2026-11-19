# 0038 · apps/web/app/p/[slugToken]/page.tsx · LENS=reliability · v0.4.0 proposal

## Scope note

The module is a 20-line static-export shell: `generateStaticParams()` pins `token_golden`/`token_unknown`;
the default export renders `<SharedPageScreen />` with no props. All request-time behavior lives in
`components/shared-page/shared-page-screen.tsx` and `lib/loc.ts`. This reviews the module as entry to that
flow. Anchors: REQ-037, §8.8, api-contract (public read, 120/min/IP, `noindex`), ADR-013, D-01.

## Current behavior (as read)

- One static shell serves `/p/*`; token never known at build time (dev server needs the two seeded goldens).
- Token read from `window.location.pathname` via `useSyncExternalStore`; SSR/first paint = `null` → `loading` skeleton.
- One `fetch` per token, `credentials: "omit"`, AbortController on unmount. Non-OK (404/410/429/5xx) and any
  network/JSON failure collapse to the same "Page not available" document — by design (§8.8).
- `document.title` set post-fetch, never restored; body cast `as SharedPage` with no shape validation.
- `safeLink` allows `https?:`/`mailto:`; `safeImage` allows `https:`/base64 `data:image/*`. Plain `<img>`, no
  referrer policy, no in-document `noindex`; CSP lives (or doesn't) at the export host.

## Failure modes

1. **Expired/invalid/revoked token — uniform but undiagnosed.** All non-OK → `unavailable` (correct for
   §8.8), but 429/5xx/network are also reported as "this link may never have existed". A *valid* link during
   a backend blip reads as permanent loss; no retry, no Retry-After honor (telemetry may differ; document may not).
2. **Rate limiting (120/min/IP, D-01) collides with the shell.** Every `/p/*` visit fires one browser API
   call the CDN can't absorb. A mildly viral/scraped link trips per-IP limits for corporate NATs — whole offices
   see "not available" at once; in-memory counters make it per-replica and inconsistent; no backoff/Retry-After; refresh loops amplify.
3. **Enumeration.** No length/charset sanity check pre-fetch; `match()` decodes arbitrarily long segments; scanners
   iterate guesses at full speed, each a logged hit and limiter load. Uniform wording hides the body, but 429-vs-404
   deltas and timing may still leak validity; `token_golden`/`token_unknown` fixtures must never resolve as real pages in prod.
4. **Embed/referrer breakage.** Links carry `noopener noreferrer nofollow`, but `<img>` has no `referrerPolicy` —
   viewing a shared page leaks the full (capability) token URL to third-party image hosts via Referer, violating §8.8's
   `no-referrer` posture. `frame-ancestors 'none'` blocks third-party iframing; export-host CSP/noindex unverified in this module.
5. **Unvalidated payload → crash.** `blocks.map` throws on a 200-with-garbage body (caught → `unavailable`, but after
   `document.title` may be mutated, no log); a block missing required fields (e.g. `{type:"text"}`) throws in render —
   no error boundary → white screen.
6. **Stale `document.title`.** Set on success, never reset; a later transition to `unavailable` keeps the old page's
   title — tab-title leak across the SPA shell and a §8.8 tell.
7. **Loading forever.** getServerSnapshot returns `null`; hydration mismatch, blocked JS or a hung fetch leaves the
   `aria-busy` skeleton indefinitely — no `<noscript>`, no watchdog, no timeout.
8. **Abort/timeout fragility.** Aborts are swallowed correctly today, but the stale-closure guard is one refactor away
   from interleaving `setPhase`, and there is no fetch timeout at all.
9. **Chart SVG numeric edge.** Negative point values yield negative rect heights; all-negative series render full-height bars.

## Proposed hardening (v0.4.0)

- **A. Failure taxonomy, uniform document, non-uniform telemetry.** Keep the §8.8 document byte-identical
  for 404/410/403/unknown, but branch *telemetry and one opt-out UI affordance* on status: 429/5xx/network
  → same document + a discreet "Check connection and retry" link (reload) and a counter metric;
  404-class → silent. Honor `Retry-After` on 429 by disabling auto-retry; never auto-refresh. Risk:
  any visible divergence breaks §8.8 — keep the differing element out of the DOM for 404-class
  responses (two code paths, one shared component, snapshot-tested for identity).
- **B. Timeout + bounded retry.** Wrap fetch in a 10s `AbortSignal.timeout` composed with the unmount
  controller; on timeout/5xx/429 surface the same document with the retry link (A). No automatic
  retry storm — one manual retry per user gesture. Risk: timeouts on legitimately slow backends read
  as "unavailable"; 10s is generous vs the 120/min/IP budget.
- **C. Client-side token hygiene.** In `pathToken`/`match`, reject segments > 256 chars or outside
  `[A-Za-z0-9_-]` (after decode) before any fetch — render `unavailable` locally, no API hit. This
  blunts scanner sweeps hitting the limiter and never legit tokens (backend alphabet is the same).
  Risk: rejecting a future longer/wider token alphabet breaks real links — gate on the documented
  backend token charset and add a test that fails loudly if the golden fixtures change.
- **D. Referer and embed hygiene.** Add `referrerPolicy="no-referrer"` on `<html>`/meta (or the export
  shell) and on the `<img>`; add `<meta name="robots" content="noindex,nofollow">` and
  `<meta name="referrer" content="no-referrer">` in the `/p/*` document head; confirm CSP
  `frame-ancestors 'none'` is emitted by the export host for this route. Audit every outbound URL
  in blocks (already scheme-allow-listed). Risk: meta referrer is ignored by some old browsers; the
  header is the real fix — verify at the CDN/export layer, not only in markup.
- **E. Payload validation before render.** Validate `SharedPage` shape with a tiny runtime guard
  (array checks, per-block field presence, numeric coercion, cap re-slice) before `setPage`; on
  failure → `unavailable` + log event (no body content, only shape keys). Wrap `<Block>` in an error
  boundary that falls back to the `unavailable` document, not a white screen. Risk: duplicate
  validation drift vs backend validator — keep the guard permissive (drop bad blocks, keep good
  ones) rather than strict-reject, and log drops.
- **F. `document.title` lifecycle.** Set title only on successful render; on any transition to
  `unavailable`, reset to a constant "Page not available"; restore on unmount. Risk: none beyond a
  test churn; pin with a golden.
- **G. No-JS / dead-hydration fallback.** Add `<noscript>` inside the loading branch ("This page
  requires JavaScript to load its shared content.") and a client-side 15s watchdog that swaps the
  skeleton for the `unavailable` document if the fetch never settles. Risk: watchdog fires on slow
  networks — pair with A's retry link; keep copy identical to §8.8.
- **H. Embed resilience.** If product wants "open in Boma" footer CTA later, it must not add session
  fetch or cookies (credentials stay `omit`); keep `frame-ancestors 'none'` and document that third-
  party embedding of the shell is unsupported. Add a Playwright golden asserting the response for
  `token_unknown` and `token_golden` are the same document under JS disabled (or noscript) too.
  Risk: product pressure to allow embedding — treat as a spec change (§8.8/CSP), not a quiet flag.
- **I. Observability without leaking the token.** Emit one structured event per shared-page outcome
  (`shared_page.ready|unavailable|timeout|invalid_payload`) with a *hash* of the token, never the
  token itself (it is a capability). Alert on 429-rate and invalid-payload-rate; the token hash lets
  abuse sweeps be counted without creating a log of valid tokens. Risk: hash linkability across
  sessions — acceptable for abuse counting; document retention.
- **J. Rate-limit coordination (backend dependency).** Push for shared counters (D-01 fix) or at
  least a per-IP allowance sized for NAT'd offices; until then, client must not auto-retry (B) and
  CDN should serve the static shell for all `/p/*` without API fan-out beyond the single read. File
  against the backend module, not this one.
- **K. Chart negative-value fix.** Floor `max` reduce at 0 and clamp bar height to `[2, 100]` with
  sign-aware rendering (or drop negatives with a log). Risk: changes golden SVG — regenerate the
  REQ-037 fixture deliberately, don't hand-edit.
- **L. Goldens and tests.** Extend `apps/web/tests/golden.spec.ts` + fixtures: 429-with-Retry-After
  renders uniform document; 200-with-garbage-JSON → uniform document; block missing required field →
  dropped block, page renders; noscript path; title reset on navigation away. Keep REQ-037-shared-page(-invalid)
  green; add `REQ-037-shared-page-rate-limited.json` if the golden format allows.

## Risks / notes

- **Spec tension:** §8.8 uniform failure is non-negotiable; every branch above keeps one DOM shape.
  A/B/E must be snapshot-verified. If any hardening needs a user-visible change (retry link), get spec
  sign-off first and mirror it in `08-ui-ux.md` §8.8 and the goldens.
- **Token charset (C)** must match the backend generator exactly; coordinate before tightening or real
  links break — this is the highest-blast-radius item.
- **CSP/referrer (D)** are export-host concerns; this module can only set what markup allows. Verify
  at the CDN layer with a header probe in CI.
- **Rate limiting (J)** is the real enumeration/viral-link bottleneck; client-side work only softens it.
- **Fixture drift:** `token_golden`/`token_unknown` exist for the dev server; ensure production build
  never special-cases them (add a build-time grep assertion).
