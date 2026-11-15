import { expect, test } from "./fixtures";
import { t } from "../lib/i18n";

/**
 * The artifact screens (spec/08-ui-ux.md §8.5, REQ-036).
 *
 * The grid renders the latest version of every family; the detail screen renders one artifact
 * with its version chain, never the bytes as markup. Everything is seeded route-wise like the
 * other suites — the network never leaves the loopback routes Playwright seeds.
 */

const ARTIFACT_IMAGE = {
  id: "art_image",
  botId: "bot_golden",
  runId: "run_golden",
  name: "chart.png",
  description: "Weekly rollup",
  declaredMime: "image/png",
  mimeType: "image/png",
  size: 2048,
  hash: "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
  rootArtifactId: "art_image",
  version: 2,
  createdAt: "2026-01-02T00:00:00Z",
};

const ARTIFACT_TEXT = {
  id: "art_text",
  botId: "bot_golden",
  runId: "run_golden",
  name: "notes.txt",
  description: "One line",
  declaredMime: "text/plain",
  mimeType: "text/plain",
  size: 11,
  hash: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  rootArtifactId: "art_text",
  version: 1,
  createdAt: "2026-01-03T00:00:00Z",
};

async function seedGrid(page: import("@playwright/test").Page) {
  await page.route("**/rpc/artifacts/listSpace", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ artifacts: [ARTIFACT_IMAGE, ARTIFACT_TEXT] }),
    });
  });
  await page.route("**/rpc/bots/list", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        bots: [
          {
            id: "bot_golden",
            name: "Boma Bot",
            threadId: "thr_golden",
            archivedAt: null,
            snoozedUntil: null,
            unreadCount: 0,
          },
        ],
      }),
    });
  });
  await page.route("**/rpc/bots/get", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ id: "bot_golden", name: "Boma Bot", description: null, threadId: "thr_golden", model: "anthropic/claude-sonnet", color: null, createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z" }),
    });
  });
  await page.route("**/api/auth/capabilities", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        version: "0.1.0",
        runtime: "sandboxed",
        signup: "open",
        password: true,
        emailCode: false,
        passkey: false,
        sso: [],
        provider: null,
        revision: null,
      }),
    });
  });
}

const PNG_BYTES = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x01]).toString("base64");

async function seedViewer(page: import("@playwright/test").Page) {
  await seedGrid(page);
  await page.route("**/rpc/artifacts/get", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ artifact: ARTIFACT_IMAGE, contentBase64: PNG_BYTES }),
    });
  });
  await page.route("**/rpc/artifacts/listVersions", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        versions: [
          { id: "art_image", version: 2, rootArtifactId: "art_image" },
          { id: "art_image_v1", version: 1, rootArtifactId: "art_image" },
        ],
      }),
    });
  });
}

test("REQ-036 the grid lists both families and search narrows them", async ({ page }) => {
  await seedGrid(page);
  await page.goto("/app/artifacts/");
  await expect(page.getByRole("heading", { name: t("en", "artifacts.title") })).toBeVisible();
  await expect(page.getByText("chart.png")).toBeVisible();
  await expect(page.getByText("notes.txt")).toBeVisible();

  await page.getByRole("searchbox", { name: t("en", "artifacts.search") }).fill("chart");
  await expect(page.getByText("chart.png")).toBeVisible();
  await expect(page.getByText("notes.txt")).toHaveCount(0);
});

test("REQ-036 the grid filters by type and by bot", async ({ page }) => {
  await seedGrid(page);
  await page.goto("/app/artifacts/");

  await page.getByRole("combobox", { name: t("en", "artifacts.filter.type") }).selectOption("image");
  await expect(page.getByText("chart.png")).toBeVisible();
  await expect(page.getByText("notes.txt")).toHaveCount(0);

  await page.getByRole("combobox", { name: t("en", "artifacts.filter.type") }).selectOption("all");
  await page.getByRole("combobox", { name: t("en", "artifacts.filter.bot") }).selectOption("bot_golden");
  await expect(page.getByText("chart.png")).toBeVisible();
});

test("REQ-036 the viewer shows the version chain and the open-in-thread action", async ({ page }) => {
  await seedViewer(page);
  await page.goto("/app/artifacts/artifact_golden/");
  await expect(page.getByRole("heading", { name: "chart.png" })).toBeVisible();
  await expect(page.getByRole("combobox", { name: t("en", "artifacts.detail.version") })).toHaveValue("2");

  // "Open in thread" points at the producing bot's thread.
  await expect(page.getByRole("link", { name: t("en", "artifacts.detail.open-in-thread") })).toHaveAttribute("href", "/app/bot_golden/");

  await expect(page.getByRole("button", { name: t("en", "artifacts.detail.download") })).toBeEnabled();
});
