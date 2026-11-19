# 0045 — continue-in-a × motion parity proposal (Boma Bot)

- Brief: `id 45`, area `continue-in-a`, lens `motion` (`docs/openbot-swarm/briefs-1000.jsonl`).
  Status: **PROPOSAL only**; nothing else changed.
- Provenance: upstream read as **data** only, clone `/tmp/opencode/openbot-parity`, HEAD `559d5ac`
  (ContextUsage.tsx, context-usage.css, composer-stack.css, menu-motion.tsx, tokens.css, i18n,
  providers.md, whats-new-releases.json, CHANGELOG.md). No code, wording or brand enters Boma.
  **CONFIRMED** = read this session; **GUESS** = inferred.
- Area reading (**GUESS**): `continue-in-a` is a truncated upstream fragment; the exact upstream
  matches are "continues in a new session/provider session" — the compaction/session-rotation capability
  (no-old-session continue; ~80% fill rotates the provider session; agent, chat, files and workspace stay).
  Restated below as Boma **REQ-069** / M24. Siblings 0041–0060 share this reading.

## 1. Upstream capability, in my own words (inspiration only)

When a conversation nears the window, the harness compacts history (or rotates to a fresh provider
session on the same thread) and tells the person. Seen through a motion lens, its life is a few
one-shot, discrete events, never a stream:

- **Context meter** ring: quiet under half full, tone shifts near the threshold; while compacting it
  shows a fixed low fraction with a slow linear spin (`ui-spin 900ms`; reduced motion: no spin)
  (**CONFIRMED**, context-usage.css). Tone changes are instant state.
- **Popover**: `ui-menu-in`/`out` 150ms `cubic-bezier(0.16, 1, 0.3, 1)`, `scale(0.95)` + opacity;
  reduced motion turns it off (**CONFIRMED**, action-menu.css).
- **Composer notice** (`role="status"`): running (spinner + ~4 Hz estimated percent, `aria-hidden`),
  slower than 2× the estimate (elapsed-time text, no fake percent), completed (holds ~2 s, then a
  220 ms ease-in-out exit, translateY + opacity), failed (persists until dismissed; queued messages
  continue). Reduced motion: 1 ms fade (**CONFIRMED**, ContextUsage.tsx, context-usage.css).
- **Transcript marker** (`role="status"`, `aria-live="polite"`): running/completed/failed, with
  before→after token counts (**CONFIRMED**, ContextUsage.tsx).
- **Rotation**: the harness saves work steps, closes the session, opens a new one on the same
  public thread; the first turn takes a history handoff. Agent, thread, workspace and messages stay
  (**CONFIRMED**, providers.md; CHANGELOG 0.71.0). Looping motion exists only on the compacting
  spinner, paused when the window is hidden (**CONFIRMED**, CHANGELOG).

Transferable fact: **continuation is an announced, one-shot state change — not a flourish**; nothing animates while it is read, nothing ticks at high frequency.

## 2. Parity position for Boma Bot, grounded in REQ-069

- **REQ-069 (SHOULD)** — context is elided before the turn dies, **and the person is told**
  (`spec/01-requirements.md:556`). CONFIRMED.
- **M24 landed server-side**: `compaction.rs`, wired in `worker.rs:314`; on a report the worker
  inserts a System-role message with the raw, unlocalized text
  `"[context compacted: N tool results elided (B bytes), M earlier messages dropped]"`
  (`worker.rs:316-337`). CONFIRMED.
- **Web is not legible yet** (M24's "user-legible" clause open): no meter, notice or marker under
  `apps/web/`; the note renders as an ordinary System text block in a `role="log"` transcript
  (`thread-screen.tsx:457`). The raw string is not in the i18n catalogue (CONFIRMED).
- No test names REQ-069's UI clause; `motion.spec.ts` pins REQ-057 only. M24's UI/motion slice is
  unstarted; Boma has **no spinner convention** to grandfather.

Gap: nothing says how the compacted/continuing event enters, persists and leaves — and the raw note does it with zero discipline.

## 3. Strict recipes, grounded in `boma.css` and §8.9

1. **Entrance once, reuse what exists.** The compaction note/marker enters via `.boma-card-in`
   (220 ms `cubic-bezier(0.23, 1, 0.32, 1)`, opacity + `translateY(8px)` + `scale(0.99)`;
   boma.css:46-53). No second keyframe set, no new token, no new asset, no Pro component.
2. **transform/opacity only.** Never `width`, `height`, `top`, `left`, `margin`, `box-shadow`,
   `filter`. The 150 ms colour transitions on `.boma-target` stay grandfathered.
3. **Strong ease-out, ≤ 300 ms everywhere.** 100 ms press, 150 ms colour, 220 ms entrance, 220 ms
   exit mirror (opacity + one small translate), 200 ms reduced fade — all inside the spec
   (§8.9: 120–180 ms state changes, ≤ 300 ms ceiling) and never a bounce, spring or loop.
4. **Pointer-only feedback, no keyboard motion.** Press scale stays behind `(hover: hover) and
   (pointer: fine)` (boma.css:32-38); keyboard/focus are instant — `.boma-focus` is a static
   `box-shadow` (boma.css:70-76). The marker is non-interactive: no press motion; any dismiss is a
   `.boma-target` button.
5. **No high-frequency motion.** Do not port the ~4 Hz percent re-tick or the 900 ms ring spin
   as-is. A later compacting indicator is a static glyph + text state, never a per-frame value;
   nothing ticks while the person reads; `.boma-stagger` never on a live list; any infinite
   animation (owner decision only) pauses on `document.hidden` and vanishes under reduced motion.
6. **Reduced motion keeps the fade, drops movement.** `boma-fade-in` ~200 ms `ease-out`, opacity
   only, `transform: none` (boma.css:55-68); identical text in both modes.
7. **RTL and reading order.** Entrance mirrors the drawer/rail direction (§8.9:189-190); token
   counts, ids and URLs stay LTR `<bdi>` islands; nothing numeric animates.
8. **Never animate a value being read.** Percent and before→after token counts render at final
   value instantly; the exit runs once, on dismiss/timeout.

## 4. One REQ-named Playwright test

In `apps/web/tests/motion.spec.ts` (beside the REQ-057 test), `seedRpc` from `./fixtures`; the REQ
id sits in the test name (`trace_check` counts declarations):

```ts
test("REQ-069 the compaction note enters once, transform/opacity only, reduced motion keeps the fade", async ({ page }) => {
  // GIVEN the thread seeded with the System-role compaction note the worker writes today,
  // THEN the note row carries .boma-card-in exactly once; computed animationName "boma-card-in",
  //   duration ≤ 300ms, timing cubic-bezier(0.23, 1, 0.32, 1); transcript log and note text never animate.
  // Keyboard: focus shows a static box-shadow ring, no transition/animation on box-shadow.
  // emulateMedia({ reducedMotion: "reduce" }): animationName "boma-fade-in", transform "none".
});
```

Motion-specific; complements `motion.spec.ts` (REQ-057) and any `thread` specs; the marker's copy
and i18n keys are siblings 0041–0044's concern, not this test's.

## 5. First slice (test-first, smallest safe step)

1. Record in `docs/STATE.md` → "Decisions needed" with GUESS marks; await owner. Copy and marker
   shape are owner decisions — no new REQ, no ADR.
2. Write the REQ-069 test first, red, seeding the System note as `worker.rs` writes it.
3. Render the note as a **structured marker row** in the existing message pipeline: `role="status"`
   + `aria-live="polite"` (transcript is already `role="log"`), Remix Icon glyph by reference,
   catalogue copy via `t()` (the raw English bracket string stops reaching the screen),
   `.boma-card-in` on entrance only, semantic tokens, `cx()`. No new transport, meter, spinner or
   keyframes.
4. Green, then gates: web typecheck/lint/build; `motion` + `a11y` + `locale` spec clusters; 360 px;
   light/dark; keyboard-only path; both motion preferences; axe light/dark/`he`. Update
   `docs/STATE.md` (CONFIRMED/GUESS) and `docs/UI.md`; `DEPENDENCIES.md`/`TRACEABILITY.md` unchanged;
   `req_069_*` Rust tests untouched.
5. Out of scope: context meter/ring, live percent notice and its 2 s hold/exit choreography,
   exit-timeout UX, session-rotation transport, M24 server changes, `Compact now`/`New chat` actions.

## 6. Boundaries

Inspiration only: no upstream code, asset, wording, address or brand; no Pro, one design system,
BoardUI semantic tokens, no raw palette classes, dark mode rides the tokens; press/focus feedback is
never animated; this file is the only artefact written — product code, spec and the upstream clone
stayed read-only.
