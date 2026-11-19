# 0011 — "always allow on" through the mobile lens (parity proposal, Boma Bot)

- Brief: `id 11`, area `always-allow-on`, lens `mobile` (`docs/openbot-swarm/briefs-1000.jsonl`).
- Status: **PROPOSAL only**. No product code, spec, golden, catalogue or dependency changed.
- Provenance: upstream read as **data only** (parity clone HEAD `80ddc4d`), restated in my own words; no upstream code, asset, wording or brand; no Pro, no second design system. CONFIRMED = read from our spec/worktree; GUESS = recommendation needing the owner's decision.

## 1. Upstream capability, in my own words (inspiration only)

The host gives an agent a standing permission tier, plus a separate decision a *person* owns: whether one repeated tool call becomes a standing grant instead of a card each time. Widening resolves cards already parked and stops future ones being raised.

- The wide, persistent grant belongs to the **full surface**; the **compact companion surface** shows immediate Allow/Deny but deliberately withholds the standing grant — a small screen cannot safely authorise a rule it cannot fully display (CONFIRMED, upstream CHANGELOG 1362, dock-chat card whose persistent choice stays in the main window).
- A **partial** request — one the small client holds only in part — offers Deny only; Allow appears only after the whole request is fetched from the host, so nobody approves text they have not read (CONFIRMED, upstream `mobile/.../chat-approval-card.tsx`, `approvalIsPartial`).
- The tier is a settings field on a full page, read with no cache so a reopen picks up another device's or an administrator's change; widening opens a confirmation whose confirm is the destructive choice, narrowing saves at once, a locked tier cannot be narrowed (CONFIRMED, upstream `mobile/.../agent-access-fields.tsx`, `staleTime: 0`).
- Answers are acknowledged on-device, errors stay inline, card and controls carry accessibility labels (CONFIRMED, same files).

Transferable fact: **the standing "always allow" is a full-surface control; the phone gets the immediate decision, never the silent widening.** Parity preserves that boundary and the same RPC contract across surfaces, not the compact UI.

## 2. Boma Bot's parity target, grounded in REQ-056

- **REQ-056 (SHOULD)** — an Expo app covers sign-in, chat, computer viewing, artifacts, groups, spaces and push with secure credential storage, and resyncs a thread from its snapshot after a long background (CONFIRMED, `spec/01-requirements.md:457`; `spec/08-ui-ux.md:3` "same product in shells").
- **Contract, not a new one.** REQ-056 is "Expo/RN against the same RPC contract" (CONFIRMED, `docs/PLAN.md:175` M32). A card is answered by `threads.answer` with `{cardId, answer}`; "which calls need approval" lives in `threads.updatePolicy` / the `approvalRules` namespace (CONFIRMED, `spec/03-api-contract.md:22,46`). Both shells call these exact procedures — no phone-only endpoint.
- **Current state.** This snapshot is before M5: no `apps/mobile/` tree or Expo app; the installable PWA is the M1 step and the Expo shell waits until device pairing is proven (CONFIRMED, `docs/PLAN.md:53`, ADR-016, `spec/TRACE.md:73`). REQ-056 was un-deferred by the owner 2026-11-16 but stays unimplemented and in the trace gap (CONFIRMED, `docs/IMPROVEMENTS.md:161`).
- **No feature invented.** Boma Bot has no permission tiers or "always allow" button; adding one is a DEVIATION needing a new REQ id, ADR and owner approval (CONFIRMED, AGENTS.md:36,63). Parity here is a **surface rule and contract guarantee** for the future Expo client, verified today on the web at a phone viewport (the web app is the contract's renderable source of truth).

**Gap:** nothing yet specifies that the compact/mobile card withholds a future widening control, or that a partial card must not grant what the phone cannot show.

## 3. Mobile parity changes (grounded in our own sources)

1. **Compact surfaces never widen.** At phone width the card offers only immediate Approve / Deny; no auto-approve affordance. A widening control, if ever owner-approved, belongs to a full settings panel (spec/08 §8.1: anything unlisted is not a screen). The compact surface answers the request, it does not rewrite policy.
2. **Never approve unread text (partial cards).** If the effect cannot be fully shown (long command, multi-line change), offer Deny and a plain-language "load full request" action that fetches the complete copy through the same card RPC before Allow is enabled. No silent grant, no colour-only warning (spec/08 §8.9).
3. **One contract, both shells.** Web and Expo send the same `threads.answer` `{cardId, answer}` payload and read policy from the same procedures; only presentation differs. Reopening after background resyncs from its snapshot without duplicating a card (REQ-056; spec/08 §8.3 "Offline").
4. **No-card state stays legible.** For a classified call policy lets run unanswered, the phone shows a named status region rather than an empty slot — §8.3 "state is never ambiguous" (GUESS: wording via `t()`).

## 4. BoardUI tokens where relevant, touch a11y, direction, motion

- **One design system.** Web uses installed free BoardUI tokens only: `bg-background-secondary-default`, `border-separator-border`, `text-text-primary` / `text-text-secondary`, `text-text-error-primary`, `text-foreground-icon-primary`, `focus-visible:ring-2 focus-visible:ring-border-focus-ring`, composite `text-body-medium` / `text-caption-1-semibold`, merged with `cx()`. Expo/RN cannot use Tailwind classes, so per AGENTS.md:31 it reuses the **same semantic token roles and design rules** from a shared theme — never a new palette or second design system. No Pro, no vendored edits.
- **Touch a11y.** Targets ≥44 px via `.boma-target` (WCAG 2.5.8); the card is a named accessible group with a label sentence, each answer a native accessible button (RN: `accessibilityRole` / `accessibilityLabel`), and the outcome in a polite live region — the transcript is never that region (spec/08 §8.9). State is text + icon, never colour alone (WCAG 1.4.1); haptics on answer are optional.
- **RTL.** Locale drives `DirectionProvider` on web and matching RN direction on mobile; logical `ps/pe`, `ms/me`, `start/end`, mirrored horizontal motion; paths, commands, ids and URLs stay LTR in `<bdi>` / `dir="auto"` (§8.9, REQ-057). All copy through `t()`.
- **Motion.** State changes 120–180 ms via tokens; `prefers-reduced-motion` reduces to a fade; no animation on a value being read or on streaming text (§8.9).

## 5. One REQ-named Playwright test

One test in `apps/web/tests/approval-card.spec.ts` (via `seedRpc` + `page.route`), REQ id in the title:

```ts
test("REQ-056 phone-width approval card answers via threads.answer without a widening control", async ({ browser }) => {
  const context = await browser.newContext({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true, colorScheme: "light", reducedMotion: "no-preference" });
  const page = await context.newPage();
  await seedApproval(page); // existing helper; spy on **/rpc/threads/answer fills answers[]
  await page.goto("/app/bot_golden/");
  const card = page.getByRole("region", { name: "Pending decision" });
  await expect(card).toBeVisible();
  await expect(card.getByRole("button", { name: /always allow|auto.approve/i })).toHaveCount(0);
  for (const btn of await card.getByRole("button").all()) expect((await btn.boundingBox())!.height).toBeGreaterThanOrEqual(44);
  await card.getByRole("button", { name: "Deny" }).tap();
  expect(answers).toEqual([{ cardId: "card_approval", answer: "deny" }]); // same RPC contract
  await expect(page.getByRole("status")).toContainText("Denied");
  // Zero critical/serious axe at phone width; repeat with he + RTL and reducedMotion:"reduce".
  await context.close();
});
```

Pins the compact-surface rule, 44 px touch targets, the exact `{cardId, answer}` contract, the polite outcome and a clean axe run at phone width, in RTL and reduced motion; `tap()` proves touch.

## 6. First slice (test-first, small, reversible)

1. Write the failing REQ-056 test above.
2. Enforce it in the web card component: phone width never renders the widening affordance and partial cards gate Allow behind a "load full request" path — copy through `t()`, no new RPC or policy write.
3. Add `.boma-target` sizing and the polite outcome live region if absent; keep tagged status + text/icon state.
4. Gates: typecheck, lint, REQ-056 test, `approval-card`, `a11y`, `golden`, `motion` specs, axe light/dark/RTL; record the rule in `docs/STATE.md` / `docs/UI.md` as a mobile decision.
5. **Out of scope:** building Expo, device pairing, any auto-approve control, transport change, new REQ or ADR — those need owner approval and belong to M32 (GUESS per AGENTS.md:27,63).

## 7. Boundaries

Inspiration only: no upstream code, asset, wording or brand; no Pro component, no second design system, no hand-edited BoardUI skill, no new permission semantics or RPC without the owner. This file is the only artefact written; product code and the upstream clone stayed read-only.
