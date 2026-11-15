import { expect, test } from "./fixtures";
import { t } from "../lib/i18n";

/**
 * The invitation page (spec/08-ui-ux.md §8.2, REQ-042).
 *
 * The token is the capability: the anonymous preview shows only what the link authorizes, the
 * accept action goes to the RPC for the signed-in invitee, and a stale or unknown token reads
 * back the same not-available sentence either way.
 */

async function seedPreview(
  page: import("@playwright/test").Page,
  token: string,
  found: boolean,
) {
  await page.route(`**/api/v1/invites/${token}`, async (route) => {
    if (!found) {
      await route.fulfill({
        status: 404,
        contentType: "application/json",
        body: JSON.stringify({ error: "Not found." }),
      });
      return;
    }
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        spaceName: "Golden space",
        inviterName: "Person One",
        role: "member",
        expiresAt: "2027-01-01T00:00:00Z",
      }),
    });
  });
}

test("REQ-042 the invite page previews a live token without leaking anything else", async ({ page }) => {
  await seedPreview(page, "golden-invite-token", true);

  await page.goto("/invite/?token=golden-invite-token");
  await expect(page.getByText("Golden space")).toBeVisible();
  await expect(page.getByText(t("en", "invite.inviter"))).toBeVisible();
  await expect(page.getByRole("button", { name: t("en", "invite.accept") })).toBeEnabled();
});

test("REQ-042 the invite page renders the same not-available sentence for a dead token", async ({ page }) => {
  await seedPreview(page, "dead-invite-token", false);

  await page.goto("/invite/?token=dead-invite-token");
  await expect(page.getByText(t("en", "invite.unavailable"))).toBeVisible();
  await expect(page.getByRole("button", { name: t("en", "invite.accept") })).toHaveCount(0);
});

test("REQ-042 accepting the invite from /invite/<token> posts the token to invitations.accept", async ({ page }) => {
  let posted: { token?: string } | null = null;
  await seedPreview(page, "golden-invite-token", true);
  await page.route("**/rpc/invitations/accept", async (route) => {
    posted = route.request().postDataJSON();
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ spaceId: "spc_golden" }),
    });
  });

  await page.goto("/invite/?token=golden-invite-token");
  await page.getByRole("button", { name: t("en", "invite.accept") }).click();
  await expect.poll(() => posted).toMatchObject({ token: "golden-invite-token" });
});
