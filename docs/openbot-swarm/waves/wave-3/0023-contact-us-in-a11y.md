# 0023 — "contact us in" through an accessibility lens (Boma Bot parity proposal)

- Brief: id 23, area `contact-us-in`, lens `a11y` (`briefs-1000.jsonl`).
- Status: **PROPOSAL only** — no product code, spec, golden or catalogue was changed.
- Provenance: the clone named in the brief (`/tmp/opencode/openbot-parity`, HEAD `386085ff`) is
  **absent** this session. Upstream was consulted as **data only** in the analysed tree
  `/home/user/myapp/re-workspace/target/openbot` (HEAD `f1905cd`): `CHANGELOG.md`, `README.md`,
  `prompt.txt`, `docs/` and `app/src/`. No dedicated upstream "Contact us" screen was found; the
  only literal hit is a fixture query `search_issues "Contact Us replies"`
  (`app/tests/parallel-lookups.fixture.ts:36`). The capability below is therefore a **generic
  restatement marked GUESS**; the Boma-side parity contract is CONFIRMED from our own spec/docs.
  No upstream code, asset, wording, icon, colour, font, product name or brand enters Boma Bot; no
  Pro component, no second design system. Language: English (disk convention).

## 1. The upstream capability, in my own words (inspiration only)

The area name points to an **in-product contact / support affordance**: a control in the product's
chrome that lets a person reach the party operating the deployment for help — either an outbound
channel (mail or chat) or a short form that captures a reply address and a message and hands them to
the operator (GUESS — exact upstream surface unconfirmed). Restated generically:

- It is a *real control*, not decoration: it has a name a screen reader can read and a target a
  keyboard can reach; a dead or pointer-only link is a defect.
- It says **who is being contacted and what sending does** before the action — no silent outbound
  message, no surprise channel switch.
- It confirms **receipt** (the request landed, or it did not) and, for support, exposes a
  **correlation id** so a human can find the related run or error.
- It asks for the minimum data needed and keeps a reply address or URL readable, not mangled by
  layout direction.

Accessibility interest: this is a small but **high-consequence outbound action** — a form whose
submit actually sends something. It must be reachable by keyboard, named in plain language,
announced through a live region, kept LTR where the data is LTR, and never signalled by colour alone.

## 2. What Boma Bot actually has (the parity target)

Boma Bot has **no "contact us" REQ and no support channel**; adding a new endpoint, address or UI
flow is a new feature and a DEVIATION requiring owner approval (AGENTS.md). Parity here is
**accessibility parity of the affordance where the product already points to support**, not copying
an upstream page:

- REQ-021 (MUST): failures surface as a run status plus a reason (CONFIRMED, `spec/01-requirements.md:170`).
- `spec/08-ui-ux.md` §8.9 "Error copy": one sentence of what happened, one of what to do; the
  **correlation id is offered behind "details" for support** (CONFIRMED). §8.3 "Failed" is an error
  row with a plain-language cause and a retry action (CONFIRMED).
- §8.7 `/admin/plugins`: a submission queue shows the **submitter contact** (CONFIRMED) — the other
  place a contact value is rendered and must stay readable.
- REQ-057 (SHOULD): user text through catalogues, RTL for Hebrew/Arabic-class locales, fallback to
  English (CONFIRMED, `spec/01-requirements.md:465`).
- `apps/web/tests/a11y.spec.ts` already gates zero critical/serious axe findings across routes,
  light/dark and RTL (CONFIRMED).

So the contact/support affordance is the **"details / for support" disclosure in error copy**: a
control that reveals the correlation id and a way to act on it. We make that existing, in-spec
surface accessible; we do not invent a new support product.

## 3. WCAG 2.2 AA parity changes (grounded in our own sources)

1. **Keyboard and focus.** The disclosure is a real `button` with `aria-expanded`, reachable by Tab
   from the error row and operable with Enter/Space; `focus-visible:ring-2 ring-border-focus-ring`.
   No trap, and focus returns to the trigger when a revealed panel closes. §8.9 "Keyboard", docs/UI.md
   foundation 5. WCAG 2.1.1, 2.4.3, 2.4.11.
2. **Name and role.** The trigger and any revealed panel get names from the i18n catalogue, never a
   placeholder or a bare "Details"; the correlation id sits in `<bdi>` so it reads correctly.
   §8.9 "Error copy", docs/UI.md "Copy conventions". WCAG 1.3.1, 4.1.2.
3. **Polite announcement, not a live transcript.** Revealing the id and any "copied / sent / failed"
   result land in a polite `role="status"`; the transcript itself stays a non-live region
   (§8.9 "Announcements"). WCAG 4.1.3.
4. **No colour-only meaning and a real target.** "for support", copied and failed states are text
   plus icon, never a coloured chip alone (§8.9 run-status vocabulary); targets ≥44 px via
   `.boma-target`; contrast rides the remapped tokens, including `text-secondary` in dark
   (docs/UI.md "Local deviations", §8.10 dev record). WCAG 1.4.1, 1.4.3, 1.4.11, 2.5.8.
5. **RTL.** Locale drives `DirectionProvider`; use logical `ps/pe`, `ms/me`, `start/end`, mirror the
   disclosure chevron and any drawer motion; keep the id, any URL and reply address LTR in
   `<bdi>`/`dir="auto"`; `lang`/`dir` written together by the boot script. §8.9 "RTL", docs/UI.md
   "i18n and RTL". REQ-057.
6. **Reduced motion.** Reveal is 120–180 ms, CSS-first, `prefers-reduced-motion` falls back to a
   static fade; never animate the correlation id the user is reading. §8.9 "Motion", docs/UI.md
   foundation 9.

## 4. Tokens, type and direction (BoardUI only)

- Surfaces/text: `bg-background-secondary-default`, `text-text-primary`, `text-text-secondary`,
  error `text-text-error-primary`; hairline `border-separator-border`; icon
  `text-foreground-icon-primary`. Composite type: `text-body-medium`, `text-body-2-regular`,
  `text-caption-1-semibold`. Merge with `cx()`; focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`.
- Use the installed BoardUI `button`, `tooltip`, `directional-icon` and `scroll-fade` components;
  extend them, never hand-build a lookalike. No Pro, no vendored-source edits, no second system.

## 5. One REQ-named Playwright test (first gate)

Add to `apps/web/tests/a11y.spec.ts` (pattern: the existing REQ-057 axe gate and `motion.spec.ts`),
with the REQ id in the title so `tools/trace_check.py` counts it:

```
test("REQ-021 the support/contact affordance is keyboard-only, announced, RTL-mirrored and motion-safe", ...)
```

Steps: seed a failed run carrying a correlation id; load `/app/bot_golden`; Tab to the "for support"
trigger and assert the token focus ring; press Enter to reveal the id inside a named region and
assert `getByRole("status")` announces the reveal; assert no critical/serious axe finding in light
and dark; set `boma-locale=he` in an init script and re-run to assert `dir="rtl"`, the mirrored
chevron, an LTR `<bdi>` id and a clean axe run; set `reducedMotion: "reduce"` and assert the fade
class, not a slide.

## 6. First slice

Keep it in-spec and small: (a) turn the "details / for support" disclosure into a named,
keyboard-operable button with a polite status; (b) wire tokens, RTL and reduced motion per §§3–4;
(c) add the test above. **No** new RPC, endpoint, address, brand, asset or support feature. Land it
and run `a11y`, `motion`, `locale` and `golden` specs; record CONFIRMED/GUESS in `docs/STATE.md`.
A real submission channel stays a separate, owner-approved slice with its own REQ id and ADR.

## Guardrails

Inspiration only — no upstream code, asset, wording or brand; no Pro; one design system. Proposal
only: nothing was built, no test was run, no product file changed.
