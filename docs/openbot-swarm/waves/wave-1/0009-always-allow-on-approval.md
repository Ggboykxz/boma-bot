# 0009 — always-allow-on × approval gate (parity proposal, Boma Bot)

- Brief: id 9, area `always-allow-on`, lens `approval` (`briefs-1000.jsonl`).
- Status: **PROPOSAL only** — no product code, spec, golden or catalogue was changed.
- Provenance: upstream consulted as **data** (HEAD `386085ff`, 2026-11-18) in `CHANGELOG.md`, `docs/architecture/structure.md`, `docs/architecture/computer-use.md`, `docs/architecture/agents.md`. No upstream code, asset, wording, icon, colour, font or brand enters Boma Bot; no Pro component, no second design system. Claims are CONFIRMED from files read this session unless marked GUESS.

## 1. Upstream capability, in my own words (inspiration only)

Some agent hosts let a person turn a one-off approval into a **standing rule** for a class of tool call: once set, later calls in that class raise no prompt and the action proceeds. The mechanics, restated generically:

- The escalation offer appears only in the middle permission tier. The most-open tier never shows a card (the host answers its own prompts); the most-closed tier keeps prompting.
- Where persisted, the rule is scoped to a tool and provider account, held in a user-owned config file, and owned by the person — the host stores no consent itself.
- Enabling the rule (or an admin raising access) also closes approval prompts **already waiting** for that agent, instead of leaving each one open.
- A less-trusted import downgrades an existing rule to a warning so a safer default wins; changing a rule also discards any cached provider session still carrying the old one.

**Permissive defaults stay out.** We do not adopt: the host silently answering approvals so no card reaches a client; automatic promotion to a fully-open tier from one card click; "never ask" as any default; an import silently keeping an elevated grant; or the host storing/sharing consent on a person's behalf. In Boma Bot every effect is gated by default and always-allow is an explicit, named, reversible policy choice.

## 2. Approval-gate parity, grounded in REQ-017

`spec/01-requirements.md` REQ-017 (MUST): "A policy per space and per user decides which tool calls need approval; an approval executes its effect once even if retried." Always-allow-on is therefore **one value of a policy we already own** — the value for which a classified call creates no pending card and its effect runs once under the recorded effect key. We invent no permission vocabulary and copy no upstream tier names. The approval lens fixes these invariants:

1. **No card is created** under always-allow — a deliberate policy state, not a missing card or a silent failure (pairs with the 0002 empty-state lens).
2. **Exactly-once still holds** — permissive never means double-send/double-charge (REQ-017 second bullet, REQ-018); the effect key is unchanged.
3. **No fabricated consent.** If the policy flips on while a card waits, it closes as an attributable *"superseded by policy"* terminal state — never retroactively minted as the member's "Approved". This is our guardrailed counterpart to the upstream "close already-waiting prompts" behaviour: same lifecycle, honest attribution.
4. **Scope and reversibility** — per space and per user (REQ-017), ideally per tool class; turning it off restores the gate and a change takes effect on the next classification with no stale gate.
5. **Auditability** — the outcome states whether a member or a policy resolved the card.

No new externally visible API/data behaviour is proposed here, so no `DEVIATION` entry.

## 3. The current contextual CardBlock and the approval-lens gap

`apps/web/components/thread/message-blocks.tsx` `CardBlock` is the gate's only client surface: a `<section role="region">` named by `thread.card.region`, an explain line, an effect sentence (`thread.card.effect.approve`), a reversal sentence (`thread.card.reversal`), a waiting `role="status"` line, `Approve`/`Deny` via BoardUI `Button` through `threadsAnswer` with a `CONFLICT` guard, and a transient "Approved"/"Denied" label — the copy/presentation-only slice shipped by swarm `2026-11-18-001` (Queen decision A, no wire field).

Approval-lens gap: the card never says **why the gate is active** (which policy required it), so an operator cannot tell a mandatory gate from one they could change; bare "Approved" reads like a standing grant though it is a one-off decision; and there is no terminal state for "card closed by a policy change". `spec/08-ui-ux.md` §8.3 ("the card is the state") and §8.9 ("Approvals are modal and blocking for the effect only"; announcements via a polite live region) set the contract.

## 4. BoardUI surface (one design system, semantic tokens only)

In `CardBlock`, add a **read-only, translated policy-disclosure line** under the explain line (above the waiting status), plus a short one-off qualifier on the approve outcome. Reuse the region shell: `bg-background-secondary-default`, `border-separator-border`, `rounded-3xl`, `gap-2`, merged with `cx()` from `@/utils/cx`. Type via `text-body-2-regular` / `text-caption-1-semibold` in `text-text-secondary` / `text-text-tertiary`. One icon from `@remixicon/react` as a component reference (e.g. `RiShieldCheckLine`), `aria-hidden`, `text-foreground-icon-secondary`. Keep BoardUI `Button` for decisions, `.boma-focus` / `focus-visible:ring-2 focus-visible:ring-border-focus-ring` and `.boma-target` (44 px). Copy comes from `t()` (`@/lib/use-translation`); add `thread.card.policy.required`, `thread.card.policy.oneoff`, `thread.card.superseded` to the `en`/`fr`/`he` catalogues in `apps/web/lib/i18n.ts` (English fallback for the long tail, per REQ-057 and the 0004 lens). Keep `role="status"` for the announcement — the transcript is never a live region. No palette literals, no `dark:` literals, no Pro.

## 5. One REQ-named Playwright test

In `apps/web/tests/approval-card.spec.ts`, seed with `seedRpc` and add **exactly one** test, distinct from the existing REQ-007 and REQ-017 cases (which cover naming and the Deny path):

```ts
test("REQ-017 the approval gate states its policy, runs no effect before the decision, and records exactly one attributable answer", async ({ page }) => {
  const calls: unknown[] = [];
  await seedApproval(page); // pending approval card, bot_golden (existing helper)
  await page.route("**/rpc/threads/answer", async (route) => {
    calls.push(route.request().postDataJSON());
    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ ok: true }) });
  });
  await page.goto("/app/bot_golden/");
  const card = page.getByRole("region", { name: "Pending decision" });
  await expect(card).toContainText("required by your space policy"); // disclosure present
  await expect(card).toContainText("for this request only"); // one-off, not a grant
  await expect(page.getByRole("textbox", { name: "Message" })).toBeDisabled(); // effect blocked
  expect(calls).toEqual([]); // no effect before decision
  await card.getByRole("button", { name: "Approve" }).click();
  expect(calls).toEqual([{ cardId: "card_approval", answer: "approve" }]); // exactly once
  await expect(page.getByRole("textbox", { name: "Message" })).toBeEnabled();
  await expect(card).toContainText("Approved");
  await expect(card).not.toContainText("always allow"); // one-off, not a standing grant
  expect(calls).toHaveLength(1); // no replay / second answer
});
```

It pins the approval-lens contract: an attributable gate, no pre-decision effect, exactly-once answer, no standing-grant wording. Extend `spec/golden/ui/REQ-007-pending-card.json` with the disclosure text if the state needs re-pinning (state, never a click path).

## 6. First slice (reversible, test-first, small)

1. Write the failing REQ-017 test above.
2. Add the three `thread.card.policy.*` keys to `en`/`fr`/`he`; no new strings otherwise.
3. In `CardBlock` only, render the read-only disclosure and one-off qualifier; keep effect, reversal, buttons and the `CONFLICT` guard untouched.
4. **Do not** add an always-allow control, policy write RPC, or "supersede waiting cards" here — those change permission semantics and need a new REQ id, an ADR and owner approval (AGENTS.md, 0004 lens). Record them in `docs/STATE.md` as a later slice.
5. Gates: web typecheck, lint, build, Playwright approval-card/golden/axe clusters (light, dark, RTL), 360 px; update `docs/STATE.md` (CONFIRMED/GUESS) and `docs/UI.md`; no dependency change.

## Guardrails

- No upstream code, asset, wording or brand; analysis is inspiration only. No Pro. One design system.
- GUESS (per AGENTS.md): no approval-policy RPC exists today (only `notifications.policyGet/Set` and `mail.policyGet/Set` in `crates/api/src/rpc.rs`). Confirm before any later write slice.
- Every claim is CONFIRMED only after a command actually ran; this document ran no build or test.
