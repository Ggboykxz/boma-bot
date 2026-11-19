# 0056 · apps/web/components/auth/auth-shell.tsx · LENS=reliability · v0.4.0 proposal

## Scope and evidence (CONFIRMED — read-only review at `before-m5`)

`auth-shell.tsx` is a 42-line, hook-bearing frame: `useTranslation()` → `useLocale()`
(`lib/use-translation.ts:14-16`), a `<Link href="/">` wordmark (`icon.svg` + `t("app.name")`,
lines 22-29), and a title/subtitle + card around `children` (lines 31-39). Grep this session found
**8 importers**: five auth screens (`sign-in`, `sign-up`, `forgot-password`, `reset-password`,
`invite/accept-screen`), plus `integrations/integrations-setup.tsx:8`,
`artifacts/artifacts-grid.tsx:14`, and no `error.tsx`/`global-error.tsx` anywhere under `app/`.
`t()` falls back to English then to the literal key (`lib/i18n.ts:1218-1226`). This proposal is
complementary to architecture-lens workers; it owns only reliability of the shared frame.

## Failure modes

1. **Dead imports blur the component's contract (CONFIRMED).** `integrations-setup.tsx:8` and
   `artifacts-grid.tsx:14` import `AuthShell` and never reference it again (grep returned only the
   import line in each). The header comment claims "the one frame for every public auth screen,"
   but two non-auth screens import it — a copy-paste artifact that (a) fails any
   `no-unused-vars`/`ts-prune` gate with `--max-warnings 0`, (b) drags the client-hook graph into
   non-auth chunks even if the bundler later tree-shakes it, and (c) teaches future readers that
   `AuthShell` is a generic page frame, inviting misuse on `/app` screens. Risk of simply deleting
   the lines: none found; both are genuinely unused (CONFIRMED this session).

2. **Missing client / error / fallback boundary (CONFIRMED for source; impact GUESS).**
   (a) The file has **no `"use client"` directive** yet calls a client-only hook chain
   (`useLocale`); it works today only because all 8 importers are `"use client"` modules. A future
   server-component consumer (e.g. a direct import in a `page.tsx`) turns this into a runtime
   hooks/`useLocale` error instead of a build-time failure. No boundary is declared in the module.
   (b) There is **no error boundary** under `app/`; if any child throws, the entire auth screen —
   including the wordmark and the only "back home" link — unmounts, leaving no recovery path.
   (c) Page shells use `<Suspense fallback={null}>` (`app/sign-in/page.tsx`,
   `app/reset-password/page.tsx`), so on a slow/hydration-delayed link the user sees white, not the
   frame. (d) `title`/`subtitle` are plain strings with no guarantee callers passed `t()`; a
   hardcoded label is invisible to every current test.

3. **Duplicated chrome across public screens (CONFIRMED).** The wordmark recipe —
   `<Link href="/">` + `boma-target boma-focus` + `text-headline-medium` + `t("app.name")`, often
   with `icon.svg` — is hand-copied into `admin/access-screen.tsx:146-148`,
   `admin/plugins-screen.tsx:86-88`, and `shell/landing-screen.tsx:46-47` in addition to the
   canonical copy at `auth-shell.tsx:22-29`. A change to mark size, alt text, RTL mirroring, link
   target, or focus ring propagates to at most one of four sites. Line 26's
   `@next/next/no-img-element` eslint-disable is a second fragile coupling: with
   `--report-unused-disable-directives`, a rule rename/exemption flips it into an error. Adjacent
   phase markup (loading skeletons, error icon rows) is also repeated per screen, but the frame
   itself is correctly centralized — the defect is the *unmanaged* wordmark copies.

## Concrete hardening (v0.4.0)

1. **Declare the boundary explicitly.** Add `"use client"` to `auth-shell.tsx` so it is safe from
   either module graph and the hook chain is honest at the file boundary. *Risk:* none for current
   importers (all client); a presentational no-hook variant would avoid the directive but adds
   wordmark props to every call site — reject that trade for now.

2. **Delete the two dead imports and gate them.** Remove lines 8/14 in `integrations-setup.tsx` and
   `artifacts-grid.tsx`; enable `no-unused-vars` (or add `ts-prune`) with `--max-warnings 0` in the
   web lint job. *Risk:* a future legitimate use would be flagged — review imports every wave, do
   not blanket-disable the rule.

3. **Extract one `AuthWordmark` and reuse it.** Move lines 22-29 into a single exported component
   (mark + link + focus/target classes + `t("app.name")`) and have AuthShell, both admin screens,
   and the landing screen consume it. Keep admin `nav` chrome local — those screens intentionally
   have navigation; only the mark+wordmark link is shared. *Risk:* admin screens are wider and may
   want a different mark size — expose one optional `size` prop, not raw class overrides. Verify
   light/dark, 360 px, and `dir=rtl` snapshots against current UI goldens.

4. **Add an auth-scoped error boundary that reuses the frame.** Add a client `error.tsx` (route group
   for public auth screens; root-only if grouping is deferred) whose fallback renders `AuthShell`
   with a translated `auth.error.title`, `role="alert"`, and the `reset()` retry control plus the
   home link. *Risk:* a root `error.tsx` also wraps `/app` screens — keep the fallback generic or
   scope it; `reset()` must not remount a child stuck in a failing fetch loop (coordinate with the
   sign-in error-classification work in 0042).

5. **Ship a frame-shaped Suspense fallback.** Export `AuthShellSkeleton` (mark + card skeleton) and
   replace `fallback={null}` in the auth page shells with it, reusing the loading markup already
   present in `sign-in-screen.tsx:91-97`. *Risk:* more markup on every route; keep `aria-hidden` on
   the skeleton and match the real card geometry to avoid CLS.

6. **Remove the eslint-disable fragility and enforce string hygiene.** Prefer `next/image` if static
   export allows it; otherwise keep the `img` plus its disable but pin lint with
   `--report-unused-disable-directives`. Add a test that no hardcoded user-visible text appears in
   the auth screens' JSX and that every `t()` key exists in all 14 catalogues. *Risk:* `next/image`
   may alter the static-export asset path — verify `/icon.svg` resolves before switching.

## Test coverage

Extend `apps/web/tests/auth.spec.ts`: (i) a route-mocked child throw on each auth route keeps the
wordmark/home link visible and `reset()` recovers; (ii) Suspense delay shows the skeleton, not
white; (iii) one wordmark link per screen and matching DOM at 360 px / LTR / RTL / dark; (iv) lint
gate fails on a reintroduced unused `AuthShell` import. Carry `REQ-04x` in test names. *Risk:*
route-mocked throw timing is brittle — reuse existing fixtures; axe scan the error fallback.

## Success criteria

- No dead `AuthShell` imports remain; lint gate proves it.
- A throwing child never destroys the frame or the home escape route.
- One canonical wordmark implementation, with admin/landing copies removed.
- Slow hydration shows the frame-shaped skeleton (no white flash / CLS).
- Every client boundary is explicit; a server-side import fails at build, not runtime.
