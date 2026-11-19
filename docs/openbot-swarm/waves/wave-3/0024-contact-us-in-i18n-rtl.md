# 0024 — contact-us-in × i18n/RTL parity proposal (Boma Bot)

- Brief: id 24, area `contact-us-in`, lens `i18n-rtl` (`docs/openbot-swarm/briefs-1000.jsonl`).
- Status: **PROPOSAL only**. No product code, spec, golden or catalogue was changed.
- Provenance: upstream consulted as **data** only (data-only clone path in the brief). No upstream
  code, asset, wording or brand enters Boma Bot; the upstream licence stays upstream, and no
  upstream Pro component is used. The capability below is restated generically, not copied.

## 1. Upstream capability, in my own words

The "contact us" surface is the product's in-app route for a user to reach the operator without
leaving the client: a low-key entry point in the shell's secondary/footer area opens a way to send
a message — a support address, a feedback form, or a link to a hosted help channel — often carrying
the current conversation or run identifier so the operator can locate the context. Restated as
generic rules, not as a copy of any implementation:

- It is a supporting channel, not a primary action: available to signed-in users from a secondary
  nav slot, and it never approves or triggers an agent side effect.
- It composes an outbound message and, where the product records one, the user's contact identity
  plus a correlation id that ties the message back to the originating thread or run.
- It is mostly free text plus a send action — which is exactly why its copy, field labels and any
  reply e-mail must localize and mirror with the rest of the shell (REQ-057 explicitly includes
  e-mail). Getting localization in late means retrofitting every label and template.
- It handles the unhappy path too: an offline/failed send state and a "we'll reply" acknowledgement,
  both of which are user-visible strings.

This is a shell-copy and channel concern, not a permission or data-format concern.

## 2. Parity position for Boma Bot

- **Already ours:** a secondary "Support" nav entry with a service icon lives in
  `apps/web/components/application/dashboard/dashboard-sidebar.tsx`; a submitter `contact` field is
  already in the data model and `spec/openapi.yaml` (`PluginSubmission`); `spec/08-ui-ux.md` already
  offers the support correlation id behind "details". The whole i18n/RTL chassis (catalogue +
  `t()` fallback, pre-paint `lang`/`dir`, `DirectionProvider`, logical classes) is shipped.
- **Gap this lens exposes:** the "Support" secondary label is currently a hardcoded English
  literal in the sidebar, a direct REQ-057 breach (user-visible text not through `t()`). Other
  secondary labels ("Settings") and the "No results" string use a copy/template helper, not the ICU
  catalogue. These are the concrete parity defects to fix.
- **Out of scope, proposal only:** a brand-new outbound transport (ticket backend, mail worker,
  hosted help brand) changes behaviour and needs a new REQ id, an ADR and the owner's approval
  before any code (AGENTS.md). No upstream wording, address or asset is imported for it.
- **In scope now:** make the existing support/contact entry points completely i18n- and RTL-correct
  so any future contact form lands without retrofit — REQ-057's "declared support must be real".
  No wire, data-format or permission change; therefore no DEVIATION.

## 3. i18n changes (REQ-057; `apps/web/lib/i18n.ts`)

REQ-057 is in `spec/01-requirements.md` ("user-visible text goes through message catalogues; 14
locales ship; RTL for Hebrew and Arabic-class locales, including in e-mail"; rationale "declared
support must be real"). `SUPPORTED_LOCALES` already lists 14 locales and is the count to pin.

- Replace the hardcoded sidebar "Support" label with a catalogue key under `shell.nav.support`
  (and a `shell.nav.support.hint`/`shell.contact.*` set only if a future surface needs it); reuse
  existing keys first, never invent parallel copy.
- Add any new keys to **all 14** `CATALOGUES`: full translations in `en`/`fr`/`he`, core label +
  action strings in the other eleven with the documented English fallback for the long tail.
  Keep the key set identical across catalogues and keep ICU placeholders such as `{name}` inside
  `t()`.
- No hardcoded user-visible text in the support entry or any contact dialog (title, field labels,
  placeholder, send/cancel, success/failure). REQ-057 also covers e-mail: a future support reply
  must render from the shared ICU catalogue on the Rust side, never from hand-built HTML.
- Extend the existing count/fallback assertions in `apps/web/tests/locale.spec.ts` /
  `shell.spec.ts` to cover the new keys — assert `SUPPORTED_LOCALES.length === 14`, every
  catalogue's key set equals `en`'s, and a deliberately missing key falls back to English, never to
  the key or a blank. Do not add a second catalogue test.

## 4. RTL changes (spec/08-ui-ux.md §8.9 + BoardUI direction rules)

- Keep one direction source: the pre-paint `lang`/`dir` on `<html>` and the existing
  `DirectionProvider` in `components/providers/app-providers.tsx` (BCP 47 locale; `he`/`ar` → RTL).
  Do not add a second provider or a CSS-only `dir` switch.
- Audit the support entry and any contact dialog for physical classes. Replace `left/right`,
  `ml/mr`, `pl/pr`, `text-left/right`, `rounded-l/r`, `border-l/r` with `start/end`, `ms/me`,
  `ps/pe`, `text-start/end`, `rounded-s/e`, `border-s/e` so the nav row and send/cancel row mirror
  automatically. Primary action sits at the reading start; chevrons and any drawer motion mirror.
- Keep physical coordinates where meaning is spatial: charts, canvas and the computer stream are
  **not** mirrored. Keep e-mail addresses, URLs, ticket/correlation ids and any run id in explicit
  LTR islands (`<bdi>` / `dir="auto"`).
- BoardUI only: semantic tokens (`text-text-*`, `bg-background-*`, `border-separator-border`),
  composite type utilities, `cx()` from `@/utils/cx`, installed components, icons by reference from
  `@remixicon/react`, `focus-visible:ring-2 focus-visible:ring-border-focus-ring`. Dark mode rides
  `.dark` tokens; no raw palette, no `dark:` literals, no Pro, no second design system.

## 5. One REQ-named Playwright test

Add to `apps/web/tests/a11y.spec.ts` (distinct from the existing REQ-057 shell/locale names):

```ts
test("REQ-057 the in-app support entry resolves from the catalogue and mirrors for Hebrew RTL", async ({ page }) => {
  // GIVEN the shell with locale he (and ar-EG), WHEN the secondary nav renders,
  // THEN html dir=rtl/lang matches the locale, the "Support" label is catalogue text
  //      (never a hardcoded English literal, never a raw key), the send/cancel row
  //      mirrors with logical classes, any e-mail/url/correlation id stays in an LTR
  //      island, focus ring is visible, and axe reports no serious finding.
});
```

Also assert that a locale with a deliberately missing key falls back to English, not the key. Note
`docs/STATE.md`: the `he` axe artefact is a known local nix-chromium issue and is green on CI —
record the run honestly; do not weaken the gate.

## 6. First slice (test-first, small)

1. Write the failing REQ-057 test above with a seeded `he` locale and the secondary nav rendered.
2. Add the support keys to all 14 catalogues and replace the hardcoded "Support" literal; no new
   strings beyond those the test needs (a real contact backend stays a proposal).
3. Apply logical utilities to the support entry and send/cancel row only; keep effect copy and all
   RTL islands; re-run the shell golden.
4. Record the "new contact form/transport" idea in `docs/STATE.md` as a proposal awaiting an REQ id
   + ADR; implement none of it here.
5. Gates: web typecheck, lint, build, Playwright shell/locale/a11y clusters, 360 px and dark mode.
   Update `docs/STATE.md` with CONFIRMED/GUESS marks and `docs/UI.md` for any component edit; add
   no dependency (`docs/DEPENDENCIES.md` untouched). Keep trace and gate budgets strict.

## Guardrails

No upstream code, asset, wording or brand. No Pro. One design system, one theme. No new REQ or
behaviour change without the owner. Every claim marked CONFIRMED only after a command actually ran.
