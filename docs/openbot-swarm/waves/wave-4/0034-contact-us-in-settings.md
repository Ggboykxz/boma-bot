# 0034 — contact-us-in × settings parity proposal (Boma Bot)

- Brief: id 34, area `contact-us-in`, lens `settings` (`docs/openbot-swarm/briefs-1000.jsonl`).
- Status: **PROPOSAL only**. Product code, spec, goldens and RPC surface are untouched.
- Provenance: upstream consulted as **data only** at HEAD `559d5ac` this session (`packages/contracts/src/hosted-servers.ts`, `packages/ui/src/features/servers/AddServerDialog.tsx`, `packages/ui/src/features/billing/LicenseSection.tsx`, `src/renderer/src/WorkspaceOverlays.tsx`, `src/renderer/src/features/web-client/WebWorkspace.tsx`, `CHANGELOG.md`). No upstream code, asset, wording, address or brand is carried over; no Pro component; one design system.
- **CONFIRMED** = read this session; **GUESS** = recommendation needing the owner's call.

## 1. Upstream capability, in my own words

- A hosted-workspace app shows a low-key footer in its add-server dialog and in the license purchase flow: when the self-serve paths cannot fit the need (a team larger than the largest license tier, or a request for dedicated infrastructure), a secondary call to action asks whether the person needs something custom, and routes to the vendor's inbox.
- Activating it does not send anything. It opens the person's own mail client with a fixed subject line and a short fixed message body containing a placeholder for extra context; the web client assigns a `mailto:` URL, the desktop shell asks the OS to open the external mail target over its IPC path. No data leaves before the human presses send; there is no app-side transport, ticket object, or API call involved.
- The same switch appears where the self-serve offer tops out (seat slider above the maximum turns the confirm action into the contact action), and the marketing site later gained a contact form whose messages reach the team by email.
- Restated generically: a deployment-configured, person-initiated outbound draft offered next to purchase/admin surfaces when self-serve ends. Not an agent action, not a notification, not an entitlement, and never gated by anything but the deployment having a contact target configured.

## 2. Settings parity, grounded in REQ-058 and REQ-017 policy surfaces

- **REQ-058 (SHOULD)** — administration surfaces ("access grants and deployment settings; none of it is reachable by ordinary members"; spec/01:473-481). A contact target is a *deployment setting*, the same class as the deferred `deployment.settings.get|update` (spec/03:109; DEC-18, ADR-024, docs/STATE.md M9). Its settings home is therefore (a) one read-only row in the settings modal's General page, visible only to `owner`/`admin` of the space and only when the operator configured a target, and (b) later, a read-only section on `/admin/access`. Plain members get nothing — no dead affordance, and any admin procedure remains refused server-side (REQ-058 first bullet).
- **REQ-017 (MUST)** — approvals gate side effects; effects are idempotent (spec/01:137-145). The affordance performs **no effect**: it yields a local `mailto:` draft. If an agent ever proposes "reach the operator" on a member's behalf, that is a write-class effect and rides the existing gateway — pending card, member approval, exactly-once effect key — never this row, and it mints no new approval class. The per-space/per-user policy surfaces we already own (`notifications.policyGet/Set`, `mail.policyGet/Set` with `hold` the bot default; crates/api/src/rpc.rs) are untouched and this row never weakens them.
- Policy-surface read (as in 0014): display before write, scope stated (deployment operator contact; human-initiated draft), reversibility stated (operator edits deployment config; there is no per-user state to revoke), no card created in any state, no "superseded by policy" case because nothing is executed.

## 3. Where Boma Bot stands (CONFIRMED this session)

- Settings modal (`apps/web/components/application/settings/settings-modal.tsx`) pages `general|profile|storage|tools|marketplace`; "Billing", "Appearance", "Rules and Workflows" render as static, non-navigable rows. General page (`settings-general.tsx`) stacks plan/limits/PR/notifications `SettingsCard`s from `settings-rows.tsx`.
- The dashboard sidebar has a secondary `Support` icon item (`RiCustomerServiceLine`) with **no destination** (`dashboard-sidebar.tsx:533-535`) — a hook, not a channel. No contact/support RPC exists; `/api/auth/capabilities` advertises auth/billing/`localFirst` only (spec/openapi.yaml:81-106); `deployment.settings.*` is specced but not wired.
- Admin surfaces `apps/web/app/admin/{access,plugins}` enforce REQ-058 (members/invitations/grants; member refusal; plugin decide), tested in `apps/web/tests/admin.spec.ts`.

## 4. BoardUI surface (semantic tokens only)

- In `settings-general.tsx`, add one `SettingsCard` + `SettingsRow` (label from our own `t()` keys `settings.contact.label/description/action`; description states scope + that nothing is sent until the person sends the mail). Control: `<a>` styled by BoardUI `Button variant="secondary" size="small"`, `href` = validated target, subject/body from our own catalogue template, `<bdi dir="auto">` for the address. Optional later icon `RiCustomerServiceLine` as component reference, `text-foreground-icon-secondary`.
- Chrome tokens: `bg-background-secondary-default`, `border-separator-border`, `text-text-primary/secondary/tertiary`, composites `text-body-regular`/`text-body-2-regular`; merge with `cx()`; focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring` / `.boma-focus`; hit target ≥44 px (`.boma-target`). Logical layout (`ms/me`, `ps/pe`, `text-start`), same `DirectionProvider`; mirrors under RTL; addresses stay LTR/`bdi`. Dark mode rides tokens; no palette/hex, no `dark:` literals, no new dependency, no Pro.
- Rendered only when the server layout passes a validated `contactUrl` prop (GUESS: `BOMA_CONTACT_URL` env, `mailto:` or `https:` only, length-capped, else ignored) **and** viewer role ∈ {owner, admin}; otherwise absent. Capabilities stay unchanged in this slice (wire change ⇒ REQ id + ADR + PARITY/STATE entries).

## 5. One REQ-named Playwright test

In `apps/web/tests/admin.spec.ts` (CONFIRMED: exists with REQ-058 cases; no settings spec exists):

```ts
test("REQ-058 the deployment contact row is admin-only and performs no effect until the person sends (REQ-017)", async ({ page }) => {
  // member at /admin/access: forbidden phase, no contact row anywhere
  for (const p of ["members.list", "invitations.list", "grants.list"])
    await page.route(`**/rpc/admin/${p}`, (r) => r.fulfill({ status: 403, contentType: "application/json", body: '{"code":"FORBIDDEN"}' }));
  await page.goto("/admin/access");
  await expect(page.getByRole("link", { name: t("en", "settings.contact.action") })).toHaveCount(0);
  // admin: row renders with the configured target; clicking performs no RPC write (REQ-017: no effect)
  await page.unrouteAll();
  let rpcWrites = 0;
  page.on("request", (r) => { if (r.method() !== "GET" && /\/rpc\//.test(r.url())) rpcWrites += 1; });
  // seed role=owner + BOMA_CONTACT_URL via the test server fixture; open Settings → General
  const row = page.getByRole("link", { name: t("en", "settings.contact.action") });
  await expect(row).toHaveAttribute("href", /^mailto:/);
  await row.click();
  expect(rpcWrites).toBe(0);
});
```

Also covers RTL (`localStorage["boma-locale"]="he"` → `dir="rtl"`, mirrored row, clean axe) per REQ-057/0004 discipline. State, never a click path, is what goldens pin.

## 6. First slice (test-first, small, reversible)

1. Write the failing REQ-058 test; extend the admin fixture with a seeded `owner` session and the env target.
2. Add `settings.contact.*` keys to `en`/`fr`/`he` in `apps/web/lib/i18n.ts` (English fallback; REQ-057); no other strings.
3. Server layout validates `BOMA_CONTACT_URL`, passes `contactUrl` + viewer role to `SettingsModal` → `SettingsGeneral`; render the single `SettingsCard` row for admins only.
4. Docs: record the env field and the admin `/admin/access` section as proposals in `docs/STATE.md`; note `deployment.settings.*` stays DEC-18; update `docs/UI.md`.
5. Gates: web typecheck/lint/build; Playwright admin/a11y/locale clusters (light, dark, RTL, 360 px); `docs/DEPENDENCIES.md` untouched.

## 7. Non-goals

No upstream wording/address/brand; no Pro; no ticket backend, mail transport, or auto-send; no agent "contact operator" tool; no change to REQ-017 gateway, `mail.policySet` semantics, or approval classes; no `/api/auth/capabilities` or `deployment.settings.*` field in this slice; no discount/seat-count upsell logic, analytics, or cross-device dismissal state.

## Evidence

CONFIRMED — files/sections cited per claim; upstream read as data at `559d5ac` only. GUESS — `BOMA_CONTACT_URL` name and validation; contact-summary sentence in the row; the owner's call on whether members ever see a deployment-published alias. Proposal only; nothing executable ships from this document.
