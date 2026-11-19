# 0044 — continue-in-a × i18n/RTL parity proposal (Boma Bot)

- Brief: id 44, area `continue-in-a`, lens `i18n-rtl` (`docs/openbot-swarm/briefs-1000.jsonl`).
- Status: **PROPOSAL only**. No product code, spec, golden or catalogue was changed.
- Provenance: upstream read as **data** only (clone `/tmp/opencode/openbot-parity`, HEAD `559d5ac`):
  `WebPhoneAppScreen.tsx`, `docs/web-client.md`, `docs/architecture/browser.md`, `AGENTS.md`
  ("Phone screens"), `packages/i18n/src/locale.ts`, `languages.ts`, `messages/*/webClient.ts`
  (15 locales, five `phoneApp` keys each — CONFIRMED this session). No upstream code, asset,
  store URL, wording or brand enters Boma Bot; no Pro.

## 1. Upstream capability, in my own words

On a phone-like device (coarse pointer, narrow width or short landscape height) the web client
opens not on the app but on a polite handoff: a product lockup, short title, one descriptive
sentence, two store buttons (iOS / Android, reordered by user agent so the likely platform is the
primary action), and a quiet "continue in the browser" affordance. Choosing it remembers the
choice on the device and drops the reader into the web layout. The surface is catalogue text,
direction comes from the locale (Arabic and Hebrew mirror, region subtags included), and a signed
deep link bypasses the gate entirely. It is kept, not extended: a handoff, not a second product,
and desktop stays computer-first. None of it approves or triggers agent work, and every visible
string is localised with English fallback.

## 2. Parity position

- **Not built here.** `before-m5` has no phone handoff, store badges or store URLs (grep clean).
  Mobile is REQ-056 (Expo, SHOULD) and not started; the web shell is a static, desktop-first
  export. Inventing a store screen or mobile gate needs an owner decision, an REQ id and an
  ADR; this lens does not smuggle one in.
- **Shipped already:** the REQ-057 chassis — `SUPPORTED_LOCALES` (14), `en`/`fr`/`he` full and
  eleven partial catalogues in `apps/web/lib/i18n.ts`; `t()` falls back to English, then to the
  key (never a blank); `direction()` gives RTL for `ar`/`fa`/`he`/`ur` incl. region subtags; the
  boot script writes pre-paint `lang`/`dir` on `<html>`; `DirectionProvider`
  (`components/foundations/direction/direction.tsx`, React Aria `I18nProvider` + `useLocale`) in
  `AppProviders`; logical-class convention in `docs/UI.md`; REQ-057-named tests in `shell.spec.ts`,
  `a11y.spec.ts`, `locale.spec.ts`.
- **In scope now:** the catalogue keys, direction rules and one REQ-named Playwright test so an
  adopted handoff lands i18n/RTL-correct from day one — REQ-057's "declared support must be real."
  No wire, data-format or permission change; no DEVIATION.
- Flag, not fix: REQ-057 pins **14** catalogues; `spec/04-nonfunctional.md`, TRACE and ADR-014
  say 15 (upstream ships 15, incl. `tr`). This proposal pins 14 per REQ-057; DEC-11 is the owner's.

## 3. i18n changes (REQ-057)

- Add one key set, `handoff.phone.*`, with Boma Bot's own copy (never upstream wording):
  `title`, `description`, `ios`, `android`, `continue`. No ICU placeholders in v1.
- Add the keys to **all 14** `CATALOGUES` in one change: full translations in `en`/`fr`/`he`, core
  label + action strings (`ios`, `android`, `continue`, `title`) in the other eleven, long tail
  falls back to English by design. Key sets stay identical across catalogues.
- No hardcoded user-visible literal anywhere in the eventual surface; the accessible name of the
  region is a catalogue string (`aria-labelledby` → the title key).
- REQ-057 includes e-mail: if a "get the app" mail is ever adopted, it renders from the shared
  catalogue (Rust side), never from hand-built HTML, with a `dir="rtl"` template for `ar`/`he`.
- Extend, never duplicate: fold the five keys into the existing `apps/web/tests/locale.spec.ts`
  parity assertions (14 locales, key-set equality with `en`, `t()` fallback). Do not add a
  second catalogue test.

## 4. RTL changes (spec/08-ui-ux.md §8.9 + BoardUI direction rules)

- One direction source only: pre-paint `<html lang dir>` plus `DirectionProvider`. No CSS-only
  `dir` switch, no second provider. The boot script already matches `ar|fa|he|ur` with region
  subtags — reuse it, don't rewrite it.
- Logical utilities only: `ms/me`, `ps/pe`, `start/end`, `text-start/end`, `rounded-s/e`,
  `border-s/e`. The store-button row order follows the **user agent** (which store is primary);
  direction mirrors placement, never the UA decision. Primary action sits at the reading start.
- LTR islands: store URLs, any e-mail/sign-in link and correlation ids in `<bdi>` / `dir="auto"`.
  Device-frame art, charts and the computer stream stay physical — never mirrored.
- Type/font: Inter ships latin subsets only, so Arabic/Hebrew glyphs silently fall back to system
  fonts; when the surface (or any `ar`/`he` screen) ships, add covering subsets or a fallback stack,
  and never add letter-spacing for Arabic. Note for the owner; do not retune the theme now.
- BoardUI only: semantic tokens (`text-text-*`, `bg-background-*`, `border-separator-border`),
  composite type utilities, `cx()` from `@/utils/cx`, icons by reference from `@remixicon/react`
  (`RiAppleLine`, `RiGooglePlayLine`, `RiSmartphoneLine` exist — store badges would need no new
  dependency), `focus-visible:ring-2 focus-visible:ring-border-focus-ring`. Dark mode rides
  `.dark` tokens; no raw palette, no `dark:` literals, no Pro, no second design system. Store URLs
  come from deploy-time config with the owner's own listings, never upstream's.

## 5. One REQ-named Playwright test

Add to `apps/web/tests/a11y.spec.ts` (name carries the id per `tools/trace_check.py`; distinct from
the existing REQ-057 shell/locale/motion names):

```ts
test("REQ-057 the continue-in-a handoff copy ships in the catalogue and mirrors for Hebrew RTL", async ({ page }) => {
  // GIVEN stored locale "he" (second pass "ar-EG"), at a 360x640 phone viewport,
  // WHEN the shell (or the adopted handoff surface) renders,
  // THEN html carries lang=he dir=rtl; each handoff.phone.* key resolves to the
  //      catalogue string (never a raw key/blank; a missing key falls back to
  //      English); the primary store action sits at the reading-start side with a
  //      visible token focus ring; any URL/e-mail/id is in an LTR island;
  //      axe reports no critical/serious finding.
});
```

Until the owner adopts a handoff surface, the browser-side assertions tighten onto `/app/` at
phone width (dir=rtl, no horizontal overflow, catalogue labels, axe clean); the catalogue half
lands first via `locale.spec.ts`. When the surface ships, the same test retargets it unchanged —
keep the name stable so the trace holds. Record runs honestly in `docs/STATE.md` (the local
`he` axe artefact is a known nix-chromium issue, green on CI); do not weaken the gate.

## 6. First slice (test-first, small, reversible)

1. Write the failing REQ-057 test above plus the `locale.spec.ts` key-parity extension.
2. Add `handoff.phone.title|description|ios|android|continue` to all 14 `CATALOGUES`; own copy;
   en/fr/he complete, eleven carry the core strings.
3. No surface, flag, route, icon import or dependency yet — those wait on the owner's call on
   REQ-056 adjacency and the 14-vs-15 locale decision (DEC-11), logged in `docs/STATE.md`.
4. If and when the surface is adopted: apply §4's logical classes, LTR islands and font covering
   in the same change; re-run the shell golden and the RTL a11y cluster.
5. Gates: web typecheck, lint, build, Playwright REQ-057 + axe (light/dark/RTL, 360 px). Update
   `docs/STATE.md` (CONFIRMED/GUESS) and `docs/UI.md` only if a component changes;
   `docs/DEPENDENCIES.md` untouched.

## 7. Boundaries

No upstream code, asset, store URL, wording or brand. No Pro, one design system, one theme. No new
REQ, store screen or transport without the owner. No animation is added by this slice. Claims are
CONFIRMED only where a command ran this session; the 14-vs-15 catalogue conflict is the owner's.
