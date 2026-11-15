import { expect, test } from "./fixtures";
import { t } from "../lib/i18n";

/**
 * `/integrations/setup` (spec/08-ui-ux.md §8.7, REQ-034, REQ-048).
 *
 * The landing shows what the space installed. The partial-install report is part of the
 * happy path, not an error state.
 */

test("REQ-048 the landing shows the messaging install bound to its bot", async ({
  page,
}) => {
  await page.route("**/rpc/marketplace/list", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        connectors: [],
        installedPlugins: [],
      }),
    });
  });
  await page.route("**/rpc/connections/list", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ connections: [] }),
    });
  });
  await page.route("**/rpc/messaging/list", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        installations: [
          {
            id: "ins_golden",
            platform: "slack",
            botId: "bot_golden",
            workspaceId: "T1234567",
            workspaceName: "Golden workspace",
            status: "active",
            allowPrivate: false,
            createdAt: "2026-01-01T00:00:00Z",
            updatedAt: "2026-01-01T00:00:00Z",
          },
        ],
      }),
    });
  });

  await page.goto("/integrations/setup?installed=slack");
  await expect(page.getByRole("heading", { name: t("en", "integrations.title") })).toBeVisible();
  await expect(page.getByText(t("en", "integrations.installed-heading"))).toBeVisible();
  await expect(page.getByText("slack · Golden workspace")).toBeVisible();
  await expect(page.getByText("bot_golden")).toBeVisible();
});

test("REQ-034 an installed plugin with skipped parts surfaces the report, not a green check", async ({ page }) => {
  await page.route("**/rpc/marketplace/list", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        connectors: [
          {
            connectorId: "github",
            name: "GitHub",
            description: "Issues and PRs",
            authKind: "none",
            installed: true,
          },
        ],
        installedPlugins: [
          {
            name: "golden-plugin",
            source: "https://git.example/golden.git@v1",
            resolvedSha: "a".repeat(40),
            skillCount: 1,
            mcpServerCount: 0,
            skipped: [{ part: "plugin_card", reason: "part kind unsupported in v1" }],
            createdAt: "2026-01-02T00:00:00Z",
          },
        ],
      }),
    });
  });
  await page.route("**/rpc/connections/list", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        connections: [
          {
            connectorId: "github",
            name: "GitHub",
            description: "Issues and PRs",
            authKind: "none",
            installed: true,
          },
        ],
      }),
    });
  });
  await page.route("**/rpc/messaging/list", async (route) => {
    await route.fulfill({
      status: 404,
      contentType: "application/json",
      body: JSON.stringify({ code: "NOT_FOUND", error: "Not found." }),
    });
  });

  await page.goto("/integrations/setup");
  await expect(page.getByText("golden-plugin")).toBeVisible();
  await expect(page.getByText(/plugin_card/)).toBeVisible();
});
