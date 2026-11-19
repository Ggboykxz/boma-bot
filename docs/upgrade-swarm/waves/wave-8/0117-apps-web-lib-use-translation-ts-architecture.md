# 0117 — `apps/web/lib/use-translation.ts` — architecture proposal (v0.4.0)

Module: `apps/web/lib/use-translation.ts` (17 lines, read this session).
Lens: architecture / structure of the `t()` hook and its contract.
Spec anchor: REQ-057 (i18n and RTL, SHOULD), `spec/01-requirements.md` §465–471; ADR-013
(static export) via the locale boot path.
Claims: CONFIRMED = read in this tree this session (hook, `lib/i18n.ts`,
`components/providers/app-providers.tsx`, `components/foundations/direction/direction.tsx`, grep of
callers); numbered upgrades and risks are PROPOSAL/GUESS (design inference, not implemented).

## Current structure (CONFIRMED)

The hook is a deliberate seam: `"use client"` + `useLocale()` from React Aria + `t()` from
`./i18n`, returning `(key: string) => t(locale, key)`. It holds no catalogue data itself.
The locale travels `html.lang` → `useSyncExternalStore` in `AppProviders` → `I18nProvider` →
`useLocale()` → `t()`, so catalogue lookup and React Aria direction share one source (REQ-057).
Server snapshot is always `"en"`; boot script writes `lang`/`dir` before paint; a `MutationObserver`
flips locale after hydration. `t(locale, key)` calls `languageOf()` (strips BCP 47 region), looks up
the catalogue, then falls back English → key (REQ-057 rule 1). ~62 `t()` calls across ~35 components.

Observed structural facts (CONFIRMED):

- The returned translator is a fresh arrow function on **every render** — no `useCallback`/`useMemo`.
  Two callers already list `[t]` as a `useCallback` dependency (`admin/access-screen.tsx:118`,
  `admin/plugins-screen.tsx:75`), so those callbacks are invalidated every render.
- The signature is `(key: string) => string`: untyped keys, no autocomplete, no compile-time check
  that a key exists or that its placeholders are supplied.
- No interpolation exists. `i18n.ts` stores `"thread.external-agent.connected": "Connected to {name}"`
  but no caller passes vars, so a translation of that key would render the literal `{name}`.
- Direction is derived twice: `i18n.direction()` (its own `RTL_LANGUAGES` set, exported but unused by
  this hook) and `useLocale().direction` inside `DirectionProvider`. Two sets can drift.
- The hook returns only the translator; callers needing locale/direction import `useLocale()` /
  `useDirection()` separately. Three hooks, one concept.

Invariants to preserve: missing string → English → key (never blank, never a silent key leak as
normal output); direction from locale, never from text; RTL mirrors layout but not charts or code;
all user-visible text through `t()`; no new wire/format change without a DEVIATION entry.

## Proposed upgrades (v0.4.0)

### 1. Stabilise translator identity with `useCallback` (PROPOSAL)
Return `useCallback((key) => t(locale, key), [locale])`. Identity then changes only when the locale
actually changes, which fixes the two `[t]`-dependent `useCallback`s and any future memoised child.
*Risk:* low, internal. Must grep all ~62 call sites for anything relying on a new function per render
(none found so far); gate on UI goldens and axe. Do not memoise across a locale change — that would
freeze text in the old language.

### 2. Typed keys and typed interpolation, without breaking callers (PROPOSAL)
Derive `TranslationKey` from the catalogue's English keys and make the hook return
`<K extends TranslationKey>(key: K, vars?: VarsOf<K>) => string`. Add a small ICU-style `{name}`
substituter so placeholder keys stop rendering literally. Keep an escape hatch (`string`) during
migration so the 62 literal call sites can move in stages.
*Risk:* highest. Typing keys against the hand-maintained `CATALOGUES.en` will break the build on any
typo and on keys that exist only in non-English catalogues; it also risks drift from the "ICU-based
and shared with the Rust backend" catalogue described in `i18n.ts`'s header. Prefer generating the
key union from the shared catalogue in a build step, not from this TS copy. Interpolation changes
rendered output for placeholder keys — a user-visible fix, but test it. Internal API change, not a
wire change: no DEVIATION needed unless catalogue content changes.

### 3. One cohesive `useI18n()` result, keep `useTranslation()` as a wrapper (PROPOSAL)
Expose `useI18n(): { t, locale, language, direction, isRtl }` computed once from `useLocale()` plus
the shared `languageOf`/`direction` helpers; make `useTranslation()` return just `useI18n().t`.
No call site changes, and new code gets locale/direction from one object.
*Risk:* decide authority for direction — React Aria's `useLocale().direction` or `i18n.direction()`.
They must not disagree; keep a single RTL language set (import it into `direction.tsx` rather than
maintaining two) or RTL tests will catch a mismatch. Do not expand the hook's public surface into
layout concerns (`useDirection()` stays for interactions).

### 4. Make the hydration / locale-switch path an explicit contract (PROPOSAL)
Add a focused test: render with `lang="he"`, assert Hebrew strings + `dir="rtl"` after hydration with
no mismatch warnings, then mutate `lang` and assert `t()` output and translator identity update
together. The `useSyncExternalStore` design already avoids the mismatch; this locks it in for a
future locale switcher and documents that all text changes atomically.
*Risk:* test setup must set `<html lang>` before mount; static export (ADR-013) means the boot script,
not the server, owns the attribute — replay `spec/golden/ui/*.json` rather than inventing a new
server locale path.

### 5. Decouple the hook from catalogue storage (PROPOSAL)
Keep `use-translation.ts` a pure React binding and move catalogue data toward generated assets
(JSON from the shared ICU source) consumed by `t()`. The hook then never knows whether messages are
inline or loaded. Reduces the current duplication of catalogue data in a 1,227-line TS file.
*Risk:* static export forbids runtime fetching — catalogues must be bundled at build time. A
generation step needs provenance in `docs/DEPENDENCIES.md`; fallback semantics must stay byte-identical
to REQ-057 rule 1. Sequence after upgrade 2 so the type source and the data source agree.

## Non-goals / constraints

- No change to `t()` fallback semantics, wire contract or catalogue content without owner approval.
- No second i18n/direction library; reuse React Aria + the existing BoardUI `DirectionProvider`.
- Keep all copy through `t()`; new keys need catalogue entries, never literals.
- Shared-layer changes (boot script, `I18nProvider`, `i18n.ts` catalogue) are separate, gated commits —
  not bundled into this refactor.
