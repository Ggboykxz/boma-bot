# 0118 — `apps/web/lib/use-translation.ts` — reliability lens (v0.4.0)

Target: `apps/web/lib/use-translation.ts` (17 lines, client hook). Collaborators read this session:
`apps/web/lib/i18n.ts` (`t`, `languageOf`, `direction`, `SUPPORTED_LOCALES`, catalogues),
`components/providers/app-providers.tsx`, `components/foundations/direction/direction.tsx`.
Spec: `spec/01-requirements.md` REQ-057 (catalogue fallback, RTL for Hebrew/Arabic-class locales).
Claims: CONFIRMED = read/grepped in this tree this session; GUESS = inferred, needs verification.
Read-only review; this document is the only write.

## Current behaviour (read, not run)

- The hook calls `useLocale()` from `react-aria-components` and returns `(key) => t(locale, key)`.
  It creates a **new function on every render**: no `useCallback`, no memo (CONFIRMED).
- `locale` is the BCP 47 tag fed to `I18nProvider` by `AppProviders`: `useSyncExternalStore` with
  server snapshot `"en"` and client snapshot `<html lang>`; a `queueMicrotask(listener)` in the
  subscription flips the tree from the static English HTML to the boot script's locale after
  hydration (CONFIRMED).
- `t()` normalises with `languageOf()` (split `-`, lowercase), checks `isSupported()`, then returns
  `CATALOGUES[lang][key] ?? CATALOGUES.en[key] ?? key` (CONFIRMED, i18n.ts 1197–1227).
- `t()` accepts **no interpolation parameter**. Three catalogues contain a literal `{name}` in
  `thread.external-agent.connected` (en/fr/he); grep found no consumer of that key under
  `apps/web` yet (CONFIRMED), so the trap is latent, not currently rendering braces.
- `direction()` is a separate export; the hook exposes only the translator, despite its docstring
  claiming catalogue and direction "always agree" (CONFIRMED, use-translation.ts 8–12).
- Catalogues `en`/`fr`/`he` are full; `ar` and the other ten locales are ~10-key stubs (CONFIRMED,
  catalogue boundaries at i18n.ts 23/376/729/1083/1093–1183). `ar` is both RTL and mostly English
  fallback text (CONFIRMED by combining `RTL_LANGUAGES` with catalogue sizes).
- Grep found no production call site of `useTranslation()` outside its own definition (CONFIRMED).

## Failure modes

1. **Missing keys are silent in production (medium).** The fallback chain works, but an
   untranslated new screen renders a raw id such as `thread.card.approve`; there is no dev warning,
   no counter and no `onMissingKey` seam. `t()` types `key: string`, so a typo compiles cleanly
   (CONFIRMED). Partial catalogues make this the normal state for every locale except en/fr/he,
   including RTL Arabic (CONFIRMED).
2. **Prototype-key lookup can return a non-string (medium).** `CATALOGUES[lang][key]` is a plain
   object index; `t("en", "toString")` / `"__proto__"` returns an inherited function/object, not
   `undefined`, so the fallback never fires and the declared `string` return is a lie. React then
   throws "Objects are not valid as a React child". Dynamic keys are allowlisted today in
   `lib/run-status.ts` (CONFIRMED), but the hook offers no guard of its own.
3. **Interpolation is unimplemented (high, forward-looking).** `{name}` is returned verbatim. When
   the ICU catalogue shared with Rust lands, callers will either show braces or hand-roll
   `.replace()`, with inconsistent escaping and no handling of missing/extra params or ICU
   plural/select forms. Brace substitution into an HTML attribute or `dangerouslySetInnerHTML`
   would be an injection sink; React text nodes escape, but the hook cannot enforce the caller's
   context (CONFIRMED code; product impact GUESS until M1).
4. **Direction blind spot (medium).** A consumer can read `document.dir`, `navigator.language` or a
   cookie for direction while the catalogue comes from `useLocale()`, producing English text in an
   RTL frame (or vice versa). Interpolated names inside Hebrew/Arabic sentences need `<bdi
   dir="auto">` isolation, which a plain `string` return cannot express (CONFIRMED code;
   mis-layout GUESS without a screenshot).
5. **Hydration / identity churn (medium).** Any component that derives the locale outside
   `useSyncExternalStore` will render a non-English first client render, mismatching the static
   export and causing a React warning, a flash of the wrong language and CLS. Even when correct,
   the fresh `translate` closure changes identity on every render, so `useEffect`/`useMemo` that
   list it as a dependency re-run needlessly — and once again when the locale flips after
   hydration (CONFIRMED code; warning severity GUESS).
6. **Locale input is taken on trust (low).** `getClientSnapshot()` reads `<html lang>` unvalidated;
   malformed tags degrade to English via `languageOf()`/`isSupported()` (safe today), but the
   boot-script source of that attribute was not traced (GUESS).

## Concrete hardening (v0.4.0)

- **Memoise and add an additive API:** wrap the translator in `useCallback` keyed on `locale`;
  keep `useTranslation(): (key: string) => string` callable and add an optional second
  `params?: Record<string, string | number>`. Add a sibling `useI18n()` returning
  `{ translate, locale, direction, isRtl }` with `direction` computed from the *same* locale via
  `direction(locale)`, so catalogue and layout cannot drift. No current call sites, but keep the
  callable form for docs proposals and future callers.
- **Own-key guard + typed keys:** change lookup to `Object.hasOwn(CATALOGUES[lang], key)` before
  returning, or build catalogues with `Object.create(null)`; export
  `MessageKey = keyof typeof CATALOGUES.en` plus template-literal families
  (`run.status.${RunStatus}`, `run.trigger.${RunTrigger}`) so dynamic keys still typecheck. Keep a
  `key: string` overload for unmodelled keys.
- **Missing-key observability:** in dev, `console.warn` once per `(locale, key)` pair (module-level
  `Set` dedupe); expose an optional `onMissingKey` callback in `t()`/provider for production
  reporting with a counter only — no key/PII payload. Preserve the English-then-key fallback
  required by REQ-057; bump `docs/QUALITY_BAR.md` tests to assert it.
- **Minimal interpolation:** implement brace substitution inside `t(locale, key, params)` for
  `\{(\w+)\}`; leave unknown/unclosed braces untouched, keep the placeholder and dev-warn on a
  missing param, ignore extra params, coerce numbers with `String()`. Never inject into attributes
  or HTML from this API; document that user-supplied values must sit in `<bdi dir="auto">` in RTL.
  Defer ICU plurals/select/number formatting to the shared Rust/i18n package and record the gap in
  `docs/STATE.md` as a proposal.
- **Hydration contract:** keep the hook free of `document`/`window` access and of `useState`; it
  must only read the provider. Add a Playwright REQ-057 test asserting the SSR HTML is English,
  post-hydration Hebrew (`html[lang="he"]`, `dir="rtl"`) text renders with **no console errors**,
  and no layout shift beyond the agreed budget.
- **Tests (unit + e2e):** `t()` fallback for missing locale/key, prototype keys (`toString`,
  `__proto__`), interpolation happy path and missing/extra params, `direction("he-IL") === "rtl"`,
  and stable translator identity across re-renders with the same locale.

## Risks / trade-offs

- Changing the return shape would break future consumers; keep the callable signature and add
  `useI18n()` rather than replacing it. Extra optional parameters are backward compatible.
- Typed keys can reject legitimate dynamic lookups; the template-literal families and `string`
  overload must be kept, otherwise `lib/run-status.ts` and similar helpers stop compiling.
- Dev warnings on every missing key can flood the console during tests; dedupe and strip them from
  production builds. A production callback adds a new integration seam — keep it opt-in and
  payload-free.
- Brace interpolation is a deliberate subset of ICU; if callers start passing plural/select
  messages the output will be wrong rather than empty. Flag this as a DEVIATION risk in
  `docs/PARITY.md` before the shared catalogue lands, and never claim ICU parity for v0.4.0.
- Exposing `direction` from the hook does not make components RTL-safe by itself; logical styles,
  mirrored glyphs and bidi isolation remain the caller's job, and Arabic's English fallback text is
  a spec-level tension (REQ-057 mandates both fallback and RTL) — document, do not silently "fix".
- `useCallback` does not eliminate the post-hydration locale switch (it is intended); it only stops
  identity churn on unrelated renders. Tests must distinguish the two.
