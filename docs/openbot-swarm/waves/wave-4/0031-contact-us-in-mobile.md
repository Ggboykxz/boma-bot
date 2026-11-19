# 0031 — contact-us-in × mobile parity proposal (Boma Bot)

- Brief: `id 31`, area `contact-us-in`, lens `mobile` (`docs/openbot-swarm/briefs-1000.jsonl`).
- Status: **PROPOSAL only** — no product code, spec, golden, catalogue or dependency changed.
- Provenance: upstream read as **data only** (clone `/tmp/opencode/openbot-parity`, HEAD `559d5ac`, verified): `SUPPORT.md`,
  `apps/auth-api/src/routes/_contact/contact.tsx`, `components/contact/ContactPage.tsx`, `server/contact-message.ts`, `lib/contact.ts`,
  `apps/mobile/.../settings/support.tsx`, `.../support-settings-screen.tsx`, `.../support-log-capture.ts`,
  `packages/contracts/src/feedback.ts`. No upstream code, asset, wording or brand enters Boma Bot; no Pro, one design system.
  CONFIRMED = read this session; GUESS = owner decision.

## 1. Upstream capability, in my own words (inspiration only)

Two contact doors, split by surface size:

- **Full (web) surface:** a short public form — optional name/company, a reply address, a fixed topic, a bounded
  message with a live counter — plus outbound channel links and a plain mail fallback. Same-origin POST under
  per-sender and global fixed-window limits; validation before send; first invalid field takes focus; send disables
  in flight; named idle/sending/sent/failed/limited states (CONFIRMED, `ContactPage.tsx`, `contact-message.ts`).
- **Compact (phone) surface:** no form at all. A settings → support screen: rows opening chat, an issue-tracker form
  prefilled with platform/app version/device/network (never conversation content), a social row; plus a local
  warning/error/connection log the person can open, count, save via the OS share sheet, or clear behind a confirm;
  link/save failures surface inline (CONFIRMED, `support-settings-screen.tsx`, `support-log-capture.ts`).

Transferable fact: **the small screen never invents a second write path** — it links out and shares explicitly; anything
persisted stays on-device until the person acts. One contract, never replicated per surface.

## 2. Parity position for Boma Bot, grounded in REQ-056

- **REQ-056 (SHOULD)** — an Expo app covers sign-in, chat, computer viewing, artifacts, groups, spaces and push,
  with secure credential storage; threads resync from snapshot after long background (CONFIRMED, `spec/01-requirements.md:457`).
  Un-deferred 2026-11-16; unbuilt; M32 (`docs/PLAN.md:175`); no test names REQ-056 (CONFIRMED).
- **Contract, not a new one.** M32 is "Expo/RN against the same RPC contract". Contact/support needs no RPC while it
  links out; an adopted form posts to the same endpoint as web, same limits — never phone-only (GUESS: no `/contact`
  route in Boma today; siblings 0021–0030 read the area the same way).
- **Current mobile state.** Before M5: no `apps/mobile/` tree; the installable PWA is ADR-016's M1 step — no manifest
  exists (CONFIRMED absence); ADR-016 promises safe-area insets, no hover-only affordances. `docs/UI.md:141`: the shell
  is fluid at 360 px but no narrow-viewport test exists — the phone viewport is unverified (CONFIRMED).
- `spec/08-ui-ux.md` §8.3 **Mobile**: the rail becomes a drawer; the context panel a bottom sheet with three tabs;
  the composer sticks above the keyboard. §8.1: there is no `/contact` route — the affordance belongs in the shell's
  secondary nav until the owner names one.
- Boma today: the Support entry renders as `<a href="#">` with no handler — a dead control (CONFIRMED,
  `dashboard-sidebar.tsx` NavItem `href = "#"`). Parity starts by making it real at every width.

Gap: nothing says the compact surface may only link out / share explicitly, nor that 360 px keeps the entry usable.

## 3. Compact-surface parity rules (grounded in our own sources)

1. **Link-out, never silent send.** The phone surface opens channels or shares a local log by explicit tap. Diagnostics
   leave the device only by share action; a prefilled bundle carries app version, platform, network-kind and a
   support-log excerpt only — never message bodies, secrets, tokens or vault data (§8.9; QUALITY_BAR).
2. **No capability widening on compact.** The 360 px drawer carries the same Support entry as desktop — tap- and
   keyboard-operable — gaining no form, submit RPC or "sent" state the web lacks. An adopted form is one contract
   everywhere (same-origin POST, same windows, same state vocabulary, first-invalid focus); policy stays full-surface.
3. **One entry, all widths.** Same label via `t()` in the drawer (§8.3) and the desktop sidebar; collapsed mode
   keeps a 44 px target with an accessible name — never hover-only (ADR-016).
4. **State is named, never ambiguous (§8.3).** Link/save failures render inline: one sentence of what happened, one of
   what to do; a correlation id behind "details" where one exists (§8.9). No silent no-op, no fake "sent".
5. **Offline honesty.** Outbound rows fail visibly (inline error naming the channel), never a spinner that lies
   (§8.3 Offline; §4.8 honest degraded state).
6. **RTL and catalogue.** Copy through `t()`; logical `ps/pe`, `ms/me`; URLs/handles stay LTR in `<bdi dir="auto">`;
   `he`/`ar` render mirrored, identical roles (REQ-057).

## 4. BoardUI tokens, touch a11y, motion

- **One system, free tier.** Web at 360 px: semantic tokens only — `bg-background-secondary-default` / `-hover`,
  `border-separator-border`, `text-text-primary` / `-secondary`, `text-foreground-icon-secondary`, composite
  `text-body-medium`, `cx()`; installed `RiCustomerServiceLine` stays. Expo/RN cannot use Tailwind classes, so it
  reuses the same token **roles** from a shared theme — never a second palette or hex; no Pro, no vendored edits.
- **Touch a11y.** `.boma-target` ≥ 44 px on the entry and rows; `.boma-focus` / `focus-visible:ring-2
  focus-visible:ring-border-focus-ring`; native roles with accessible names (RN later: `accessibilityRole`,
  `accessibilityLabel`, ≥ 44×44); state by text + icon, never colour alone (1.4.1); outcomes in a polite live
  region — the transcript never is (§8.9); verified with `tap()`.
- **Motion.** Entry, drawer open, rows are immediate: no stagger, no ripple; press feedback stays pointer-only behind
  `(hover: hover) and (pointer: fine)`; reduced motion drops movement (CONFIRMED, `boma.css`); ≤ 120–180 ms only for
  state-change cross-fades (§8.9). Never animate focus, a value being read, or streamed text.

## 5. One REQ-named Playwright test

Add to `apps/web/tests/shell.spec.ts` (carries REQ-056, named by no test today — narrows the trace gap):

```ts
test("REQ-056 the Support entry at 360 px is a 44 px touch target reaching a named support surface", async ({ browser }) => {
  const ctx = await browser.newContext({ viewport: { width: 360, height: 740 }, hasTouch: true, isMobile: true, reducedMotion: "no-preference" });
  const page = await ctx.newPage();
  await page.goto("/app/");
  await page.getByRole("button", { name: /menu/i }).tap();   // open the drawer (§8.3)
  const support = page.getByRole("link", { name: "Support" });
  await expect(support).toBeVisible();
  await expect(support).not.toHaveAttribute("href", "#");    // real destination, no dead control
  const box = (await support.boundingBox())!;
  expect(box.height).toBeGreaterThanOrEqual(44);             // touch target (QUALITY_BAR)
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(360); // no h-scroll
  await support.tap();
  await expect(page.getByRole("heading", { name: /support|contact/i })).toBeVisible();
  // axe: zero critical/serious at this viewport — repeat with colorScheme "dark" and locale "he" (dir=rtl)
  await ctx.close();
});
```

Pins the 360 px contract, touch size, drawer reach, real destination and no h-scroll; adjust roles to the surface,
keep the REQ-056 name and assertions.

## 6. First slice (test-first, small, reversible)

1. Write the failing REQ-056 test above; run it to red.
2. Give Support a real destination (sibling 0030: link-only page or mailto; owner call), same intercept pattern as
   Settings; never `href="#"`.
3. Token sizing/focus/labels on the entry; named outcome region if the surface has states; copy via `t()`; URLs in `<bdi>`.
4. Drawer conformance at 360 px: one entry, 44 px target, no overflow; add `viewport-fit: cover` + safe-area insets if
   still absent (ADR-016's PWA promise; GUESS, tiny, reversible).
5. Gates: typecheck, lint, REQ-056 + shell + a11y + motion specs; axe light/dark/he at 360 px; record in `docs/STATE.md`;
   no new dependency, endpoint or DEVIATION.
6. Out of scope: Expo/M32, PWA installability, `/contact` transport, diagnostics beyond link metadata, new REQ/ADR.

## 7. Boundaries

Inspiration only: no upstream code, asset, wording, address or brand; no Pro; no second design system; no new
transport or REQ without the owner; touch/keyboard/focus feedback is never animated; this file is the only artefact
written — product code and the upstream clone stayed read-only.
