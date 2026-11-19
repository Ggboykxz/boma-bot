# Worker #070 — `integrations-setup.tsx` — Reliability (v0.4.0)

- Module: `apps/web/components/integrations/integrations-setup.tsx`
- Lens: reliability
- Status: PROPOSAL (not implemented — review gate)
- Evidence markers follow repo rule: CONFIRMED = verified by reading source this session; GUESS = inferred.

## Context (CONFIRMED)

Post-OAuth landing page (spec §8.7, REQ-032/034/048). It fires three RPCs
(`connections.list`, `marketplace.list`, `messaging.list`) in one `Promise.allSettled`,
applies only fulfilled results, and collapses to a `"ready"` or `"partial"` phase.
`"partial"` currently means *a plugin skipped a part* (a success sub-case), not a
load failure. `rpc.call()` throws `RpcError` on any non-2xx (401 also triggers the
global `unauthorizedHandler`), and propagates abort via `AbortSignal`.
No i18n keys for error/retry states were found (`integrations.*` has none).

## Failure modes

1. **F1 — allSettled swallows total failure (HIGH, CONFIRMED).** If all three
   RPCs reject (network down, 500, expired session), every `status === "rejected"`
   branch is skipped and line 79 still sets `"ready"`. The user gets a header +
   empty page, indistinguishable from "nothing installed". The spec's own doc
   comment claims "nothing here is a false negative" — this is exactly one.
   Rejection reasons (error codes/messages) are discarded: no log, no UI signal.

2. **F2 — silent per-section partial load (HIGH, CONFIRMED).** Results are applied
   independently (lines 67–78). A single failed RPC drops its whole section with
   no marker: `messaging.list` fails → no messaging list even if installs exist;
   `marketplace.list` fails → no plugins, and `"partial"` is never set for it.
   Users cannot tell "empty" from "failed to load".

3. **F3 — OAuth callback errors ignored / `installed` param trusted (HIGH,
   CONFIRMED).** `installedFromParam` (lines 22–32) only trims/lowercases; it never
   handles `error`, `error_description`, `denied`, `state` mismatch or `code`
   failure params. A cancelled/denied OAuth shows no failure. Conversely an
   unvalidated `?installed=foo` paints a success banner for a platform that is not
   in `messagingList`/`marketplace` — a false positive. The param is never
   consumed, so refresh/back re-shows a stale "Installed …" banner indefinitely.

4. **F4 — contradictory state: banner vs. active-only filter (MEDIUM, CONFIRMED).**
   Banner says installed (F3) while line 94 keeps only `status === "active"`.
   A pending/failed/revoked install is invisible, so the page can simultaneously
   claim success and show nothing.

5. **F5 — no timeout, no retry, no recovery (MEDIUM, CONFIRMED).** The only signal
   aborts on unmount; a hung RPC pins the skeleton (line 84) forever. A transient
   failure needs a full reload; no retry affordance exists.

6. **F6 — fragile phase update (LOW, CONFIRMED).** `setPhase("partial")` inside the
   marketplace branch and the trailing functional `setPhase(current => …)` in the
   same `.then` rely on React batching order. Works today but is subtle; adding
   an error flag makes this ordering easy to break.

## Proposed hardening (concrete)

- **H1 (→F1,F2,F6):** Replace `Phase` with a derived result object. Inspect all
  three settled results once, compute booleans `loadFailed`, `partialInstall`,
  plus per-section `available` flags, and set a single state: `loading | ready |
  load-error`. Add a distinct `load-error` phase rendering an `aria-live="assertive"`
  error callout (semantic `text-text-error-primary` / `border-border-error-default`)
  and a "Try again" button that re-runs the fetch (bump a `reloadKey` in the
  effect deps). Partial per-section failure renders an inline degraded banner
  naming which data could not load, while healthy sections still render.
- **H2 (→F1):** Keep rejection reasons in state (`Array<{ns, code, message}>`),
  log via the app's structured logger if one is wired (GUESS — confirm existing
  pattern); never render raw server messages to the user (use a generic
  translated string + code for support).
- **H3 (→F3):** Parse callback params into a discriminated result:
  `{kind:"success", platforms} | {kind:"error", code, description} | {kind:"none"}`.
  Treat `error`/`denied`/`state` mismatch as an error banner, not silence.
  Whitelist `installed` values against known platform ids from
  `messagingList`/`marketplace` before showing the success banner; drop unknown
  names. Only show the banner once: after first successful render call
  `router.replace(pathname)` to strip callback params (prevents stale repeats on
  refresh/back).
- **H4 (→F3,F4):** Reconcile banner with reality: if a named platform has no
  matching installation, downgrade the banner to a "not yet active / still
  processing" notice rather than a hard success claim. Surface non-active
  installation statuses (pending/failed/revoked) in a muted row so an incomplete
  install is visible.
- **H5 (→F5):** Add a per-request timeout via `AbortSignal` composition
  (`AbortSignal.any([controller.signal, AbortSignal.timeout(N)])`, N≈10s — GUESS,
  tune to measured RPC p99) and a bounded single retry for idempotent list RPCs
  before declaring `load-error`.
- **H6 (i18n, CONFIRMED gap):** Add all new strings (error, retry, degraded,
  pending/failed status, not-yet-active) to `useTranslation` key set; no
  hardcoded user-visible text.

## Risks & mitigations

- **R1 — DEVIATION risk (CONFIRMED policy):** new error UI, URL-param stripping
  and showing non-active statuses are externally visible behavior changes. Record
  each in `docs/PARITY.md` and wait for approval before shipping; do not land
  silently.
- **R2 — false banner regression (H3):** validating `installed` against live data
  could hide a genuinely-just-installed platform if its list RPC is slow/failing.
  Mitigate by keeping the banner on callback success when data is unavailable,
  labelled "confirmation pending", and only downgrading when data is present and
  contradicts it.
- **R3 — `router.replace` loops / back-button surprises (H3):** stripping params
  on every render would loop. Guard with a ref (`consumedRef`) and replace only
  once; verify 360px, light/dark and back navigation in Playwright.
- **R4 — retry storms (H5):** bound to one automatic retry + manual button;
  keep abort-on-unmount so unmounted retries are cancelled.
- **R5 — timeout too aggressive (H5):** don't guess N; measure RPC latency first
  and set above p99, else slow networks will see spurious errors.

## Verification (test-first, REQ-tagged)

- Playwright `REQ-034`/`REQ-048`: mock all three RPCs reject → assert `load-error`
  callout + retry; mock one reject → assert degraded banner, healthy sections
  still visible; `?error=access_denied` → error banner; forged `?installed=bogus`
  → no false success; refresh → banner not repeated; pending install → visible
  non-active row. Cover light/dark, 360px, keyboard-only retry, axe 2.2 AA.
