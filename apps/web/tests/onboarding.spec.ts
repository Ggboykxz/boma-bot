import { expect, test } from "./fixtures";
import { t } from "../lib/i18n";

/**
 * The onboarding wizard (spec/08-ui-ux.md §8.2, REQ-042, REQ-052).
 *
 * Steps 1 and 2 call the creation RPCs; step 3 reaches the models endpoints, and the finish
 * action navigates onward. localStorage resumes the wizard where it left off.
 */

const CATALOG = {
  providers: [
    {
      id: "anthropic",
      name: "Anthropic",
      models: [{ id: "claude-sonnet", name: "Claude Sonnet", contextWindow: 200_000, roles: ["chat"] }],
    },
  ],
};

test("REQ-042 step 1 creates the space and advances to the bot step", async ({ page }) => {
  let posted: { name?: string } | null = null;
  await page.route("**/rpc/spaces/create", async (route) => {
    posted = route.request().postDataJSON();
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ id: "spc_new", name: "Research lab", isDefault: false, role: "owner" }),
    });
  });

  await page.goto("/onboarding/1/");
  await page.getByPlaceholder(t("en", "onboarding.step1.space-placeholder")).fill("Research lab");
  await page.getByRole("button", { name: t("en", "onboarding.step1.submit") }).click();
  await page.waitForURL("**/onboarding/2/", { timeout: 10_000 });
  expect(posted).toMatchObject({ name: "Research lab" });
});

test("REQ-042 step 2 creates the first bot and advances to the model step", async ({ page }) => {
  await page.addInitScript(() => {
    window.localStorage.setItem("boma-onboarding", JSON.stringify({ version: 1, spaceId: "spc_new" }));
  });
  let posted: { name?: string; description?: string } | null = null;
  await page.route("**/rpc/bots/create", async (route) => {
    posted = route.request().postDataJSON();
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ id: "bot_new", name: "Analyst", threadId: "thr_new", archivedAt: null, createdAt: "2026-01-01T00:00:00Z" }),
    });
  });

  await page.goto("/onboarding/2/");
  await page.getByPlaceholder(t("en", "onboarding.step2.bot-placeholder")).fill("Analyst");
  await page.getByPlaceholder(t("en", "onboarding.step2.bot-purpose-placeholder")).fill("Watches the inbox");
  await page.getByRole("button", { name: t("en", "onboarding.step2.submit") }).click();
  await page.waitForURL("**/onboarding/3/", { timeout: 10_000 });
  expect(posted).toMatchObject({ name: "Analyst", description: "Watches the inbox" });
});

test("REQ-052 step 3 blocks finish until a provider is connected", async ({ page }) => {
  await page.addInitScript(() => {
    window.localStorage.setItem("boma-onboarding", JSON.stringify({ version: 1, spaceId: "spc_new", botId: "bot_new" }));
  });
  await page.route("**/rpc/models/catalog", async (route) => {
    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(CATALOG) });
  });
  let credentialsCall = 0;
  await page.route("**/rpc/models/credentials", async (route) => {
    credentialsCall += 1;
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(
        credentialsCall === 1 ? { credentials: [] } : { credentials: [{ provider: "anthropic", status: "connected", createdAt: "2026-01-01T00:00:00Z" }] },
      ),
    });
  });
  await page.route("**/rpc/models/connect", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ provider: "anthropic", connected: true }),
    });
  });

  await page.goto("/onboarding/3/");
  const finish = page.getByRole("button", { name: t("en", "onboarding.step3.finish") });
  await expect(finish).toBeDisabled();

  await page.getByPlaceholder("<provider key>").fill("sk-test-key");
  await page.getByRole("button", { name: t("en", "onboarding.step3.connect") }).click();
  await expect(finish).toBeEnabled();
});
