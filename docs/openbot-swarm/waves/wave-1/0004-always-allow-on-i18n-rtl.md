# 0004 — always-allow-on × i18n/RTL parity proposal (Boma Bot)

- Brief: id 4, area `always-allow-on`, lens `i18n-rtl` (`docs/openbot-swarm/briefs-1000.jsonl`).
- Status: **PROPOSAL only**. No product code, spec, golden or catalogue was changed.
- Provenance: upstream consulted as **data** (HEAD `386085ff`) in `CHANGELOG.md`,
  `docs/architecture/computer-use.md`, `docs/architecture/structure.md`. No upstream code,
  asset, wording or brand enters Boma Bot; upstream licence stays upstream.

## 1. Upstream capability, in my own words

At the middle tier of an agent's permission setting, an approval card offers a one-tap choice that
both answers the current request **and widens the agent's standing permission** so later calls in
the same class stop asking. In other words: a decision about *this one action* is also a decision
about *future actions*. The observed rules, restated generically:

- The escalation choice is offered only at the middle tier; the most-restricted tier keeps its card
  and the least-restricted tier never shows one.
- Where a provider records it, the standing grant is scoped per server, per tool and per provider
  account, and lives in that provider's own configuration — the host app deliberately persists
  nothing of its own for tool consent.
- It is a main-window control; the compact/dock surface answers the single action only.
- Raising an agent's standing permission also resolves cards already waiting for that agent.
- A standing grant can preserve a provider session across turns; changing it invalidates a loaded
  session so the new policy takes effect.

This is a permission-policy feature with real security consequences, not a copy tweak.

## 2. Parity position for Boma Bot

- **Already ours:** approval/question cards (REQ-007, REQ-017) with a named region, an explicit
  "what will happen" sentence, a reversal sentence, Approve/Deny via BoardUI `Button`, and all
  strings already routed through `t()` (`apps/web/components/thread/message-blocks.tsx`).
- **Out of scope, proposal only:** a durable "grant future calls" escalation changes permission
  semantics and needs a new REQ id, an ADR and the owner's approval before any code. Per AGENTS.md
  it is logged as a proposal, not smuggled into parity. No upstream wording is reused for it.
- **In scope now (this lens):** make the existing approval surface completely i18n- and RTL-correct
  so that any future consent-scope copy lands without retrofit — REQ-057's "from day one" rule.
  No wire, data-format or permission behaviour changes; therefore no DEVIATION.

## 3. i18n changes (REQ-057; `apps/web/lib/i18n.ts`)

REQ-057 is defined in `spec/01-requirements.md`; its UI face is `spec/08-ui-ux.md` §8.9. The
catalogue already fixes the two rules: missing string → English (never a key or blank), and
direction derived from the locale, not from the text.

- Reuse `thread.card.region`, `thread.card.effect.approve`, `thread.card.reversal`,
  `thread.card.approve`, `thread.card.deny`, `thread.card.waiting`; do not invent parallel copy.
- If the owner later approves a consent-scope control, add its keys to **all 14** `CATALOGUES`
  under `thread.card.scope.*`: full translations in `en`/`fr`/`he`, and core label + action strings
  in the other eleven with the documented English fallback for the long tail. Preserve ICU
  placeholders such as `{name}` and keep interpolation inside `t()`.
- No hardcoded user-visible text in the card; the card's `role="region"` name stays a catalogue
  string so axe and screen readers agree in every locale.
- REQ-057 also covers e-mail: any future notification about a pending decision must render from
  the shared ICU catalogue on the Rust side, never from hand-built HTML strings.
- Extend the existing `tests/locale.spec.ts` count/fallback assertion to check the approval keys,
  rather than adding a second catalogue test.

## 4. RTL changes (spec/08-ui-ux.md §8.9 + BoardUI direction rules)

- Keep one direction source: the pre-paint `lang`/`dir` on `<html>` and the existing
  `DirectionProvider` in `components/providers/app-providers.tsx` (BCP 47 locale, `he`/`ar` → RTL).
  Do not add a second provider or a CSS-only `dir` switch.
- Audit the card for physical classes. Replace `left/right`, `ml/mr`, `pl/pr`, `text-left/right`,
  `rounded-l/r`, `border-l/r` with `start/end`, `ms/me`, `ps/pe`, `text-start/end`,
  `rounded-s/e`, `border-s/e` so the Approve/Deny row and the question form mirror automatically.
  Primary action sits at the reading start; disclosure chevrons and any drawer motion mirror too.
- Keep physical coordinates where meaning is spatial: the computer screen stream, charts and
  canvas are **not** mirrored. Keep codes, URLs, OTP digits and correlation ids in explicit LTR
  islands (`<bdi>` / `dir="auto"`), including the id behind "details".
- BoardUI only: semantic tokens (`text-text-*`, `bg-background-*`, `border-separator-border`),
  composite type utilities, `cx()` from `@/utils/cx`, installed components, icons by reference
  from `@remixicon/react`, and `focus-visible:ring-2 focus-visible:ring-border-focus-ring`.
  Dark mode rides `.dark` tokens; no raw palette, no `dark:` literals, no Pro, no second design
  system, no hand-edited BoardUI skill.

## 5. One REQ-named Playwright test

Add to `tests/a11y.spec.ts` (distinct from the existing REQ-057 shell/locale/a11y/motion names):

```ts
test("REQ-057 an approval card in a RTL locale mirrors its decisions and resolves every string from the catalogue", async ({ page }) => {
  // GIVEN a seeded thread with one pending approval card, locale he (and ar),
  // WHEN the thread renders,
  // THEN html dir=rtl/lang matches, the region name + effect + reversal + actions
  //      are catalogue text (never a key, never English fallback in a complete locale),
  //      the primary action is on the reading-start side, focus ring is visible,
  //      and the correlation id stays inside an LTR island; axe reports no serious finding.
});
```

Also assert in a locale with a deliberately missing key that the card falls back to English, not
to the key. Note `docs/STATE.md`: the `he` axe artefact is a known local nix-chromium issue and is
green on CI — record the run honestly, do not weaken the gate.

## 6. First slice (test-first, small)

1. Write the failing REQ-057 test above with a seeded pending card.
2. Assert/restore the approval keys in all 14 catalogues; no new strings unless step 4 is approved.
3. Apply logical utilities to the card action row and question form only; keep the effect/reversal
   sentences and all RTL islands; re-run the card golden.
4. Record the standing-grant escalation in `docs/STATE.md` as a proposal awaiting an REQ id + ADR;
   implement nothing of it here.
5. Gates: web typecheck, lint, build, Playwright card/golden/axe clusters, 360 px and dark mode.
   Update `docs/STATE.md` with CONFIRMED/GUESS marks and `docs/UI.md` for any component edit; add
   no dependency (so `docs/DEPENDENCIES.md` is untouched). Keep the trace and gate budgets strict.

## Guardrails

No upstream code, asset, wording or brand. No Pro. One design system, one theme. No new REQ or
permission change without the owner. Every claim marked CONFIRMED only after a command actually ran.
