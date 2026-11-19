# 0109 — apps/web/lib/i18n.ts — architecture (v0.4.0)

Worker #109 · module `apps/web/lib/i18n.ts` · lens: architecture · READ-ONLY.
Proposals only; no code in this change. Line refs against `before-m5`.

Status: "Today" and the discrepancies are CONFIRMED by reading `i18n.ts`, its consumers
(`use-translation.ts`, `direction.tsx`, `app-providers.tsx`, `layout.tsx`) and `spec/01,04,08`
this session; proposals and risks are GUESS (design inference, not implemented or tested).

## What the file does (structure today)

A single 1227-line module that is both the catalogue data and the loader:

- Types `Locale` (`:15`, 14-member union), `Direction` (`:17`), `SUPPORTED_LOCALES` (`:20`, 14 tags).
- `CATALOGUES` (`:22-1193`) typed `Record<Locale, Readonly<Record<string, string>>>`: three
  full catalogues (`en`, `fr`, `he`, ~350 keys each) and eleven token catalogues
  (`ar, de, es, it, ja, ko, nl, pl, pt, ru, zh`, 8 keys each, `:1083-1192`).
- `RTL_LANGUAGES` set (`:1195`), `languageOf` (`:1198`), `direction` (`:1203`),
  `isSupported` (`:1208`), `t` (`:1218`). `t` normalises with `languageOf`, looks up the
  language catalogue, else English, else the key.
- Consumers: `use-translation.ts:16` wraps `t(locale,key)` from `useLocale()`; every screen
  calls the sync `t(key)` (grep: ~80 hits). Direction flows through `DirectionProvider`
  → React Aria `useLocale().direction` (`direction.tsx:19-34`) and a second inline regex in
  the boot script (`layout.tsx:49`).

## Architecture seams / discrepancies (CONFIRMED)

1. **Direction has three sources of truth**: `RTL_LANGUAGES` (`:1195`), the layout regex
   `/^(ar|fa|he|ur)([-_]|$)/i` (`layout.tsx:49`), and React Aria's own locale→direction map
   (`direction.tsx:28`). A new RTL locale must be edited in three places; they already
   diverge (`fa`/`ur` are in the set but have no catalogue).
2. **Locale registry is triplicated**: the `Locale` union, `SUPPORTED_LOCALES`, and the
   `CATALOGUES` keys are maintained by hand. Only the map is compiler-checked (`Record<Locale, …>`).
3. **Normalisation is inconsistent**: `direction()` and `t()` call `languageOf()`, but the
   exported `isSupported()` tests the raw string, so `isSupported("he-IL")` is `false`
   while `t("he-IL", …)` resolves Hebrew. External callers get a different answer than the loader.
4. **Declared support ≠ real support**: 11 of 14 catalogues translate 8/~350 keys (~2%).
   REQ-057's rationale is "declared support must be real"; `SUPPORTED_LOCALES` conflates
   "renderable" with "fully translated".
5. **Data ships eagerly**: all 14 catalogues live in one module imported by `use-translation`,
   so every route's bundle carries ~1200 lines of strings — relevant to the docs/PLAN.md budget.
6. **Keys are untyped**: `t(locale, key: string)` and `Record<string,string>` mean a typo or an
   invented key compiles and silently falls back to English (visible only in a locale screenshot).
7. **ICU is aspirational**: the header (`:4-6`) claims an ICU catalogue shared with Rust and
   REQ-057 mentions e-mail; this client does no interpolation. `{name}` (`:89,442,795`) is
   returned literally, and there is no plural/`Intl` layer.
8. **Stale comments**: "ten more locales" (`:1081`) but 11 follow; "`he` is here as the RTL
   proof, not as a shipped translation" (`:19`) despite a full Hebrew catalogue; "never to the
   key" (`:8`) contradicts `?? key` (`:1226`). Spec conflict: REQ-057 says 14 locales,
   `spec/04:113` says 15 (`hi`, `tr`, `pt-BR`, `zh-CN`, `zh-TW`; "ar is not shipped"); our set
   uses `pt`/`zh` and ships `ar`. Per AGENTS.md this belongs in `docs/STATE.md` — flag, do not guess.

## Proposed upgrades (v0.4.0)

### U1. One locale registry with co-located metadata
Make `SUPPORTED_LOCALES` the source: `export type Locale = (typeof SUPPORTED_LOCALES)[number]`
and keep `CATALOGUES: Record<Locale, …>` so adding a tag forces a catalogue entry. Attach a
`LocaleMeta` (language, direction, translated/complete flag) beside each tag; derive `Direction`
and RTL-ness from it rather than a standalone set.
- Risk: tuple→union inference is safe, but an external consumer importing `Locale` as a
  hand-written union must be checked. Adding a locale becomes one edit instead of three —
  verify no code mutates `SUPPORTED_LOCALES` at runtime.

### U2. Single direction source of truth
Export a pure `directionOf(locale)` (normalising via `languageOf`) as the authority; have
`DirectionProvider` and the `layout.tsx` boot script consume the same rule (the inline script
needs a stringified copy — generate it from the set, not a second literal). Keep React Aria as
the renderer but assert its result matches `directionOf` in a test.
- Risk: the boot script must stay dependency-free and synchronous; a generated copy can
  drift if not built from the same constant. Behaviour change for `fa`/`ur` (currently RTL
  without a catalogue) must be an explicit decision, not an accident.

### U3. Type-safe keys and coverage validation
Derive `MessageKey = keyof typeof CATALOGUES.en`; type `t(locale, key: MessageKey)` and each
catalogue as `Readonly<Partial<Record<MessageKey,string>>>`. Add a dev-only completeness/missing-key
warning for partial locales, and a test asserting every key exists in `en` and that `t()` never
returns an empty string.
- Risk: ~80 call sites pass plain `string` (some dynamic, e.g. `t(item.key)` in
  `app-shell.tsx:50`); type all key sources as `MessageKey` or keep a documented `string`
  overload. Warnings must be dev-gated (`NODE_ENV`) so axe/console-error gates stay clean.

### U4. Split catalogue data from loader; dynamic per-locale import
Move `CATALOGUES` into `locales/<tag>.ts`, keep `en` eager as the fallback, load other
locales via `import()` and cache in a module map populated at bootstrap (in `app-providers`),
preserving the sync `t()` API. This makes the bundle O(active locale) rather than O(14).
- Risk: true async `t()` would ripple through every screen; the preload gate must guarantee a
  locale is loaded before render (Suspense or preload await), or text would flash English.
  Measure with the same docs/PLAN.md command; do not claim a budget win without a measurement.

### U5. Interpolation layer behind the existing accessor
Add `translate(locale, key, params)` for named `{token}` substitution and `Intl`-based
number/date/plural formatting, leaving `t()` as the raw accessor; migrate the three `{name}`
keys first. Define the seam for the Rust-shared/ICU catalogue and server-side e-mail copy.
- Risk: full ICU/Lingui adds a dependency (license check per AGENTS.md) and can change rendered
  strings — a DEVIATION to record in `docs/PARITY.md` and await approval. Keep the default
  path byte-identical to today for keys without params, and isolate mixed/RTL text with `<bdi>`/`dir="auto"`.

## Execution order & gate

U1 → U2 → U3 (cheap, no behaviour) → U4 (bundle) → U5 (string change, approval gate).
Every step: `tsc` + lint + unit tests for `t`/`languageOf`/`direction`/`isSupported` green,
REQ-057 Playwright i18n/RTL tests green (`he`, `ar`, missing-key fallback), axe AA clean,
UI goldens green, bundle budget unchanged or improved, no rendered-string change until U5.

## What not to touch

REQ-057 fallback-to-English rule; the `<html lang/dir>` SSR attributes; React Aria's
`I18nProvider` contract; BoardUI DirectionProvider semantics; spec §8.8 RTL and LTR charts;
locale picker behaviour until the 14-vs-15 conflict is resolved in `docs/STATE.md`.

CONFIRMED by reading `i18n.ts`, `use-translation.ts`, `direction.tsx`, `app-providers.tsx`,
`layout.tsx`, and `spec/01-requirements.md`/`04-nonfunctional.md`/`08-ui-ux.md`.
GUESS: the exact `MessageKey` migration strategy, dynamic-import preload design, and whether
full ICU/Lingui is in v0.4.0 scope — verify with the owner before coding.
