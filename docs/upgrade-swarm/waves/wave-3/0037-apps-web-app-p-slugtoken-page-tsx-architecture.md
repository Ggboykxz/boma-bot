# 0037 — apps/web/app/p/[slugToken]/page.tsx — architecture lens — v0.4.0 proposal

Module under review: `apps/web/app/p/[slugToken]/page.tsx` (21 lines): `generateStaticParams()`
pins `token_golden` / `token_unknown`; the default export renders `<SharedPageScreen />` with no
props. All behavior lives in `apps/web/components/shared-page/shared-page-screen.tsx` and
`apps/web/lib/loc.ts`; this proposal covers the module as the entry to that flow. READ-ONLY
at `before-m5`. Anchors: REQ-037, §8.8, ADR-013, spec/golden/ui/REQ-037-shared-page{,-invalid}.json.
Reliability items are covered by 0038; this file does not repeat them.

## Current structure (CONFIRMED by reading)

- The route is a static-export shell (ADR-013): one pre-rendered `p/token_unknown/index.html`
  serves every `/p/*` deep link; `generateStaticParams` exists only so the dev server and the
  two UI goldens have a file to open. The token is never known at build time.
- Client-side flow: `useSyncExternalStore(locSubscribe, () => pathToken(), () => null)` → SSR
  paint is `null` → `loading` skeleton; then one `fetch` of
  `/api/v1/pages/shared/{encodeURIComponent(token)}` with `credentials: "omit"` and an
  `AbortController`; non-OK/network/JSON failure all collapse to the same "Page not available"
  document (§8.8 uniform failure, by design).
- The renderer is defensive in depth: `safeLink` (https?/mailto:), `safeImage`
  (https:/base64 data:image/*), caps on columns/rows/points/text, `whitespace-pre-wrap`, no
  `dangerouslySetInnerHTML` anywhere. The page is data, never markup (REQ-039 spirit).
- Access model: the URL token is the capability. No session, no cookie, no CSRF surface, no
  navigation chrome, `credentials: "omit"`. Backend evidences (CONFIRMED): uniform 404 body
  for unknown/revoked/malformed (`crates/api/src/public.rs:162-217`), `cache-control: no-store`,
  `x-robots-tag: noindex, nofollow`, `referrer-policy: no-referrer`, 120/min/IP per D-01.
- Two render paths exist for the same resource (CONFIRMED): the static shell above, and a
  backend server-rendered HTML document at the same API route when `Accept: text/html`
  (`crates/api/src/router.rs` route, `public.rs` `accept_prefers_html` branch,
  `pages::render_shared_page_html` with CSP `SHARED_PAGE_CSP` = `default-src 'none'; sandbox
  allow-scripts; …`). The web client sends `accept: application/json`, so the second path is
  dead code from this app's point of view — and it styles with raw hex, not tokens.
- Static-serving gap (CONFIRMED): `web_fallback` serves the export shell with `nosniff`,
  global `no-referrer`, explicit `cache-control: no-cache` — but **no `x-robots-tag` and no
  CSP**. The `noindex` promise of §8.8/REQ-037 currently holds only on the API responses, not
  on the document the browser actually loads.
- Spec drift (CONFIRMED): §8.8 allows "no product chrome beyond a small footer"; the screen
  ships no footer at all. Goldens expect `navigation`/`button` absent — a footer link is legal.

## Access model assessment

Token-as-capability is the right shape for "readable by anyone with the link, signed out, and
revoked": revocation is a server-side token delete, and uniformity is preserved by collapsing
every failure into one document. The weak seam is the *transport shell*: the static document
is the one response a crawler or intermediary sees first, and it is the one missing the
hardening headers. Everything else (enumeration resistance, rate limiting on the API, timing
equality) lives server-side and is out of this module's control.

## Concrete upgrades (v0.4.0)

1. **Fix the header contract on the static path, not the component.** Give the Rust static
   handler (or `generateStaticParams` documents via a `public/_headers`-style mechanism if the
   export gains one) an `x-robots-tag: noindex, nofollow` and a shared-page CSP
   (`default-src 'self'; img-src data: https:; connect-src 'self'` — the shell needs exactly
   one API origin and inline Next bootstrap) for `/p/*` responses; keep `no-store`.
   Alternative: move `/p/<token>` fully server-side to the backend HTML renderer and delete the
   client fetch — one render path, one CSP, no JS required. Risk: full SSR is a bigger
   behavioral change (document.title, hydration, goldens) and the backend renderer is
   untokenized; the header fix is additive and reversible. Decide in an ADR; either way,
   one of the two duplicate render paths should die in v0.4.0.
2. **Token-in-`<head>` metadata hygiene.** Add a static `<meta name="robots" content="noindex,
   nofollow">` / `<meta name="referrer" content="no-referrer">` to the `/p/*` shell via the
   root metadata/export config, so the promise holds even behind misconfigured proxies that
   strip headers. Risk: duplicated source of truth vs. headers — keep both, they assert the
   same fact; verify with the Playwright REQ-037 spec.
3. **Small footer, token-styled.** Add the §8.8 footer: one line, e.g. "Shared with Boma Bot",
   `text-caption-1-semibold text-text-tertiary`, optional `accent-600` link to the product
   home, `border-separator-border` top hairline, no button/nav roles (goldens stay green).
   Risk: goldens count `button`/`navigation` roles only — safe; wording is user-visible, so
   run it through the i18n layer, not a literal.
4. **Server shell / client island split.** Make `page.tsx` a server component exporting
   `metadata` (`robots: { index: false }`, `referrer: "no-referrer"` — Next emits the meta tags
   automatically) and rendering `<SharedPageScreen />` inside `<Suspense>` with the existing
   token-matched skeleton as fallback (announced `role="status"`, not silent). Risk: static
   export + client `useSyncExternalStore` means the skeleton must remain CSS/token-only or CLS
   regresses; meta robots in a static export only help if the handler doesn't override it.
5. **Contract-pin the JSON payload.** Validate the `response.json()` body against a minimal
   runtime guard (title: string, blocks: known-tagged union, per-block caps) before `setPage`,
   collapsing malformed payloads into the same `unavailable` phase — today a 200-with-garbage
   throws inside render with no error boundary (white screen). Share the guard with the
   backend's `PageBlock` mirror or generate it from `spec/openapi.yaml`. Risk: schema drift
   between three copies (backend validator, TS type, guard) — generate from the OpenAPI doc or
   accept a hand-maintained mirror with a contract test; over-strict validation breaks pages
   the backend considers valid.

## Notes / non-goals

- Keep `credentials: "omit"`, keep one shell for `/p/*`, keep the uniform-failure document
  byte-identical for unknown/revoked/malformed — §8.8 is the requirement, not wording.
- No `<img>` referrer fix, fetch timeout, token-alphabet pre-check, title reset, or retry
  affordance here — see 0038 (reliability lens); do not double-count.
- If the ADR in item 1 picks full SSR, items 2/4 collapse into it and the client screen is
  retired behind the same golden tests — do not ship both paths "temporarily".
- All UI stays on BoardUI semantic tokens (`bg-background-full`, `text-text-*`, composite
  `text-body-*`/`text-title-*` utilities, `border-separator-border`, `accent-*` for the chart
  and links); no raw hex, no `dark:` forks; icons from `@remixicon/react` if the footer gains one.

## Suggested order

1 (headers, blocks the §8.8 promise) → 2+4 (one PR: metadata + shell split) → 5 (payload
guard) → 3 (footer, needs i18n sign-off). Each step lands with the existing REQ-037 Playwright
spec extended (header assertions, malformed-payload fixture), never a new harness.
