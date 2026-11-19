# 0030 — contact-us-in × keyboard parity proposal (Boma Bot)

- Brief: `id 30`, area `contact-us-in`, lens `keyboard` (`docs/openbot-swarm/briefs-1000.jsonl`).
- Status: **PROPOSAL only**. No product code, spec, golden, catalogue or dependency changed.
- Provenance: upstream read as **data only** (clone `/tmp/opencode/openbot-parity`, HEAD `02a7493`) in
  `SUPPORT.md`, `apps/auth-api/src/routes/v1/contact.ts`, `src/lib/contact.ts`,
  `src/components/contact/ContactPage.tsx`, `src/server/contact-message.ts`. No upstream code, asset,
  wording, address or brand enters Boma Bot; no Pro, no second design system. Claims are CONFIRMED
  from files read this session unless marked GUESS.

## 1. Upstream capability, in my own words (inspiration only)

A "contact us" surface is the product's public, no-sign-in support door: a short form (optional
name/company, required address, fixed topic, bounded message) plus channel links, delivered by a
same-origin POST with per-sender and global rate limits and idle/sending/sent/failed/limited states.
Generic rules (restated, not copied):

- Fields are explicitly labelled and natively operable (inputs, one select, one button): Tab/Shift+Tab
  and Enter/Space suffice, with no custom key handling.
- Invalid submit moves focus to the **first invalid control**; errors attach via
  `aria-invalid`/`aria-describedby` and an alert, replacing a placeholder note so the field does not
  move under focus. Submit disables while sending; a status region announces the result and a
  "send another" control returns to idle. No animation is tied to focus or a keystroke.

Transferable fact: even a low-frequency form needs a complete keyboard path — focus lands on the problem
and state is announced. Mirror risk: in the shell it must not steal existing chords.

## 2. Parity position for Boma Bot

spec/01 has no contact-us REQ (GUESS: only a plugin `contact` field and a support correlation id exist).
Siblings read the area as a "Contact us in …" topic exposing a secondary **Support** nav entry
(`dashboard-sidebar.tsx:533`, a dead `<a href="#">` with a hardcoded label — sibling 0024's fix). No
`/contact` route exists.

This lens defines only the keyboard contract; it adds no transport. We anchor the test name to
**REQ-057**, the shell/UI requirement all shell, a11y and motion tests use, because §8.9 has no
dedicated REQ id (one is an owner proposal). No wire, data or permission change ⇒ no DEVIATION.

## 3. Keyboard-path parity, grounded in spec/08-ui-ux.md

- §intro "**Keyboard first.** Every action in a menu is reachable without a pointer." The Support entry
  is in the secondary nav: a real tabbable link/button with Enter/Space activation, never a dead
  `href="#"` or pointer-only row.
- §8.9 "**Keyboard** — send, new line, stop run, focus composer, next/previous bot, close panel, search,
  and open the shortcut list ... never fire while a text field has focus except send/new-line." Contact
  is reached by **Tab**, adds no chord and leaves the text-field guard intact: typing in the message
  field types, it never fires a shell shortcut.
- §8.9 announcements go through a polite live region; "the transcript itself is not a live region."
  Send result and validation errors go to a named status/alert region once, never via the transcript.
- §8.3 "Waiting on user — the card ... **takes focus** ...". If the surface is a panel/dialog, focus moves
  to its heading or first field on open and returns to the trigger on close; Esc closes it.
- §8.9 density keeps hit targets ≥ 32 px (`.boma-target`, 44 px). §8.10 goldens pin seeded states, not
  click paths — add no click-path golden.

## 4. Focus rings and no stolen shortcuts

- **Focus ring** — every focusable step uses `.boma-focus` plus `focus-visible:ring-2
  focus-visible:ring-border-focus-ring`. `.boma-focus:focus-visible` in `apps/web/styles/boma.css` is a
  two-tone ring from `--color-background-full` and `--color-border-focus-ring`; dark mode rides the
  token. No hand-rolled outline, raw palette or `dark:` literal. A status region enters the Tab order
  only when focus is programmatically moved there.
- **No stolen shortcuts** — no new global `keydown` handler; the `?` overlay keeps only the §8.9 list
  (Kbd hints via the installed `Kbd` component). Native `<a>`/`<button>` and BoardUI controls give
  Enter/Space for free; `<label for>`/`id` plus `aria-invalid`/`aria-describedby` make first-invalid
  focus concrete.

## 5. No animation on high-frequency actions

`tests/motion.spec.ts` states "The frequent actions (keyboard, list navigation) deliberately have no
animation." Keyboard focus, opening the surface and submit feedback are immediate: no ripple, no
`boma-card-in`/`boma-stagger` entrance, no ring transition. The sole permitted motion is the existing
120–180 ms token cross-fade for a card→empty change (§8.9), never on a value being read. Pointer-only
`.boma-target` feedback is behind a pointer media query and drops under `prefers-reduced-motion`;
reuse it, add no new animation.

## 6. One REQ-named Playwright test

Add exactly one test to `apps/web/tests/shell.spec.ts`, name carrying the id per `tools/trace_check.py`:

```ts
test("REQ-057 the Support entry has a keyboard-only path with a token focus ring and no shortcut or animation regression", async ({ page }) => {
  await page.goto("/app/");
  const support = page.getByRole("link", { name: "Support" });
  await expect(support).toBeVisible();
  await expect(support).not.toHaveAttribute("href", "#");
  await support.focus();                       // keyboard reach + visible token ring, immediately
  await expect(support).toBeFocused();
  const ring = await support.evaluate((el) => getComputedStyle(el).boxShadow);
  expect(ring).toContain("var(--color-border-focus-ring)"); // or resolved token value
  const motion = await support.evaluate((el) =>
    getComputedStyle(el).animationName + "|" + getComputedStyle(el).transitionProperty);
  expect(motion).toBe("none|none");            // no animation on focus/open
  await page.keyboard.press("Enter");          // native activation moves focus into the surface
  await expect(page.getByRole("heading", { name: /support|contact/i })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(support).toBeFocused();         // focus restored; shell shortcuts intact
  await expect(page.getByRole("textbox", { name: "Message" })).toBeEnabled();
});
```

Pins tabbability, real destination, token ring, focus management, no animation and no stolen key;
adjust role/name to the adopted surface, keep the REQ name and assertions.

## 7. First slice (test-first, small, reversible)

1. Write the failing REQ-057 test against the current shell.
2. Give Support a real, keyboard-operable action: sibling 0026's localised `/contact` page if adopted,
   or a configured mailto — never `href="#"`. Destination is a GUESS/owner decision; add no backend.
3. Add `.boma-focus` + the ring token to the entry and every surface control; explicit labels,
   `aria-invalid`/`aria-describedby`, first-invalid focus; copy via `t()`, ids in LTR `<bdi>`.
4. Register no global key handler; leave the `?` overlay and §8.9 text-field guard unchanged.
5. Keep keyboard open/focus/submit unanimated (§5); no new animation or component dependency.
6. Gates: typecheck, lint, REQ-057 keyboard + `motion.spec.ts`, axe light/dark/RTL at 360 px; mark
   claims CONFIRMED in `docs/STATE.md`, add no dependency, log no DEVIATION.

## 8. Boundaries

No upstream code, asset, wording, address or brand; no Pro, one design system/theme. No new REQ,
transport or behaviour change without the owner. Keyboard shortcuts and focus changes are never animated.
