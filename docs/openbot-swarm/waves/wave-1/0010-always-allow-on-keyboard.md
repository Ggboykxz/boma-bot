# 0010 — always-allow-on × keyboard lens (parity proposal, Boma Bot)

- Brief: `id 10`, area `always-allow-on`, lens `keyboard` (`docs/openbot-swarm/briefs-1000.jsonl`).
- Status: **PROPOSAL only**. No product code, spec, golden, catalogue or dependency changed.
- Provenance: upstream consulted as **data only** (parity clone snapshot), restated in my own words;
  no upstream code/asset/wording/brand, no Pro, no second design system. CONFIRMED unless marked GUESS.

## 1. Upstream capability, in my own words (inspiration only)

The analysed host separates an agent's overall permission tier from a distinct, standing "let this
tool class run without asking" decision the *user* owns. Restated generically:

- The standing grant is a named, reversible policy state, not the absence of a prompt. When it is on,
  no decision card is created — there is nothing to answer.
- Where the provider records it, the choice is scoped per server, per tool and per account, kept in
  that provider's own file-backed config; the host stores nothing of its own for consent.
- A host-side automation layer can answer its own approvals so no card reaches a client; a client
  cannot grant one on a remote host's behalf.
- Widening permission resolves waiting cards; importing/downgrading turns prior grants into warnings
  so a safer default wins. The escalation control is full-window only: the compact surface answers
  the immediate request but cannot set the standing policy.

The transferable fact: a policy control absent from a compact surface strands keyboard users. Parity
must keep always-allow **keyboard-operable wherever the card surface exists**, not just pointer-visible.

## 2. What "always allow on" means in Boma Bot

**REQ-017** says "a policy per space and per user decides which tool calls need approval." Always-allow
is one value: a classified call produces no pending card and its effect runs under the recorded
idempotency key. We reuse that contract; no new vocabulary or copied strings. The keyboard lens asks
what the spec leaves implicit: can a keyboard-only user discover the mode, reach its control, change
it and get a visible, non-animated response? This defines that path, not a permission write.

## 3. Keyboard-path parity, grounded in spec/08-ui-ux.md

- §intro "**Keyboard first.** Every action in a menu is reachable without a pointer." The policy
  control must be tabbable and operable with Enter/Space, never pointer-only — the anti-pattern above.
- §8.3 "Waiting on user — the card is the state: it scrolls into view, **takes focus** ...". A modal
  card takes focus today; with no card, on a mode change move focus to the named status region (or
  the composer, per idle) instead of leaving focus on a removed card.
- §8.9 announcements go through a polite live region, "the transcript itself is not a live region."
  Announce a mode change once there; a screen reader hears it without reading every token.
- §8.9 "**Keyboard** — send, new line, stop run, focus composer, next/previous bot, close panel,
  search, and open the shortcut list ... never fire while a text field has focus except send/new-line."
  The toggle is reached by Tab, not a shortcut; it steals no chord, never fires from the composer,
  and the `?` overlay lists only §8.9 shortcuts — no new secret key.
- §8.9 density preserves hit targets ≥ 32 px: keep the toggle/link on `.boma-target` (44 px).
- §8.10 `REQ-007-pending-card.json` pins "takes focus"; add a `REQ-017-always-allow-keyboard.json`
  state for the no-card keyboard state (never a click path).

## 4. Focus rings and shortcuts (one design system)

- **Focus ring** — every focusable step uses `focus-visible:ring-2 focus-visible:ring-border-focus-ring`
  and `.boma-focus`, whose `:focus-visible` in `apps/web/styles/boma.css` is a two-tone ring from
  `--color-background-full` and `--color-border-focus-ring` (`theme.css`). No hand-rolled outline,
  raw palette or `dark:` literal — dark mode rides tokens. The status region is a focus target only
  when focus is programmatically moved there; otherwise keep it out of the Tab order.
- **Shortcut hints** — reuse the installed `Kbd` component (`components/base/kbd/kbd.tsx`), already
  token-based; no second pill style. Approve/Deny stay native buttons, so Enter/Space work for free.
  Always-allow adds **no** global key handler; any handler respects the §8.9 text-field guard.
- **Animate rule — keyboard shortcuts must not be animated.** `tests/motion.spec.ts` already states
  "The frequent actions (keyboard, list navigation) deliberately have no animation." A shortcut's
  visible effect is immediate: no transition, ripple, pulse or entrance animation on focus changes or
  shortcut-driven state changes. The only permitted motion is the 120–180 ms, reduced-motion-respecting
  token cross-fade for the card→empty status change (§8.9), never on a value being actively read.

## 5. One REQ-named Playwright test

Add to `apps/web/tests/approval-card.spec.ts` (via `seedRpc`), exactly one test:

```ts
test("REQ-017 always-allow mode has a keyboard-only path with a visible focus ring and unanimated shortcuts", async ({ page }) => {
  await seedRpc(page, {
    answers: {
      "bots/get": { id: "bot_golden", name: "Boma Bot", threadId: "thr_golden" },
      "threads/messages": { messages: [] },          // no card: always-allow on
      "runs/list": { runs: [{ id: "run_1", status: "running" }] },
    },
  });
  await page.goto("/app/bot_golden/");
  const status = page.getByRole("region", { name: "Pending decisions" });
  await expect(status).toBeVisible();
  await expect(status).toContainText("always allow");
  await expect(status.getByRole("button", { name: "Approve" })).toHaveCount(0);
  await expect(page.getByRole("textbox", { name: "Message" })).toBeEnabled();

  // Keyboard-only: reach the policy control by Tab and see the token focus ring.
  await page.keyboard.press("Tab");
  const toggle = page.getByRole("switch", { name: /always allow/i });
  await expect(toggle).toBeFocused();
  const ring = await toggle.evaluate((el) =>
    getComputedStyle(el).getPropertyValue("--color-border-focus-ring"));
  expect(ring).not.toBe("");

  // Shortcuts/focus stay unanimated: no CSS animation or transition on the effect.
  const motion = await toggle.evaluate((el) =>
    getComputedStyle(el).animationName + "|" + getComputedStyle(el).transitionProperty);
  expect(motion).toBe("none|all");
});
```

It pins the no-card state, a Tab-reachable control, a token focus ring, the enabled composer and no
animation (adjust the switch role/name to the implementation; REQ name and assertions stay).

## 6. First slice (test-first, small, reversible)

1. Write the failing REQ-017 test against a seeded no-card thread.
2. Render a named "Pending decisions" status region for the always-allow value, all copy through
   `t()` (`apps/web/lib/i18n.ts`, English fallback for a missing key).
3. Add the control as a native, tabbable BoardUI `switch` with `.boma-focus` and the focus-ring
   token; no global shortcut, write RPC left unimplemented (read-only default) until the policy
   endpoint is confirmed (GUESS — check `crates/api/src/rpc.rs`).
4. On mode change, move focus to the status region and announce once via the polite live region;
   no animation on any keyboard/shortcut effect (§4).
5. Gates: web typecheck, lint, the REQ-017 test, `motion.spec.ts`, axe in light/dark/RTL. Record
   CONFIRMED/GUESS in `docs/STATE.md`; no DEVIATION, no new dependency.

## 7. Boundaries

- No upstream code, asset, wording or brand; §1 is inspiration only, restated.
- No Pro, no second design system, no hand-edited BoardUI skill; free-tier tokens only.
- No new REQ, permission-semantics change or policy write without the owner — logged as a proposal.
- Strict animate rule: keyboard shortcuts and focus changes are never animated.
