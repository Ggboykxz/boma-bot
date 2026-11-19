# 0088 · apps/web/components/shell/landing-screen.tsx · LENS=reliability · v0.4.0 proposal

Worker #088 · module `apps/web/components/shell/landing-screen.tsx` · lens: reliability · READ-ONLY.
Proposals only; no product code edited. Line refs against `before-m5`. Complements the architecture
brief for this module; no structural duplication.

## What the file does (CONFIRMED — read at `before-m5`)

57-line `"use client"` screen mounted only at `/` (`app/page.tsx`). On mount it fires one
`botsList(controller.signal)` probe; success → `router.replace("/app/")`, **any** rejection →
`setProbeFailed(true)` + `router.replace("/sign-in")`. The static landmark (logo + `landing.*`
copy + one `ButtonLink`) renders underneath. The CTA's href flips between `/app/` and `/sign-in`
based on `probeFailed` (line 51). `call()` (`lib/api/rpc.ts:245-272`) carries the session cookie,
invokes the global 401 handler on status 401 (`:260-262`), and wraps failures in `RpcError` with
`code`/`status` — the screen reads neither. `next.config.ts` pins `output: "export"`,
`trailingSlash: true`, `reactStrictMode: true`; the Rust static handler maps directory URLs to
`index.html` (`crates/api/src/router.rs:173-198`). `out/icon.svg` exists, so `<img src="/icon.svg">`
(line 46) resolves — the static-image comment is accurate (CONFIRMED from `apps/web/out/`).

## Failure modes

1. **"Any error means signed out" (root defect).** The `.catch` at `:31-37` cannot distinguish
   401 from 429, 5xx, network/offline (`TypeError`), a mis-set `NEXT_PUBLIC_BOMA_API_BASE`, a
   CORS failure, or `response.json()` throwing on a non-JSON 200 body. A signed-in member with a
   transient backend fault is evicted to `/sign-in` and loses their place; an offline visitor is
   shipped to a sign-in page whose own `authCapabilities` probe also fails. There is no timeout on
   the fetch (the controller is only an unmount signal), so a black-holed connection leaves the
   landing page showing a CTA aimed at `/app/` forever. The error is silent — no `role="alert"`,
   no message, no retry. (CONFIRMED from `landing-screen.tsx:23-41`, `rpc.ts:245-272`.)

2. **Double navigation on 401 (redirect race).** On a 401, `call()` first invokes the global
   `UnauthorizedRedirect` handler, which builds `/sign-in?next=<current path>`; its guard excludes
   only `/sign*`, `/forgot*`, `/reset*` (`app-providers.tsx:67-82`) — `/` is **not** excluded. The
   landing screen's `.catch` then issues a second, competing `router.replace("/sign-in")`
   without `next`. Two same-tick client navigations race; whichever wins, one intent is lost
   (the `next=` return path) and React may abort one transition. If the local one wins, sign-in
   defaults to `/app/` — a signed-in-but-401 visitor lands back in the protected subtree and can
   bounce `/` → `/sign-in` → `/app/` → 401 → `/sign-in?next=/app/…` (the "already-signed-in loop").
   (Race CONFIRMED; the full loop depends on cookie/edge propagation timing — GUESS.)

3. **The CTA is a guess, not an answer.** Before the probe settles, `probeFailed === false` so the
   "Get started" link points at protected `/app/` (`:51`). A signed-out visitor who clicks during
   that window takes the long route `/` → `/app/` → 401 → `/sign-in?next=/app/`. A link whose
   destination changes asynchronously under the user is a reliability and a11y smell, and there
   is no "checking your session…" state. (CONFIRMED.)

4. **Static-export / no-JS gap.** Spec §8.1 promises `/` "redirects to /app when signed in,
   otherwise the sign-in screen," but the redirect is client-only: with JS disabled or a blocked
   chunk, the static landmark is all the visitor gets and no redirect ever occurs. `router.replace`
   in an export is a client RSC-payload fetch; a stale/missing payload degrades to a hard load with
   no error boundary here. `probeFailed` is also terminal — a transient failure sets it once with
   no re-probe, stranding the visitor on sign-in. Relative vs canonical URLs are inconsistent
   (`/app/` with trailing slash, `/sign-in` without) though both currently resolve in the Rust
   handler — brittle if the fallback table changes. (CONFIRMED code/spec; hard-load behavior GUESS.)

5. **Unvalidated success and stale env.** A 200 with well-formed-but-wrong JSON (miswired RPC,
   intercepting proxy) still resolves `botsList`, so the screen sends the visitor to `/app/`
   without ever checking the reply shape. Likewise, an API-base override pointing at a reachable
   but wrong origin yields a "signed-in" verdict from an unrelated 200. Low frequency, real
   misconfiguration amplifier. (CONFIRMED contract is HTTP-200-only, per the file's own header.)

## Concrete hardening (v0.4.0)

1. **Classify the probe result; never equate transport failure with sign-out.** Replace the boolean
   with a discriminated state (`probing | signedIn | signedOut | error`). Branch on `RpcError.status`:
   401 → `signedOut` (go to sign-in); 429/5xx/network → `error` (retryable, stay on the landmark,
   show `role="alert"` + Retry); 200 with an invalid `bots` shape → `error`, not `signedIn`.
   - *Risk:* over-classification could keep an honestly signed-out user on the landing page when
     the backend returns 5xx; mitigate with a bounded retry (2 attempts, short backoff) then a
     clear "sign in" affordance. New strings → all locales (i18n gate).

2. **Own the 401 on this route instead of racing the global handler.** Either pass a per-call flag/
   local handler so `botsList` does not fire `UnauthorizedRedirect` for the `/` probe, or have the
   local catch encode the same `next=` value and let a single redirect win. Never leave two
   `router.replace` calls in one tick.
   - *Risk:* suppressing the global handler could drop the `next=` return path for other mounts;
     scope the change strictly to this probe and assert one navigation via Playwright.

3. **Bound the request and make the CTA honest.** Combine the unmount controller with
   `AbortSignal.timeout` (e.g. 8 s, tuned to p95) → `error` state, not an indefinite `/app/` link.
   While `probing`, render the CTA pointing at `/sign-in` (the safe destination) or disable it with
   a visible "checking…" label and `aria-busy`; only switch to `/app/` on confirmed `signedIn`.
   - *Risk:* a timeout could mislabel a slow-but-valid probe; keep it non-destructive (never bounces
     an authenticated user) and measure on a throttled link (criterion 4) before fixing the value.

4. **Make the no-JS/static path degrade safely.** Add a `<noscript>` link to `/sign-in` (and keep the
   landmark copy intact) so the static artifact still offers a valid next step. Normalize all
   destinations to the canonical trailing-slash form (`/app/`, `/sign-in/`) to match `trailingSlash:
   true` and the Rust fallback table.
   - *Risk:* a `<noscript>` link changes the golden's reachable-control count; update goldens
     deliberately and keep the visible CTA count unchanged for JS users.

5. **Test the loops, not just the happy path.** Playwright (`REQ-04x`, landing golden): signed-in →
   one replace to `/app/`; 401 → exactly one replace to `/sign-in` with intact `next`; 5xx/offline
   → `error` state + retry, no sign-in bounce; hung probe → timeout state; click-before-resolve →
   no protected-route detour; StrictMode dev double-mount → one effective probe; JS-disabled →
   `<noscript>` path present.
   - *Risk:* route-mock timing brittleness — reuse existing fixtures/fake timers; assert navigation
     counts, not absolute URLs alone.

## Explicitly deferred

- Server-side cookie-aware redirect at `/` — impossible in a static export (ADR-013); the probe
  remains an enhancement. File a DEVIATION note against the wording of spec §8.1 if strict parity
  is demanded. Backend session/cookie hardening (rotation, edge propagation) is out of scope.
- Sign-in screen's own error taxonomy and rate-limit handling (brief 0042), and the `/app` shell
  fallback (brief 0079/0026). Coordinate, do not duplicate.

## Gate

All five hardening items land with landing goldens green (JS + no-JS), axe AA clean (visible focus,
`role="alert"`, no color-only state), a Playwright assertion of a single navigation per probe
result, bundle budget (docs/PLAN.md) unchanged, and no new strings missing from any locale.

CONFIRMED by reading `landing-screen.tsx`, `app/page.tsx`, `rpc.ts`, `app-providers.tsx`,
`sign-in-screen.tsx`, `next.config.ts`, `crates/api/src/router.rs`, `spec/08-ui-ux.md §8.1`, and
`apps/web/out/` layout. GUESS: full signed-in bounce loop timing, RSC hard-load fallback, golden
control-count impact — verify before implementation.
