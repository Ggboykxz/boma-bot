# 0090 — apps/web/components/shell/not-found-screen.tsx — reliability (v0.4.0)

Worker #090 · module `apps/web/components/shell/not-found-screen.tsx` · lens: reliability · READ-ONLY.
Proposal only; no code in this change. Line refs against `before-m5`.

## What the file is and how it is reached

21-line `"use client"` screen: one `h1`, one `p`, one `ButtonLink href="/"`; all copy via
`useTranslation()` (lines 8, 12-17). It is reached two ways:

1. **HTTP 404 (today).** `app/not-found.tsx:1-5` wraps it; `output: "export"` (`next.config.ts:8`)
   emits `out/404.html`, `out/404/index.html` and `out/_not-found/index.html` (CONFIRMED: all three
   present in `apps/web/out/`). The Rust fallback reads **only** `root/404.html` and returns it with
   HTTP 404 for GET/HEAD whose `Accept` contains `text/html` or `*/*` and that resolve to no export
   file (`crates/api/src/router.rs:260-309`; REQ-055 tests `http_surface.rs:1065-1082, 1158-1163`).
2. **Client dispatch (planned).** Wave-5 #0079 U4 wires `{kind:"not-found"}` from `AppEntry` to this
   screen for unknown `/app/*` shapes; wave-2 #0025 U2 agrees. Today those shapes render
   `ThreadsHome` instead, so the screen is never reached from the shell.

Spec §8.1 lists `*` → Not found, public (CONFIRMED).

## Failure modes

### F1. Renders instead of home (false 404 / wrong landmark)

- The Rust prefix fallback is hard-coded to three golden filenames (`app/artifacts/artifact_golden/
  index.html`, `p/token_unknown/index.html`, `app/index.html` — `router.rs:206-213`). A valid deep
  link whose export filename changes, or a new screen family added without a prefix entry, stops
  resolving and falls through to `404.html`: a legitimate URL gets this screen. This is a
  deployment-coupled false 404, not a React bug.
- The classifier planned in #0079 U1 can misclassify a valid home/thread shape as `not-found`
  (regex edge cases: trailing slash, `%zz`, encoded `/`, repeated slashes). Then this screen renders
  where home should be. Today the opposite bug holds (`null` → home for junk paths).
- Placement: #0079 U4 already flags that this screen emits its own `<main>`; rendered as an
  `AppShell` body it creates two `main` landmarks (axe + SR regression). Rendered top-level it loses
  the shell's rail/nav, leaving `href="/"` as the only escape.
- `useTranslation()` reads `useLocale()`; `AppProviders` hydrates `en` then switches to document
  `lang` (`app-providers.tsx:11-50`). On the static 404 that is English copy for the first frame in
  `he`/`fr`, and English permanently if the bundle fails to load.

### F2. Broken navigation back

- `ButtonLink` is a plain `<a>` (`button.tsx:181-213`): no Next `<Link>`, no prefetch, full document
  reload. In the static export this discards in-memory state (open thread, draft, scroll) and
  re-runs the boot script.
- `href="/"` is not "back". `/` is `LandingScreen`, which probes `botsList` and `router.replace`s to
  `/app/` when signed in or `/sign-in` otherwise (`app/page.tsx:3-8`, #0025 U3). "Back to the start"
  therefore bounces to a different screen depending on auth state; a signed-in visitor never sees the
  root screen. Spec-consistent (§8.1 says `/` redirects when signed in) but surprising given the label.
- Drops `?next=`/hash/deep-link context. `href="/"` is absolute so resolution is stable with
  `trailingSlash: true`; a future base path would break it (GUESS: none configured today).
- No focus management: after client dispatch, focus stays on the triggering element or `<body>`;
  neither `<main>` nor `h1` is focusable, so keyboard/SR users are not told they landed on a 404.
  (GUESS until axe/Playwright; #0025 U4 proposes `tabIndex={-1}` for the home heading only.)

### F3. Static-export 404 brittleness

- The Rust fallback couples to one generated filename. Next also emits `404/index.html` and
  `_not-found/index.html`, both ignored. A Next upgrade or build-config change that drops/renames
  `404.html` silently degrades the 404 to the JSON fallback (`router.rs:297-310`) — no alert, just
  wrong content type and shape.
- `accepts_html` treats `*/*` as HTML (`router.rs:264-268`), so a client sending `Accept: */*` gets an
  HTML 404 body and status even if it wanted JSON. This contradicts the REQ-055 comment that "any
  accept that is not HTML keeps the JSON `404`". Non-browser clients using `*/*` are affected.
- `resolve_static` allows any dotted filename (only `..`, `.`, `\` refused — `router.rs:175-181`):
  `/404.html`, `/404/`, `/_not-found/` are served with **200** as ordinary export files, while only
  unmatched paths get the 404 status. The same screen is reachable as both 200 and 404 — an SEO and
  observability inconsistency. `x-robots-tag: noindex` is set for shared pages only
  (`public.rs:235`), not here.

## Concrete hardening for v0.4.0

- **H1. Single owner of "not-found" placement.** Decide once: keep `NotFoundScreen` top-level (as
  `app/not-found.tsx` does) and do not render it inside `AppShell`'s `main`; for unknown `/app/*`
  shapes use a quiet shell-body error state or a top-level 404 document. Risk: landmark/golden
  drift; mitigate with a single-`main` axe assertion and golden replay (#0079 U4).
- **H2. Honest back contract.** Keep `href="/"` (spec §8.1 makes `/` canonical) but relabel toward
  "go to the start", or add a session-gated secondary action "Back to threads" → `/app/`. GUESS:
  session detection in this static client; do not invent a wire call. Do **not** switch to
  `router.back()` — history may be empty on a deep link. Risk: new control changes golden counts and
  may be a DEVIATION; owner sign-off before shipping.
- **H3. Focus on arrival.** Give `<main>` or `h1` `tabIndex={-1}` and focus it on mount; keep one
  `h1`/`main`; verify 360 px, light/dark, RTL (boot-script `dir`), reduced-motion (unused here).
  Risk: focus effect on a top-level 404 may fight the boot script; gate on Playwright + axe.
- **H4. Pin the export contract.** CI check that `out/404.html` exists and carries a stable marker
  (e.g. `data-screen="not-found"`, not copy text). Extend REQ-055: unknown HTML path → 404 +
  `text/html` body carrying the marker; `/404.html` and `/_not-found/` → 200; `/api/nope` with
  `Accept: */*` → JSON 404 (decide and document `*/*` semantics before touching `router.rs`).
  Risk: adding a marker touches this file's JSX — coordinate with H3 in one PR.
- **H5. Classifier guard (cross-module).** Table-driven test that every valid §8.1 shape classifies
  as home/thread/group and only truly unknown shapes as `not-found`, including trailing slash,
  `%zz`, encoded `/`, `//`. Risk: classifier lives in `AppEntry`, not here — dependency on #0079 U1.

## What not to touch

`notfound.*` keys/copy (`lib/i18n.ts` EN/FR/HE all present), BoardUI semantic tokens and composite
type utilities, the `ButtonLink` import, ADR-013 prefix fallbacks without a pinned test, spec §8.1
route table.

## Claims

CONFIRMED by reading source this session: this file, `app/not-found.tsx`, `next.config.ts`, the
`apps/web/out/` listing, `router.rs:161-310`, REQ-055 tests, `button.tsx`, `app-providers.tsx`,
`layout.tsx`, `i18n.ts`, spec §8.1, briefs #0025/#0079. GUESS until replayed: golden landmark
assertions, `*/*` client impact, build-output change risk, JS-disabled locale fallback, focus
behavior. No tests run (READ-ONLY).
