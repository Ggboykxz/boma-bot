# 0074 — AppProviders reliability proposal (v0.4.0)

Module: `apps/web/components/providers/app-providers.tsx`
Lens: reliability
Spec anchor: REQ-057 (RTL / React Aria direction), `spec/08-ui-ux.md` §8.2 auth redirect, `app/layout.tsx` boot script.

Status of claims: "Current behavior" is CONFIRMED by reading `app-providers.tsx`, `layout.tsx`,
`direction.tsx` and `lib/api/rpc.ts` this session; proposals and risks are GUESS (design inference,
not implemented or tested).

## Current behavior (CONFIRMED)

- `AppProviders` renders `DirectionProvider locale={locale}` > `UnauthorizedRedirect` > children.
- `locale` comes from `useSyncExternalStore`: server snapshot `"en"`, client snapshot
  `document.documentElement.lang || "en"`. A module-scoped `subscribe` fires a `queueMicrotask`
  once per mount, then keeps a `MutationObserver` on `<html lang>`.
- `DirectionProvider` is `I18nProvider` + a `<div lang dir class="contents">`; it computes `dir`
  from the locale via React Aria.
- `UnauthorizedRedirect` registers a module-global 401 handler in a passive `useEffect` and clears
  it with `setUnauthorizedHandler(null)` on unmount. `rpc.ts:923` is last-write-wins, no getter.
- `<html>` carries `suppressHydrationWarning`; the provider's `<div>` does not.
- The boot script (`layout.tsx:44-50`) sets `lang` and `dir` together from `localStorage`, in a
  `try/catch` that silently swallows failures.

## Failure modes

1. **Provider ordering (GUESS).** The "all portals must descend from `DirectionProvider`" rule is
   implicit at the composition site: any future portal-rendering provider (toast, dialog, tooltip)
   inserted *above* `AppProviders` or outside its tree loses I18n direction context — React Aria
   menus then open LTR on an RTL page. Separately, the 401 handler is global state with no single
   owner: two mounted `UnauthorizedRedirect` instances (nested layout during a route transition,
   micro-frontend, or multiple test roots) race, and the first unmount clears the survivor's handler.

2. **Missing / invalid locale fallback (GUESS).** `getClientSnapshot` trusts `documentElement.lang`
   verbatim. An empty value falls back to `"en"`, but a malformed or unsupported tag
   (`en_US`, `zz`, a truncated value) is passed straight to `I18nProvider`, which may throw (blank app
   — no error boundary) or silently resolve LTR. `dir` is never read: if a script or extension sets
   `lang=he` without `dir=rtl`, `<html>` stays LTR while the inner `<div>` flips RTL — split
   direction and broken keyboard navigation. `MutationObserver` and `document` are used unguarded,
   so a non-DOM test environment or an observer-less runtime throws during subscribe.

3. **Hydration mismatch (GUESS).** The design is sound — SSR and first client render both use
   `"en"`, so the `<div lang dir>` matches; the microtask re-read switches locale *after* hydration.
   Risks: the boot script mutating `<html>` is only masked because `suppressHydrationWarning` sits on
   `<html>`; adding it to the inner `<div>` to silence a future warning would hide real mismatches.
   In React StrictMode the double subscribe/unsubscribe queues two microtasks and two observers
   (harmless today, but a place to assert no hydration warning is logged). The mount→passive-effect
   window also leaves a short interval where a 401 has no handler and no redirect happens.

## Concrete hardening for v0.4.0 (GUESS)

1. **Pin and document provider order.** Keep `DirectionProvider` outermost, add a short comment
   stating that every portal-rendering provider must be a descendant, and make `UnauthorizedRedirect`
   the sole 401 owner. *Risk:* reordering later providers changes React Aria context — verify with
   an RTL keyboard/portal Playwright check before merging.

2. **Normalize and validate the locale.** Add `normalizeLocale(raw)`: trim, `_`→`-`, canonical case,
   check against a `SUPPORTED_LOCALES` set, else `DEFAULT_LOCALE`; call it in `getClientSnapshot`.
   *Risk:* a locale the catalogue drops would now fall back to English instead of passing through;
   confirm the supported set against `spec/08-ui-ux.md` and REQ-057, mark any change as a DEVIATION.

3. **Reconcile `dir` with `lang`.** Read `documentElement.dir` too; derive the boundary direction
   from `lang` (source of truth) and, if `<html dir>` disagrees, set it in the same microtask path so
   document and React Aria never split. *Risk:* writing `dir` from React could fight the boot
   script; guard with a "only when mismatched" check and test with `boma-locale=he`.

4. **Guard the platform APIs.** In `getClientSnapshot`/`subscribe`, return `"en"` when
   `typeof document === "undefined"`; skip `MutationObserver` when absent but keep the single
   `queueMicrotask` nudge. *Risk:* extra branches are untested paths — cover with a jsdom/no-observer
   unit test.

5. **Degrade gracefully on provider failure.** Wrap `DirectionProvider` in a small error boundary
   that renders children in LTR if `I18nProvider` throws on a bad tag. *Risk:* new boundary changes
   the tree shape and snapshot output — add a REQ-057-named test for the fallback render.

6. **Make the 401 handler safe to mount twice.** Change `setUnauthorizedHandler` to return the
   previous handler and restore it on cleanup instead of clearing to `null`; tighten the auth-path
   guard to segment matches (`/^\/(sign-in|sign-up|forgot|reset)/`) to avoid false positives like
   `/signature`. *Risk:* touches `rpc.ts` (outside this module) and changes which paths skip
   redirect — confirm the real auth routes in spec before changing; otherwise keep behavior as-is
   and only fix the cleanup. Consider an isomorphic layout effect to close the registration race.

7. **Pin hydration with regression tests.** Playwright: preset `localStorage["boma-locale"]="he"`,
   assert `<html lang=he dir=rtl>`, inner `<div dir=rtl>`, zero console hydration warnings, and RTL
   arrow-key behavior in a portalled menu; repeat with a malformed locale to assert English fallback.
   *Risk:* warning capture can be flaky — assert on the React error-event hook, not raw console text.

## Constraints

- Keep all BoardUI semantic tokens / composite type utilities; do not add `dark:` colour overrides.
- Keep `getServerSnapshot` constant so hydration is deterministic; never add `suppressHydrationWarning`
  to the inner `<div>`.
- Any externally visible locale/redirect behavior change is a DEVIATION — record in `docs/PARITY.md`
  and await approval before shipping.
