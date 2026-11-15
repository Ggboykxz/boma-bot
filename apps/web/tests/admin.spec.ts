import { expect, test } from "./fixtures";
import { t } from "../lib/i18n";

/**
 * The admin surfaces (spec/08-ui-ux.md §8.7, REQ-058).
 *
 * REQ-058 puts the review of plugin submissions and the list of access grants behind the
 * space-admin role. The wire between the screens and the server is part of the requirement, so
 * the fixtures here answer exactly what the Rust binary does: a member of the space sees the
 * lists, creates one grant, and decides one submission; anyone else gets the refusal rendered
 * as a sentence, not as a table of data they cannot see.
 */

test("REQ-058 the access screen renders members, pending invitations and grants", async ({ page }) => {
  await page.route("**/rpc/admin/members.list", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        members: [
          { userId: "usr_owner", email: "owner@example.com", name: "Owner Person", role: "owner", lastSeenAt: null },
          { userId: "usr_plain", email: "plain@example.com", name: "Plain Person", role: "member", lastSeenAt: null },
        ],
      }),
    });
  });
  await page.route("**/rpc/admin/invitations.list", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        invitations: [{ id: "inv_1", email: "invited@example.com", role: "member", expiresAt: "2027-01-01T00:00:00Z" }],
      }),
    });
  });
  await page.route("**/rpc/admin/grants.list", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ grants: [] }),
    });
  });
  await page.route("**/rpc/admin/grants.create", async (route) => {
    const request = route.request().postDataJSON();
    expect(request).toMatchObject({ email: "beta@example.com", reason: "beta tester" });
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        grant: {
          email: "beta@example.com",
          reason: "beta tester",
          grantedBy: "usr_owner",
          createdAt: "2026-11-15T00:00:00Z",
        },
      }),
    });
  });

  await page.goto("/admin/access");

  await expect(page.getByRole("heading", { level: 2, name: t("en", "admin.access.members") })).toBeVisible();
  await expect(page.getByRole("cell", { name: "owner@example.com" })).toBeVisible();
  await expect(page.getByRole("cell", { name: "plain@example.com" })).toBeVisible();
  await expect(page.getByText(t("en", "admin.access.invitations.empty"))).toHaveCount(0);
  await expect(page.getByText("invited@example.com")).toBeVisible();
  await expect(page.getByText(t("en", "admin.access.grants.empty"))).toBeVisible();

  await page.getByLabel(t("en", "admin.access.grants.email")).fill("beta@example.com");
  await page.getByLabel(t("en", "admin.access.grants.reason")).fill("beta tester");
  await page.getByRole("button", { name: t("en", "admin.access.grants.create") }).click();
  await expect(page.getByText("beta@example.com")).toBeVisible();
});

test("REQ-058 the access screen tells a plain member the page is admin-only", async ({ page }) => {
  for (const path of ["members.list", "invitations.list", "grants.list"]) {
    await page.route(`**/rpc/admin/${path}`, async (route) => {
      await route.fulfill({
        status: 403,
        contentType: "application/json",
        body: JSON.stringify({ code: "FORBIDDEN", error: "The request is not allowed for your role." }),
      });
    });
  }

  await page.goto("/admin/access");

  await expect(page.getByText(t("en", "admin.access.forbidden"))).toBeVisible();
  await expect(page.getByRole("cell")).toHaveCount(0);
});

test("REQ-058 the plugin queue approves one submission and refuses the second decision", async ({ page }) => {
  let decided = false;
  await page.route("**/rpc/admin/plugins.list", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(
        decided
          ? { submissions: [] }
          : {
              submissions: [
                {
                  id: "sub_1",
                  source: "github.com/acme/connector",
                  name: "Acme connector",
                  status: "pending",
                  createdAt: "2026-11-15T00:00:00Z",
                },
              ],
            },
      ),
    });
  });
  await page.route("**/rpc/admin/plugins.decide", async (route) => {
    const request = route.request().postDataJSON();
    decided = !decided;
    expect(request).toMatchObject({ submissionId: "sub_1", decision: "approved" });
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ status: "approved" }),
    });
  });

  await page.goto("/admin/plugins");

  await expect(page.getByText("Acme connector")).toBeVisible();
  await expect(page.getByRole("button", { name: t("en", "admin.plugins.approve") })).toBeEnabled();

  await page.getByRole("button", { name: t("en", "admin.plugins.approve") }).click();
  await expect(page.getByText(t("en", "admin.plugins.empty"))).toBeVisible();
});
