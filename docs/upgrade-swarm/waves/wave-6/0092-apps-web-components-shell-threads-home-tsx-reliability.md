# 0092 · apps/web/components/shell/threads-home.tsx · LENS=reliability · v0.4.0 proposal

## Scope note

`ThreadsHome` is a 20-line static client placeholder: one `h1` and one `p`, both from the catalogue
through `useTranslation()`. It has no props, no router awareness, no data subscription, no state and
no error handling. It is reached as the `null` branch of `AppEntry` (`app-entry.tsx`), which also
receives every path the single-segment bot regex fails to classify (worker #080). This proposal
treats the three named states — unknown route, empty workspace, load failure — as contracts the
component currently violates. CONFIRMED = read from source this session; GUESS = runtime behaviour
inferred, not exercised.

## Current behaviour (CONFIRMED, as read)

- Renders `shell.nav.threads` + `shell.placeholder`; tokens/type (`text-headline-medium`,
  `text-text-primary`, `text-text-secondary`, `max-w-prose`) satisfy the BoardUI rules.
- `useTranslation()` → `useLocale()` from `react-aria-components`; `t()` falls back to English on a
  missing key, never to a blank or a raw key (`lib/i18n.ts` header) — a strength, but it can hide a
  missing translation in 13 of 14 locales.
- No fetch, no `aria-live`, no loading/empty/error branches. It cannot distinguish a healthy empty
  space from a failed one, or the real home from a misrouted URL.
- No `error.tsx`/`not-found.tsx` under `app/app/` (only route folders + `page.tsx`): a throw here
  unmounts `AppShell` and the rail, not just the screen (see #080).

## Failure modes

1. **Unknown route renders the threads home (CONFIRMED via #080).** `botIdFromPath()` uses
   `/^\/app\/([^/]+)\/?$/`; two-segment deep links (`/app/artifacts/<id>/`, `/app/g/<id>/`) and
   genuinely bogus paths return `null`, so `AppEntry` renders "Threads / Pick a bot…" with no error,
   no navigation and no log. The placeholder becomes the catch-all for wrong URLs and masks
   deep-link defects. Severity: high for trust; the component has no way to tell "home" from
   "misrouted" without a route signal from above.

2. **Empty workspace contradicts itself (CONFIRMED).** The copy instructs "Pick a bot in the rail",
   but a fresh space has none: the rail shows `shell.rail.bots.empty` ("No bot yet.") while the home
   sends the user to an empty list. There is no create-bot/onboarding CTA and no actionable next step
   from the home. Transition empty → populated is silent (GUESS: no `aria-live` status announced).

3. **Load failure is invisible and unrecoverable (CONFIRMED for today; GUESS for future data).**
   The component fetches nothing, so a failing roster/threads RPC cannot be surfaced here; the rail
   swallows its own failures (`groupsList`/`botsList` `.catch(() => …)` in `bot-rail.tsx`). If v0.4.0
   lets the home list recent threads/bots, an unchecked rejection would paint an empty-looking home
   indistinguishable from a truly empty workspace — the exact drift flagged for `BoardScreen`
   (#066). No retry affordance, and `rpc.call` sets no timeout (GUESS from #066), so a hung request
   looks like permanent emptiness.

4. **Hidden provider/catalogue dependency (GUESS).** `useLocale()` requires `DirectionProvider`; a
   test harness, Storybook or an error-boundary remount without it may throw or lose locale, taking
   the home down with it. `t()`'s silent English fallback can make a missing key look "fine" in
   Arabic/Hebrew while RTL layout still applies.

5. **No local boundary (CONFIRMED).** Any render throw here (malformed data from a future thread
   list) loses the whole shell; there is no error UI inside this component.

## Concrete hardening (v0.4.0)

- **H1 — make the home honest about the route.** Accept a `variant: "home" | "notFound"` prop (or
  consume the shared `routeFromPath()` classifier) from `AppEntry`; for `notFound` render the
  existing `notfound.*` shape with a back link instead of the threads copy. *Risk:* externally
  visible change → DEVIATION candidate needing `docs/PARITY.md` + operator approval; coordinate with
  worker #080's H1/H3 so there is one classifier, not two.

- **H2 — branch on roster state.** Subscribe to `listRoster`/`serverRoster` with
  `useSyncExternalStore`, exactly as `BotRail` does. Explicit states: `loading` (`roster === null`),
  `empty` (length 0, with a catalogue create-bot CTA), `ready` (keep current copy). Add an
  `aria-live="polite"` status for the empty → populated transition. *Risk:* the SSR snapshot `null`
  also means "loading"; gate on a hydration flag so the empty state never flashes before hydration.
  A new CTA string needs all 14 locales or an explicit fallback decision; list it as a proposal in
  `docs/STATE.md` if it is outside the spec.

- **H3 — visible failure + retry.** Add a catalogue `shell.home.error` and a retry that calls
  `refreshRoster()`; render with `role="alert"` and BoardUI error tokens
  (`text-text-error-primary`, `border-border-error-default`). Treat an explicit empty result as
  empty and a rejection as error — never conflate them. *Risk:* no second client-side flag source;
  keep i18n fallback discipline; do not auto-recover silently.

- **H4 — cancellable, bounded fetch (only if the home starts loading).** Pass one
  `AbortController.signal` to every RPC, guard state writes on `!signal.aborted`, and use a monotonic
  request id so only the newest response commits. *Risk:* the phase machine grows; the missing
  client timeout is cross-module (`rpc.call`) — flag to that owner rather than fixing it here.

- **H5 — survive the boundary.** Keep the component total: never throw on props/locale, and add
  `app/app/error.tsx` so a throw keeps header/rail mounted (owned by #080 — do not duplicate).
  Prefer a Playwright assertion of the `DirectionProvider` contract over wrapping `useLocale()` in
  `try/catch`. *Risk:* an error boundary can hide defects; require a visible retry.

- **H6 — test-first coverage.** One Playwright/axe case per state — unknown route, empty workspace,
  load failure — plus an i18n/RTL pass (REQ-057) asserting no raw key in `en`/`fr`/`he` and an
  announced state change. *Risk:* route-mock timing brittleness; reuse existing fixture style.

## Explicitly rejected / deferred

- Turning this component into a bot list or a router: that duplicates the rail and `AppEntry`
  (two sources of truth). It stays a status/next-step surface.
- A second error-boundary implementation; coordinate with #080 instead.
- Silent auto-recovery from a load failure — the retry stays user-visible.

## Rollout and success criteria

Ship H2/H3/H6 first (client-only, no routing change); H1 last, after PARITY approval, because it
changes fallback behaviour. H4/H5 land with RPC/backend confirmation. Targets, not yet run:
unknown route → not-found shape and never the threads copy; empty workspace → explicit empty state
plus action; load failure → error plus retry, distinct from empty; no state write after unmount, no
hydration flash of "empty", and no catalogue key ever rendered raw.
