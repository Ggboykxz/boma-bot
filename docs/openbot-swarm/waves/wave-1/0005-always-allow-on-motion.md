# Brief 0005 — blanket-approval switch, lens: motion

- Area (brief metadata): `always-allow-on`. Lens: motion. Status: proposal only; no product code changed.
- Evidence markers: **CONFIRMED** = read from `apps/web/styles/boma.css` or `spec/` this session;
  **GUESS** = inferred, because this capability is not described in the spec.

## The upstream capability, in our own words

Some agent products expose an owner-gated setting that, once enabled, stops asking for a routine
decision before every side effect. The person holding the higher role flips a single switch and,
from then on, ordinary write actions run without pausing the thread on a "what will happen" card.
The safeguards matter as much as the convenience: only the higher role may enable it, enabling is a
deliberate act rather than a stray click, agents may narrow their own reach but never widen it, and
truly destructive or privileged actions stay behind a separate confirmation even while the switch
is on. Importing an agent from an untrusted file does not inherit the switch. *(CONFIRMED as an
upstream pattern from the read-only clone; no upstream identifier, wording, asset or brand is reused
here.)*

**Spec gap (GUESS):** `spec/01-requirements.md` has no requirement for a blanket-approval switch.
The nearest contracts are REQ-017 (an approval gate creates a pending card and executes an effect
once) and REQ-007 (pending cards never block the chat). Treat this whole note as a proposal for
`docs/STATE.md` → "Decisions needed"; do not implement it without the maintainer's go-ahead. The
motion rules below only describe how it *would* behave if accepted, using contracts we already have.

## Motion parity: what already exists

- `.boma-target`: 44 px hit target, `background-color 150ms ease, color 150ms ease`
  (**CONFIRMED**, boma.css:27-30). This is the switch control's base.
- Pointer-only press: `:active { transform: scale(0.98) }` over `transform 100ms
  cubic-bezier(0.23, 1, 0.32, 1)`, wrapped in `@media (hover: hover) and (pointer: fine)`
  (**CONFIRMED**, boma.css:32-38). Keyboard activation never sees this transform.
- Card entrance: `.boma-card-in` runs `boma-card-in 220ms cubic-bezier(0.23, 1, 0.32, 1) both`
  with `opacity 0→1`, `translateY(8px)→0`, `scale(0.99)→1` (**CONFIRMED**, boma.css:45-53).
  This is the *only* entrance a pending decision card is allowed.
- Stagger: `.boma-stagger` delays children 2/3/4 by 40/80/120 ms (**CONFIRMED**, boma.css:40-43).
- Reduced motion: `.boma-target` and `:active` drop to `transform: none` with only the two
  colour transitions; `.boma-card-in` becomes `boma-fade-in 200ms ease-out both` (opacity only)
  (**CONFIRMED**, boma.css:55-68). Focus is a static `box-shadow` ring, never animated
  (**CONFIRMED**, boma.css:70-76).
- Spec cross-cutting Motion: 120-180 ms for state changes, no animation on streaming content,
  respect `prefers-reduced-motion`, never animate a value the user is reading (**CONFIRMED**,
  `spec/08-ui-ux.md` §8.9). WCAG 2.2 AA, keyboard path, 360 px, light/dark and reduced-motion are
  gates (**CONFIRMED**, `AGENTS.md`, `spec/08-ui-ux.md`).

The existing 220 ms card entrance is slightly above the spec's 120-180 ms band but inside the
brief's 300 ms ceiling; leave it grandfathered. Any *new* motion for this capability targets
160 ms so it sits inside the spec band.

## Strict recipes for "switch on"

1. **Absence is the parity.** With the switch on, a classified tool call produces no pending card,
   so no `.boma-card-in` animation fires for it. Do not invent a confirmation flourish: the thread
   simply continues. Motion parity means the card entrance is *not* triggered, not that a new
   effect is added. (**GUESS** behaviour, anchored to REQ-017's gate.)
2. **Movement = transform/opacity only.** The knob's state flip may use `transform` (and an opacity
   cross-fade if needed). No motion on `width`, `height`, `top`, `left`, `margin`, `box-shadow` or
   `filter`. The `background-color`/`color` transitions already on `.boma-target` stay unchanged and
   are grandfathered; add no new animated property.
3. **Strong ease-out, short.** Use `cubic-bezier(0.23, 1, 0.32, 1)` — the boma.css signature curve —
   at 160 ms for the flip. Every duration stays ≤ 300 ms (100 ms press, 150 ms colour, 160 ms flip,
   220 ms card entrance, 200 ms reduced fade).
4. **No keyboard motion.** Activation by keyboard applies state instantly. The `:active` scale is
   already gated to pointer-only; keep it that way. Focus-visible stays the static ring; no stagger,
   no pulse on the switch, no animation on the "on" text.
5. **No high-frequency motion.** Do not attach motion to streaming tokens, run-status chips, live
   lists or unread counts. The switch is a one-shot discrete event. `.boma-stagger` applies only to
   a static first render of a small list, never to a list that updates on a tick; when the switch is
   on, there is no new card row to stagger anyway.
6. **Reduced motion.** Keep opacity, drop movement: the flip becomes an opacity-only fade using the
   `boma-fade-in` pattern (≈200 ms `ease-out`), `transform: none`, no scale. Keyboard path is
   unchanged. This mirrors the existing card fallback exactly, so reduced-motion users see the same
   state announcement either way.

## One REQ-named Playwright test

In a new `apps/web/tests/always-allow-motion.spec.ts` (using `seedRpc` from `./fixtures`), add:

```ts
test("REQ-017 blanket-approval on suppresses the pending-card entrance motion", async ({ page }) => {
  // seed a classified write call + the approval switch in its on state
  // assert: no ".boma-card-in" element exists and no card region appears
  // assert: toggling the switch by keyboard changes its state with no transform animation
  // emulateMedia({ reducedMotion: "reduce" }):
  //   any fade that does run reports animationName "boma-fade-in" and no translate/scale
});
```

The REQ id lives in the test name (trace_check counts declaration lines, not comments). The test is
motion-specific and complements, rather than replaces, `motion.spec.ts` (REQ-057 reduced-motion
card fade) and `approval-card.spec.ts` (REQ-007/REQ-017 card semantics).

## First slice (test-first, smallest safe step)

1. Record this as a proposal in `docs/STATE.md` → "Decisions needed" with a GUESS marker; wait for
   approval before touching product code. *(This note is that proposal; nothing beyond it is written.)*
2. Once approved: write the test above first, red, with a seeded switch-on state.
3. Add the switch as an installed BoardUI form control, class `.boma-target`, copy from the i18n
   catalogue, role-gated in the UI; no raw palette classes, no hardcoded strings.
4. Thread the state into the existing approval gate: when on, classified calls skip the pending-card
   row; REQ-017 idempotency and REQ-007 concurrency remain untouched. Reuse `.boma-card-in` only for
   the off/"ask first" path. No new keyframes, tokens, assets or Pro components.
5. Make it green, then run the a11y gate (zero critical/serious axe), keyboard-only path, 360 px,
   light/dark and both motion preferences before proposing the next slice.

No upstream code, asset, wording or brand is included; no BoardUI Pro component is used.
