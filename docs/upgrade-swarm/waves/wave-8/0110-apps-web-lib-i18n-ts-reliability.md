# Wave 8 / worker 0110 — `apps/web/lib/i18n.ts` reliability (v0.4.0 proposal)

Scope: read-only review of `apps/web/lib/i18n.ts` (1227 lines) and its direct consumers
`lib/use-translation.ts`, `components/providers/app-providers.tsx`, `app/layout.tsx`,
`components/foundations/direction/direction.tsx`, `tests/locale.spec.ts`, `tests/shell.spec.ts`.
Every claim below is CONFIRMED by reading those files unless tagged GUESS.

## What the module guarantees today
- `languageOf()` lowercases the first `-` subtag; `direction()` checks RTL_LANGUAGES
  `ar, fa, he, ur`; `isSupported()` checks the 14-entry SUPPORTED_LOCALES; `t()` resolves
  locale -> catalogue -> English -> key.
- Catalogues: `en/fr/he` are full; the other 10 ship 8 strings by design (REQ-057).
  A `{name}` placeholder exists in 3 catalogues (`thread.external-agent.connected`);
  `t()` does no interpolation.
- Locale is resolved once in `AppProviders` via `useSyncExternalStore` (server snapshot `en`,
  client reads `document.documentElement.lang`); the layout boot script writes `lang`/`dir`
  before first paint.

## Failure modes
1. Missing / malformed locale (HIGH).
   - `isSupported()` does NOT call `languageOf()`, unlike `direction()` and `t()`. A caller
     passing `he-IL` gets `false` while `t("he-IL", key)` translates. CONFIRMED.
   - `languageOf("he_IL")` (underscore, a common localStorage/HTTP form) returns `"he_il"`, so
     `t()` falls back to English; the boot-script regex `/^(ar|fa|he|ur)([-_]|$)/i` treats the
     same value as RTL. CONFIRMED divergence: English text on an RTL page.
   - `languageOf("")` returns `""`; a null/undefined arriving from a boundary throws on `.split`.
     No normalization of whitespace, `_`, or script subtags. CONFIRMED (code); GUESS on how often
     boundaries supply such values.
2. `t()` fallback semantics (MEDIUM).
   - Header comment (lines 8-9) says a missing string falls back to English "never to the key",
     but line 1226 returns the key. `tests/locale.spec.ts` repeats "never to the key" without ever
     testing a nonexistent key. The documented contract is self-contradictory. CONFIRMED.
   - No warning or telemetry on fallback: a typo'd key or an untranslated string silently renders
     English (or a raw key) in production. CONFIRMED.
   - No interpolation: `{name}` ships literally unless every caller replaces it itself. CONFIRMED.
   - `CATALOGUES[language][key]` has no runtime guard if SUPPORTED_LOCALES and CATALOGUES drift;
     the TS `Record<Locale, ...>` prevents this at build time today. GUESS that generated/edited
     catalogue data could reintroduce the drift in v0.4.0.
3. Hydration (MEDIUM).
   - Server snapshot is hardcoded `en`; the client reads `document.documentElement.lang`. This is
     correct only while the boot script runs. If it is blocked or CSP-dropped, the page stays
     English with no client fallback to `navigator.language`. CONFIRMED by code; GUESS on CSP.
   - `subscribe()` depends on `queueMicrotask(listener)` plus a `lang` MutationObserver; a locale
     switch that sets only `dir`, or a future switcher that forgets `lang`, is invisible. CONFIRMED.
   - `lang` is copied from unvalidated localStorage; garbage values become the React Aria locale
     and silently degrade to English. CONFIRMED.
4. RTL detection (MEDIUM).
   - RTL knowledge is duplicated three ways: the `i18n.ts` set, the layout boot regex, and
     implicitly SUPPORTED_LOCALES. Adding an RTL locale needs edits in both places; a missed edit
     gives `dir=ltr` with an RTL catalogue (or the reverse). CONFIRMED.
   - `direction("fa")`/`"ur"` return `rtl` though those locales are not shipped: fine defensively,
     but direction and translation can disagree by design. CONFIRMED.
   - `direction()` accepts any string and always returns a Direction, with no "unsupported"
     signal for callers. CONFIRMED.

## Concrete hardening for v0.4.0 (with risks)
1. Add one `normalizeLocale(input: string | null | undefined): Locale` that accepts `-` and `_`,
   lowercases, validates against SUPPORTED_LOCALES, and defaults to `en`. Route `isSupported`,
   `t`, `direction`, and `getClientSnapshot` through it.
   Risk: externally visible change — `isSupported("he-IL")` becomes true and `t("he_IL", ...)`
   becomes Hebrew. Record it in docs/PARITY.md as a DEVIATION or gate it behind a flag. An additive
   helper keeps existing call sites compiling.
2. Make RTL one source of truth: export the RTL set from `i18n.ts` and feed the same set into the
   layout boot script as a serialized constant (or derive `dir` from the normalized locale). Add
   coverage for `he-IL`, `he_IL`, `ar-EG`, `fa-IR`.
   Risk: the boot script is a string in a server component, so code cannot be imported — only a
   generated value. Keep it synchronous to preserve the no-flash behavior and the `en` SSR snapshot.
3. Harden `t()`: guard `const catalogue = CATALOGUES[language]`; reconcile the header comment with
   the key fallback; add an optional `params` argument for `{name}` substitution and a dev-only
   `console.warn` on unknown keys / unresolved placeholders. Never change existing English strings.
   Risk: dev warnings would be noisy across the 10 partial catalogues — warn only on missing keys
   and placeholder mismatch. Document that interpolation output is React-escaped and must never be
   fed to `dangerouslySetInnerHTML`.
4. Hydration: validate `lang` through `normalizeLocale` in `getClientSnapshot`, fall back to
   `navigator.language` when `lang` is absent, and keep observing `lang` only.
   Risk: the client-only `navigator.language` path must not leak into the server snapshot; verify
   zero hydration warnings in Playwright, preserving `suppressHydrationWarning` scope on `<html>`.
5. Tests: extend `tests/locale.spec.ts` for regional/underscore tags, empty/null input,
   `isSupported("he-IL")`, placeholder interpolation, and a `dir=rtl` assertion; keep REQ-057 in
   test names. (This review writes no code; tests are proposed work for v0.4.0.)

## Priority
P0: items 1 and 2 (wrong-language / RTL divergence). P1: items 3 and 4 (silent fallback,
hydration fragility). P2: item 5 coverage. No change to catalogue contents is proposed.
