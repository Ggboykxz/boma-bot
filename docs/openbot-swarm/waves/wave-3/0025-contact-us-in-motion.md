# 0025 — contact-us-in × motion parity proposal (Boma Bot)

- Brief: id 25, area `contact-us-in`, lens `motion` (`docs/openbot-swarm/briefs-1000.jsonl`).
- Status: **PROPOSAL only**. No product code, spec, golden, catalogue or CSS was changed.
- Provenance: upstream read as **data** only. That clone was absent here, so the capability is
  restated from the data-only reading in sibling 0024 and the swarm README — never copied. No
  upstream code, asset, wording or brand enters Boma Bot; no upstream Pro. **CONFIRMED** = read this
  session from `boma.css`, `spec/` or a test; **GUESS** = inferred or inherited from that note.

## 1. Upstream capability, in my own words (GUESS)

The "contact us" surface is the product's in-app route for a user to reach the operator without
leaving the client. A low-key entry point in the shell's secondary/footer area opens a way to send a
message — a support address, a short feedback form, or a link to a hosted help channel — often
carrying the current conversation or run identifier so the operator can locate the context. Seen
through a motion lens, its life is a few discrete one-shot events: the surface appears when the entry
point is chosen, a send action acknowledges an outbound message, and an offline/failed state and a
"we'll reply" acknowledgement are shown. None are frequent, streaming or value-reading events, so
none justifies a repeating or prolonged animation. Restated as generic rules, not a copy:

- A supporting channel, not a primary action: reachable from a secondary nav slot; it never approves
  or triggers an agent side effect.
- Composes a short outbound message plus, where recorded, contact identity and a correlation id
  tying it back to the originating thread or run.
- Appearance/disappearance is one brief entrance/exit; send/success/failure are discrete status
  changes the user must read immediately, so none can be animated while being read.

This is a shell-surface and timing concern, not a permission, transport or data-format concern. A
new outbound transport would change behaviour and need a new REQ id, an ADR and owner approval; none
of that is proposed here.

## 2. Motion parity: what already exists (CONFIRMED)

The strict recipe lives only in `apps/web/styles/boma.css`; `globals.css`/`theme.css` are vendored
BoardUI and never hand-edited.

- `.boma-target`: 44 px hit target, `background-color 150ms ease, color 150ms ease` (boma.css:27-29) —
  the base for a contact entry point or send/cancel control.
- Pointer-only press: `:active { transform: scale(0.98) }` over `transform 100ms
  cubic-bezier(0.23, 1, 0.32, 1)`, inside `@media (hover: hover) and (pointer: fine)` (boma.css:32-38).
  Keyboard activation never sees this transform.
- Card entrance: `.boma-card-in` runs `boma-card-in 220ms cubic-bezier(0.23, 1, 0.32, 1) both`,
  animating `opacity 0→1`, `translateY(8px)→0`, `scale(0.99)→1` (boma.css:46-53). The only entrance a
  transient surface is allowed; no second keyframe set is needed for a contact panel.
- Stagger: `.boma-stagger` delays children 2/3/4 by 40/80/120 ms (boma.css:40-43), for a static first
  render of a small list only, never a live-updating list.
- Reduced motion: `.boma-target`/`:active` drop to `transform: none` keeping the two colour
  transitions; `.boma-card-in` becomes `boma-fade-in 200ms ease-out both` (opacity only) (boma.css:55-67).
  Focus is a static `box-shadow` ring, never animated (boma.css:70-76).
- Cross-cutting: 120–180 ms state changes, no animation on streaming content, respect
  `prefers-reduced-motion`, never animate a value being read (spec/08-ui-ux.md:194-195); RTL mirrors
  rail/arrow/drawer motion while numbers, code and URLs stay LTR (§8.9, :190). This is the contract
  pinned by the existing `REQ-057` test in `motion.spec.ts`.
- Recorded "strict motion" decision (STATE.md, 2026-11-19): 100 ms press, 220 ms card entrance with
  strong ease-out, 40 ms stagger, reduced-motion keeps fade/drops movement; `REQ-057` pins both modes.

The 220 ms entrance sits above the spec's 120–180 ms band but inside the brief's 300 ms ceiling; it
is shipped and stays grandfathered. Any new contact surface must reuse it, not add a duration/curve.

## 3. Strict recipes for the contact surface

1. **Reuse, do not add.** An opened contact panel/dialog uses `.boma-card-in` once (220 ms, strong
   ease-out, opacity + translateY + scale) and nothing else. No new keyframes, tokens or assets.
2. **transform/opacity only.** Entrance/exit move only `transform` and `opacity`. Never animate
   `width`, `height`, `top`, `left`, `margin`, `box-shadow` or `filter`; the two colour transitions on
   `.boma-target` stay grandfathered, with no new animated property.
3. **Short, strong ease-out.** Use `cubic-bezier(0.23, 1, 0.32, 1)`; keep every duration ≤ 300 ms
   (100 ms press, 150 ms colour, 220 ms entrance, 200 ms reduced fade). No bounce, spring or loop.
4. **No keyboard motion.** The press scale is pointer-only; keyboard activation of the entry, send
   or cancel applies state instantly. Focus-visible stays the static ring — no pulse, no stagger, no
   animation on the acknowledgement text.
5. **No high-frequency motion.** Send acknowledgement, "we'll reply" and offline/failed states are
   one-shot: set them immediately, never animate streaming or a value being read. Do not attach
   `.boma-stagger` to a live list that ticks.
6. **Reduced motion and RTL.** Under `prefers-reduced-motion: reduce`, fall back to `boma-fade-in`
   (opacity only, ~200 ms ease-out) and `transform: none`, mirroring the shipped card fallback;
   keyboard path unchanged. RTL (`he`/`ar`) mirrors the entrance/drawer direction per §8.9 while
   e-mail, URLs and any correlation/run id stay in `<bdi>` / `dir="auto"` LTR islands. Dark mode rides
   `.dark` tokens; BoardUI semantic tokens, composite type utilities, `cx()`, icons by reference —
   no raw palette, no `dark:` literals, no Pro.

## 4. One REQ-named Playwright test

Add to `apps/web/tests/motion.spec.ts` (alongside the existing REQ-057 card-fade test, not replacing
it), using `seedRpc` from `./fixtures`:

```ts
test("REQ-057 the in-app contact surface entrance uses transform/opacity only and reduced motion drops movement", async ({ page }) => {
  // GIVEN the shell with a seeded secondary "Support" entry, WHEN it is opened,
  // THEN the transient surface carries ".boma-card-in" (animationName "boma-card-in"),
  //   animates only transform/opacity, ends within 300ms with a strong ease-out, and no
  //   width/height/top/left/box-shadow is computed as animated.
  // Keyboard activation of the entry and of send/cancel applies state with no transform animation.
  // emulateMedia({ reducedMotion: "reduce" }): any entrance reports "boma-fade-in",
  //   computed transform is "none", and the focus ring stays a static box-shadow.
});
```

The REQ id sits in the test name (`trace_check` counts declaration lines, not comments). This test is
motion-specific and complements, not duplicates, `motion.spec.ts` (existing REQ-057 card fade),
`a11y.spec.ts` (REQ-057 shell/locale) and sibling 0024 i18n/RTL coverage. Only if a real transport is
later approved with its own REQ id would a separate motion assertion be added.

## 5. First slice (test-first, smallest safe step)

1. Record this note in `docs/STATE.md` → "Decisions needed" with a GUESS mark; await maintainer
   go-ahead before touching product code. *(This file is that proposal; nothing beyond it is written.)*
2. Once approved: write the REQ-057 test above first, red, with the secondary entry seeded open.
3. If a contact surface exists, apply `.boma-card-in` to its single entrance and `.boma-target` to
   send/cancel; reuse tokens and existing catalogue copy (no hardcoded strings). No new keyframes,
   tokens, assets, dependency or Pro component.
4. Leave any new contact form/transport as an unimplemented proposal needing its own REQ id + ADR.
   Do not change the approval gate, wire format or permission model here.
5. Make it green, then run web typecheck, lint, build, the `motion`/`a11y`/`locale` Playwright
   clusters, 360 px, light/dark, keyboard-only path and both motion preferences. Update
   `docs/STATE.md` (CONFIRMED/GUESS marks) and `docs/UI.md` for any component edit; leave
   `docs/DEPENDENCIES.md` and `docs/TRACEABILITY.md` budgets unchanged.

No upstream code, asset, wording or brand; no upstream Pro; one design system, one theme.
