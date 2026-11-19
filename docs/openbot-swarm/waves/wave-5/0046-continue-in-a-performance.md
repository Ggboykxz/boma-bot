# Parity proposal — continue-in-a, performance lens

Worker 046 · brief `id: 46` (`area: continue-in-a`, `lens: performance`) · target milestone M32
(mobile shell, REQ-056; hosted from M10 web). Research only: upstream clone read as data at
`/tmp/opencode/openbot-parity` (HEAD `559d5ac`); no upstream code, asset, wording or brand is
reproduced here. Only this document is written.

## 1. The capability, in our own words

A person opening the product on a phone is offered the device-appropriate path first, with an
explicit, remembered escape to stay in the browser: a coarse-pointer, narrow-screen query decides
whether to show a "this works best in the app — or continue here" interstitial; one tap continues
and the choice is remembered on the device; a valid deep link (invitation, sign-in return, billing
return, plugin/marketplace target, company-SSO return) always skips the interstitial, because those
flows cannot dead-end at a store page; the heavy interactive application is fetched only after the
escape is chosen. The sibling pattern in the same clone is the company/SSO sign-in handoff
("continue in your browser"), i.e. start here, finish there, never silently.

Provenance and honesty notes: the three-word area slug `continue-in-a` does not match any
user-facing string in the pinned clone (CONFIRMED by search); the clone carries only design and
behaviour, and website help articles are not in it. The decode below is anchored on the one in-repo
phone/browser handoff that exists (CONFIRMED, `WebPhoneAppScreen` + the pre-workspace skip logic in
`WebApp.tsx`: a coarse-pointer/small-viewport query, a locally persisted "stayed in browser" flag,
and deep-link addresses bypassing the screen entirely). The mechanism is CONFIRMED; the exact
source string is UNVERIFIED — recorded as such per `AGENTS.md`, simplest safe scope picked.

For this lens the interstitial is a *bytes-and-layout problem*, not a product problem: it must
render before any application JS runs, on a cheap phone over a weak link, and must never become the
reason the initial bundle grows.

## 2. Parity changes for low-end hardware and poor networks

Grounded in `docs/PLAN.md` §4/§9 budgets (read p95 ≤ 150 ms @ 2 vCPU, SSE first frame ≤ 300 ms,
stream frame p95 ≤ 150 ms, cold start ≤ 2 s, RSS idle ≤ 150 MB, JS initial ≤ 200 KB gzip as a
build-report gate — **currently unmet, the export is ~369 KB gzip across 35 chunks, measured** —
LCP ≤ 2.5 s / INP ≤ 200 ms / CLS ≤ 0.1 on throttled 4G) and `spec/04-nonfunctional.md` §4.8
(degrade honestly; never cache an authenticated response to fake one):

1. **Zero added JS. Extend the inline boot script, never the bundle.** `app/layout.tsx` already
   ships a parser-blocking `BOOT_SCRIPT` that applies theme/locale pre-paint. The handoff decision
   (one `matchMedia` for the coarse-pointer small-screen query + one localStorage flag read + a
   closed deep-link key check on `location.search`) lives in the same IIFE. No React branch, no
   `"use client"`, no chunk, no dependency — the interstitial is static HTML in the existing export
   (ADR-013), visible before the app chunk is requested.
2. **Deep-link bypass stays closed-set.** Only the existing token-bearing routes skip the screen
   (`token`, invite/plugin/billing/SSO-return keys), parsed with a length-capped allow-list, never
   a pattern over raw input; the address is already consumed and stripped by the shell, so no new
   param surface is created. A link always wins over the interstitial — that is the whole point of
   a handoff that must not trap a sign-in on a weak connection.
3. **Choice remembered locally, never on the wire.** The "continue here" tap writes one
   localStorage key (the `boma-theme` pattern); no cookie (zero bytes on every request over a
   metered link), no RPC, no account write, no cross-device state. Storage failure (private mode)
   degrades to asking again — honest per §4.8, never a fake remembered grant. Flags are device-local
   and per-origin, like the upstream behaviour, but with our own key and copy.
4. **Fetch the app only after the escape.** The interactive shell stays a separate chunk loaded on
   demand; the gate route prerenders standalone. Total added bytes: a few hundred bytes of inline
   script + one small static page. Budget gate asserts **zero new initial chunks**; the ~369 KB
   measured total must not grow, and the track toward the ≤ 200 KB cap is unchanged by this work.
5. **No third-party bytes on the escape page.** If store links are ever shown, plain
   `<a rel="noopener noreferrer">` with inline SVG marks only — no badge CDNs, no new origin in the
   CSP, no extra DNS/TLS handshake on a 2G-class link, and fixed-aspect boxes so CLS stays 0. The
   CTA is one 44 px target; INP ≤ 200 ms costs one class flip. On weak GPUs no animation is added;
   `prefers-reduced-motion` is inherited.
6. **Honest when things fail.** Offline or storage-blocked → the page still renders, the escape
   still works (it is markup, not a fetch), and the screen never claims to have remembered what it
   could not persist. Not-live indications from §4.8 are untouched; skeleton discipline from
   `spec/08-ui-ux.md` is unchanged.

## 3. BoardUI tokens (free tier only, no Pro)

Static semantic markup only: `bg-background-primary-default` / `-secondary-default`, text
`text-text-primary` / `-secondary`, hairline `border-separator-border`, icons
`@remixicon/react` with `text-foreground-icon-*`, composite type (`text-title-2-medium`,
`text-body-medium`, `text-caption-1-semibold`), `cx()` merge, 44 px targets, visible focus ring,
logical (RTL-safe) spacing, dark mode via tokens — never `dark:` colours or hex. No Pro component;
no new motion, so no `prefers-reduced-motion` handling beyond the existing reset.

## 4. One REQ-named gate

`REQ-056_continue_in_browser_handoff_is_static_and_byte_stable`

Playwright + build-report check (name carries the id per `tools/trace_check.py`): build the static
export; serve it; on a phone-width coarse-pointer context visit `/` and assert (a) the handoff
screen renders from prerendered HTML with JavaScript disabled, (b) the initial-JS gzip baseline is
unchanged and no chunk exists for the handoff route, (c) `/invite?token=…` renders the invitation
flow, never the handoff, (d) with storage writes throwing, "continue here" still navigates and a
later visit asks again. Supporting, not a second REQ gate: the JS budget build report runs as-is.

## 5. First slice

- Web only: extend the `BOOT_SCRIPT` in `app/layout.tsx` with the coarse-pointer/deep-link check
  (≤ ~250 bytes, no allocation-heavy parsing); add one static prerendered handoff route (no
  `"use client"`); record the localStorage key and deep-link key list in
  `docs/DEPENDENCIES.md`-adjacent notes; our own copy, our own marks.
- Exit: the REQ-named test green; JS report shows 0 new initial chunks; axe zero critical/serious;
  RTL smoke clean; `REQ-056` traceability row updated in `docs/TRACEABILITY.md`.
- Explicitly out of this slice: the Expo/iOS/Android apps themselves (M32), store-asset creation,
  universal links / App Links and their signing, analytics on the handoff, any rules engine, any
  upstream code, wording, asset or brand, any Pro component.
