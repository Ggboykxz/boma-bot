# UI / UX / motion audit — 2026-11-16

Audit of the web UI as shipped on `dev` (commit `3377c69`), per the standing request: a full
Playwright-driven sweep with a focus on animations and user experience. No product code was
changed as part of the audit; the one exception is recorded under **U-09** (a regression found
and fixed during this session, locked by a test). The audit convention writes only under `docs/`;
`tools/ui-audit.mjs` is the re-runnable harness, kept in the repo for that reason.

## Method and environment

- Built static export (`npm run build`, exit 0) served by the Rust binary via `BOMA_WEB_ROOT`
  (REQ-055), exactly like production. Loopback only.
- Deployment A (`:3010`, `BOMA_LOCAL_FIRST=true`): fresh database `boma_audit`, migrations applied,
  bot **Riley** created via loopback RPC, one message sent and answered by the scripted provider.
  This is what the screenshots show.
- Deployment B (`:3011`, normal mode): the same export without local-first, used to audit the
  public/auth screens, which redirect to `/app/` on deployment A.
- Headless nix chromium 143 via Playwright; every route walked at 1280×800, three key screens also
  at 390×844; full-page screenshots (`docs/screenshots/ui-audit-2026-11-16/`), per-route console /
  pageerror / failed-request / bad-response capture, `axe-core` run per route, entrance animations
  sampled with an `addInitScript` poller (50 ms, 4 s) because `getAnimations()` alone only sees
  animations in flight; a control page animated a known `@keyframes` proved the probe detects
  them; CLS/LCP via `PerformanceObserver`; focus walk on `/sign-in` (10 Tab stops, ring styles
  computed); raw data in `docs/audits/2026-11-16/ui-audit-results.json`.
- Note on the earlier passes: the first sweep was taken while the local-first server had died
  mid-run (SIGKILL; cause not established — available memory was ~28 GB). Screenshots from that
  window showed the sign-in fallback on every route; every number in this report comes from the
  healthy-server re-run (except where marked). The dead-process event is recorded in
  `docs/STATE.md` (see the M28/audit session entry).
- Benign noise, excluded from findings: 37 client `fetch` `net::ERR_ABORTED`s, all Next.js
  `<Link>` prefetches of page destinations (e.g. 9× `/app/`); aborting a prefetch is normal. The
  `/rpc/threads/subscribe` SSE abort on navigation is likewise excluded.
- Console 4xx/5xx from capability-off features are logged but *by design* (U-08).

## Verdict

The product is functional, token-disciplined and keyboard-usable on the audited paths, and it is
**completely static**. Zero CSS animations, zero WAAPI animations, zero CSS transitions running
on the landing (redirects to `/app/` on local-first) or in a room — in normal mode *and* under
`prefers-reduced-motion: reduce`. A bespoke motion vocabulary exists in `styles/globals.css`
(`t-stagger-line`, `.animate-number-fade` 220 ms, `.animate-row-grow-in` / `.animate-check-draw`
200 ms, `.animate-flap-fall`/`-rise` 150 ms, `ai-chat-text-shimmer`, soft-circular view-transitions)
**with `prefers-reduced-motion` guards already written, and nothing in the application uses it.**
The hover states that do exist are `transition-colors` only. The owner's instinct ("les animations
et l'expérience utilisateur") names the right area, and the headroom is large — the design work
appears to be done and dormant.

## Findings (ranked)

### U-01 — No motion anywhere (design debt, not a bug)

Sampled entrance animations: 0 found (probe verified on a synthetic animated page). No
`transition-property`/`animation-name` is exercised in the product screens; the only related
rules are three `transition-colors` instances in artifacts-grid, board-screen and
notifications-panel, plus dormant utilities in `styles/globals.css` (~10 guarded keyframes and
stagger helpers) and an unused liquid-glass (`.lg-*`) recipe. `view-transition` rules exist for
theme switching, and `.theme-transitioning` freezes transitions — but no element ever triggers
them, and `--lg-ms` is never resolved at runtime.

Conventions the code already established (stagger lines, 150–220 ms durations, ease-out draw) and
BoardUI's motion language (blur-in entrances, hover at 150 ms, press darkens and never scales,
exits faster than entries, reduced-motion guarded) are compatible: wiring the dormant vocabulary
into screen mounts, list rows, message arrival, first-token wait (shimmer), approval-card draw-in
(`.animate-check-draw`) and counters (`.animate-number-fade`) is a small, well-bounded change —
and it is a product/design decision, not a drive-by edit.

### U-02 — `text-text-tertiary` fails WCAG AA everywhere (serious)

Computed contrast on light surfaces: `--color-text-tertiary` is `neutral-400` ≈ **2.4–2.6:1**
against `#ffffff` and `#f7f7f7` (AA normal text needs 4.5:1; only ~3:1 for large text).
`--color-text-secondary` (`neutral-500`) measures 4.74:1 on white but 4.43:1 on the secondary
surface — the code already knows (`app-shell.tsx` comments that 4.42:1 "is under the 4.5:1 the
axe gate demands" and routes around it there, but not elsewhere).

Axe flagged tertiary-text nodes on: `sign-in`, `sign-up` (the password-flow note),
`onboarding-3` (`p`), `artifacts` (`.h-full > p`), `admin-access` (2 × `section > p`),
`admin-plugins` (`p`), `integrations-setup` (5 × links/captions incl. `.text-text-tertiary`).
Fixing the token (or the affected pairings) is a design-token decision — `styles/theme.css` and
Figma are the source of truth; flagged for the owner, not changed here.

### U-03 — Duplicate/nested `main` landmarks on artifacts and board (moderate)

`artifacts`: `landmark-main-is-top-level`, `landmark-no-duplicate-main`, `landmark-unique`
(`.h-full > main` inside the shell's `main`). `board`: same two, plus `page-has-heading-one`
(no `h1` on the board screen). The shell owns the top-level `main`; these screens add their own.
One-line-ish fixes in each screen's root element; needs an owner decision only if the goldens
pin the current shape.

### U-04 — Dead navigation and a dead theme bootstrap (moderate)

The header nav (`components/shell/app-shell.tsx`) renders **Threads** and **Settings**, both
`href="/app/"`. "Settings" is a placeholder that goes nowhere. The layout's pre-paint bootstrap
reads `localStorage["boma-theme"]` and toggles `.dark` on `<html>` — **nothing in the product
ever writes that key**, so the whole dark theme (which the tokens support, and which flips via
`.dark`) is unreachable from the UI. Either wire a real settings/theme control or drop the dead
affordances until they exist.

### U-05 — Keyboard-only path is logical; one inert stop (minor)

Sign-in walk (10 stops, deployment B): Boma Bot → email → password → Sign in → Forgot → Create
account → (body) → loops. All interactive stops show a visible ring (`box-shadow` in the lab
range, not `outline`, matches BoardUI's focus-visible ring). The `(body)` stop is a focus landing
on a non-focusable element — inert but present; one stop per page cycle is a smell. On `/app/`
the empty threads home has no focusable element in the main region (expected: nothing to act on),
then Tab returns to the header nav. No traps, no hidden stops.

### U-06 — Mobile: usable but the rail is not adapted (minor, out of spec today)

At 390×844 the full rail (Bots/Groups/Activity) stacks above the thread — ~350 px of vertical
chrome before content; no collapse, no drawer, no bottom bar. The room remains fully usable and
composer is reachable, and desktop parity is unaffected. The spec's mobile work (REQ-056 / M32)
is Expo/RN against the same RPC contract, not the web build, so this is a choice, not a
violation — recorded here because the web export is what a phone opens by URL.

### U-07 — Accessibility overall: no criticals, some serious/moderate (mixed)

21 routes, zero `pageerror`s, zero failed requests (after excluding prefetch aborts), no missing
labels spotted on the walked paths. axe results: serious = U-02 contrast nodes; moderate =
`region` (forgot-password, reset-password, invite: the footer link block sits outside any
landmark) and the U-03 landmark/unique findings. `not-found` logs the document's own 404 in the
console (expected). The CI Playwright a11y suite passes — its scope is a few screens; this audit
is the first full-route sweep, so it does not contradict CI, it extends it.

### U-08 — Capability-off routes probe loudly (hygiene, not a defect)

- `room`: `/rpc/computer/status` answers 503 (no computer backend); the panel correctly renders
  "absent" and hides itself — but the console shows a 503 every mount.
- `board`: `/rpc/boards/get` answers 404 (flag off) → the screen logs a 404 (still renders the
  unavailable state gracefully).
- `integrations-setup`: `/rpc/messaging/list` 404 likewise; `shared-page-valid`/`-invalid` 404
  (no published page on a fresh deployment) likewise.

Each is handled in the UI; all print console errors. Treating the expected "feature absent"
answer as non-error (silent or `info`) would keep consoles honest for operators. Server-side,
`GET /rpc/*` capability probes could answer 204 or a documented capability shape instead of 404 —
that changes the wire; recorded as a proposal, not done.

### U-09 — The web fallback lost its security headers (regression, fixed this session)

Not an audit-scope change, recorded honestly because the audit caught it: while restructuring
`router.rs` for the automation surface (M28), the `fallback(web_fallback)` ended up on the root
router without any of the layers (`nosniff`, `no-referrer`, `no-store`, request-id, body cap,
timeout, request logging). Deployment A/B curl showed the static export without `nosniff`.
Fixed by moving the layer set onto the fallback-carrying root exactly once; the automation
surface keeps its own equivalent layers (and its own longer timeout). Locked by
`req_055_the_web_fallback_carries_the_same_security_headers_as_the_api`. API routes were never
affected (they kept the layers inside `main` throughout).

### U-10 — What is good (keep)

- Semantic BoardUI tokens everywhere observed: no raw palette classes or hex literals in the
  audited markup; dark mode flips via `.dark` on `<html>` with a pre-paint bootstrap (no flash).
- The auth screens state honestly what the build ships ("password flow only; passkeys land with
  the WebAuthn slice"); sign-in focus order is logical; the local-first redirect works
  (sign-in → `/app/` verified on deployment A).
- The room shows operational truth as text ("Push is not configured for this deployment.",
  "Snooze this conversation") without fake affordances; disabled Send is visibly disabled.
- CLS ≈ 0.0004, LCP ≈ 68–72 ms (loopback, static export; the real-world LCP will be
  network-bound, not layout-bound — no shift measured at all).
- Empty threads home and empty activity/board render calm, legible states.

## Raw evidence

- Per-route JSON: `docs/audits/2026-11-16/ui-audit-results.json`
- Screenshots (desktop + 390 px): `docs/screenshots/ui-audit-2026-11-16/`
- Harness: `tools/ui-audit.mjs` (`BOMA_AUDIT_BASE`, `BOMA_AUDIT_ROUTES`, `BOMA_AUDIT_OUT`,
  `BOMA_AUDIT_SHOT_DIR` overrides; re-run any time)
- Contrast ratios computed from `styles/theme.css` (`--color-neutral-400/500`,
  `--color-background-primary-default`, `--color-background-secondary-default`) via the WCAG
  relative-luminance formula.

## Proposed direction (not started; owner decision)

A small milestone (candidate **M34 — motion pass + a11y sweep**), scoped roughly as:

1. Wire the existing dormant vocabulary (U-01): screen-mount stagger for the rail/rows, blur-in
   entrances for dialogs, shimmer on first-token wait, `check-draw` on approval cards,
   `number-fade` on counters, `transition-colors` 150 ms on interactive rows.
2. Token decision (U-02): re-tune `text-tertiary`/`text-secondary` (Figma first).
3. Landmark/h1 fixes (U-03) + footer-into-landmark on the three auth screens (U-07).
4. Dead-nav cleanup (U-04): Settings link, theme bootstrap, or both wired.
5. Optional: capability probes quieter (U-08); mobile web collapse if in scope (U-06).
6. Re-run this audit to zero serious findings + a11y suite still green.

Estimation: small-to-medium (mostly CSS class wiring + two token decisions). Everything waits
for the owner's go.
