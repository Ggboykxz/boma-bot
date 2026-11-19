# 0014 — always-allow-on × settings (parity proposal, Boma Bot)

- Brief: id 14, area `always-allow-on`, lens `settings` (`briefs-1000.jsonl`).
- Status: **PROPOSAL only** — no product code, spec, golden or catalogue was changed.
- Provenance: upstream consulted as **data** (HEAD `386085ff`, 2026-11-18) in `docs/architecture/structure.md`, `docs/architecture/computer-use.md`, `docs/architecture/agents.md` and `CHANGELOG.md`. No upstream code, asset, wording, icon, colour, font, product name or brand enters Boma Bot; no Pro component, no second design system. Claims are CONFIRMED from files read this session unless marked GUESS.

## 1. Upstream capability, in my own words (inspiration only)

Some agent hosts let a person turn a one-off tool-consent decision into a **standing rule**: once saved, later calls of that class raise no prompt and the action proceeds. Restated generically:

- The offer to make a decision standing appears only in the middle permission tier. The most-open tier never shows a card (the host answers its own prompts); the most-closed tier keeps prompting for every call.
- Where persisted, the rule is scoped per tool and per provider account and lives in a user-owned, file-backed config; the host stores no consent itself — it stays the person's choice. A thread-scoped config alone does not qualify.
- An admin raising access, or a grant change, also closes cards already waiting for that agent rather than leaving each open; changing the rule discards a cached provider session carrying the old one.
- A less-trusted import downgrades an enabled rule to a warning so the safer default wins; the host's own policy stays at a request-based value, never a blanket deny.

**Permissive defaults stay out.** We do not adopt: the host silently answering approvals; automatic promotion to a fully-open tier from one click; "never ask" as any default; an import silently keeping an elevated grant; or the host storing/sharing consent. In Boma Bot a standing rule, if ever added, is explicit, named, scoped, reversible and admin-governed.

## 2. Settings parity, grounded in REQ-058 and the approval-policy surfaces

`spec/01-requirements.md` REQ-058 (SHOULD): "Server operators get diagnostics ... access grants and deployment settings; none of it is reachable by ordinary members." `spec/00-overview.md` names approval policy among the space admin's remit, and REQ-017 (MUST) fixes the policy itself: "A policy per space and per user decides which tool calls need approval; an approval executes its effect once even if retried." A standing always-on approval is therefore **one value of a policy we already own**, not a new permission vocabulary.

The settings lens resolves where that value is visible:

1. **Admin-governed, not per-member.** Because approval policy is a space/admin setting (REQ-058, REQ-000), its settings home is the admin console, not the member settings modal; a plain member must be refused (REQ-058 first bullet).
2. **Display before write.** In this slice the setting is read-only and states scope (space, user, tool class), current value and how to reverse it. No toggle writes a policy.
3. **No card is created** under a standing rule — a deliberate policy state, not a missing card (pairs with the 0002 empty-state lens).
4. **Exactly-once still holds** (REQ-017 second bullet, REQ-018): permissive never means double-send/double-charge; the effect key is unchanged.
5. **Honest attribution.** If a policy flips on while a card waits, it closes as *"superseded by policy"* — never retroactively minted as the member's approval. This is our guardrailed counterpart to closing already-waiting cards.

No new externally visible API/data behaviour is proposed, so no `DEVIATION` entry.

## 3. The current settings surfaces and the settings-lens gap

`apps/web/app/admin/access/page.tsx` + `apps/web/components/admin/access-screen.tsx` are the REQ-058 admin surfaces (members, invitations, grants). `apps/web/components/application/settings/settings-modal.tsx` has pages `general|profile|storage|tools|marketplace`; "Rules and Workflows" is a static, non-navigable nav row. `apps/web/components/thread/notifications-panel.tsx` shows the only settings-style policy editors today (`notifications.policyGet/Set`, `mail.policyGet/Set` in `crates/api/src/rpc.rs`).

Settings-lens gap: approval policy has **no settings surface at all** — a member cannot see which policy governs a call, an admin cannot see scope or reversibility, and a bare "Approved" reads like a standing grant though it is one-off (0009 lens). `spec/08-ui-ux.md` §8.7 (admin surfaces) and §8.3 ("the card is the state") set the contract.

## 4. BoardUI surface (one design system, semantic tokens only)

In `access-screen.tsx`, add a read-only **Approval policy** section using the local `SettingsCard`/`SettingsRow`/`SettingsSectionLabel` chrome from `settings-rows.tsx`; a disabled `Switch` from `@/components/base/switch/switch` shows the current value without writing. Shell: `bg-background-secondary-default`, `border-separator-border`, `rounded-3xl`, `gap-2`, merged with `cx()` from `@/utils/cx`. Type via `text-body-medium` / `text-body-2-regular` / `text-caption-1-semibold` in `text-text-secondary`/`text-text-tertiary`; one `@remixicon/react` icon as a component reference (e.g. `RiShieldCheckLine`), `aria-hidden`, `text-foreground-icon-secondary`. Keep focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring` and the 44 px `.boma-target`. Copy via `t()` (`@/lib/use-translation`); add `admin.approval.scope`, `admin.approval.standing`, `admin.approval.reversible` to `en`/`fr`/`he` in `apps/web/lib/i18n.ts` (English fallback for the long tail, REQ-057/0004 lens). No palette literals, no `dark:` literals, no Pro, no new component library.

## 5. One REQ-named Playwright test

In `apps/web/tests/admin.spec.ts`, add **exactly one** test, distinct from the three existing REQ-058 cases (members/invitations/grants, member refusal, plugin queue):

```ts
test("REQ-058 the approval-policy setting is admin-only and states scope and reversibility", async ({ page }) => {
  await page.route("**/rpc/admin/members.list", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        members: [{ userId: "usr_owner", email: "owner@example.com", name: "Owner", role: "owner", lastSeenAt: null }],
      }),
    });
  });
  await page.goto("/admin/access");
  const section = page.getByRole("region", { name: t("en", "admin.approval.title") });
  await expect(section).toContainText(t("en", "admin.approval.scope")); // scope stated
  await expect(section).toContainText(t("en", "admin.approval.reversible")); // reversal stated
  await expect(section.getByRole("switch")).toBeDisabled(); // read-only, no write
  await page.unrouteAll();
  for (const path of ["members.list", "invitations.list", "grants.list"]) {
    await page.route(`**/rpc/admin/${path}`, async (route) => {
      await route.fulfill({ status: 403, contentType: "application/json", body: JSON.stringify({ code: "FORBIDDEN", error: "The request is not allowed for your role." }) });
    });
  }
  await page.goto("/admin/access");
  await expect(page.getByRole("region", { name: t("en", "admin.approval.title") })).toHaveCount(0); // not reachable
});
```

It pins the settings-lens contract: an admin-only, read-only policy surface with honest scope/reversal copy and no member reach. Keep it presentation-only — no RPC write, no card replay. Extend `spec/golden/ui/REQ-007-pending-card.json` only if a state needs re-pinning (state, never a click path).

## 6. First slice (reversible, test-first, small)

1. Write the failing REQ-058 test above.
2. Add the `admin.approval.*` keys to `en`/`fr`/`he`; no other strings.
3. In `access-screen.tsx` only, render the read-only section; leave grants/invitations and the `forbidden` phase untouched.
4. **Do not** add a write toggle, policy RPC, or "supersede waiting cards" here — those change permission semantics and need a new REQ id, an ADR and owner approval (AGENTS.md, 0004/0009 lenses). Record them in `docs/STATE.md` as a later slice.
5. Gates: web typecheck, lint, build, Playwright admin/approval/golden/axe clusters (light, dark, RTL), 360 px; update `docs/STATE.md` (CONFIRMED/GUESS) and `docs/UI.md`; no dependency change.

## Guardrails

- No upstream code, asset, wording or brand; analysis is inspiration only. No Pro. One design system.
- We use our own label "standing approval", never the upstream's product wording.
- GUESS (per AGENTS.md): no approval-policy RPC exists today (only `notifications.policyGet/Set` and `mail.policyGet/Set` in `crates/api/src/rpc.rs`). Confirm before any later write slice.
- Every claim is CONFIRMED only after a command actually ran; this document ran no build or test.
