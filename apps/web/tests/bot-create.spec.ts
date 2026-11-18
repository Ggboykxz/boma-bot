import { expect, seedRpc, test } from "./fixtures";

/**
 * Creating a bot after onboarding (spec/01 REQ-004, spec/08 §8.3).
 *
 * The rail used to offer a `+` for groups only — a returning owner had no way to add a second
 * teammate. These tests pin the new action, its validation and its offline behaviour.
 */

const newBot = {
  id: "bot_golden",
  name: "Planner",
  threadId: "thr_created",
  archivedAt: null,
  snoozedUntil: null,
  unreadCount: 0,
};

test("REQ-004 a member creates a second bot from the rail", async ({ page }) => {
  let created = 0;
  await seedRpc(page, {
    answers: {
      "bots/create": newBot,
    },
  });
  await page.route("**/rpc/bots/create", async (route) => {
    created += 1;
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(newBot),
    });
  });
  await page.goto("/app/");
  await page.getByRole("button", { name: "New bot" }).click();
  await page.getByRole("textbox", { name: "Name" }).fill("Planner");
  await page.getByRole("textbox", { name: "Description" }).fill("Plans the week");
  await page.getByRole("button", { name: "Create bot" }).click();
  await page.waitForURL("**/app/bot_golden/");
  expect(created).toBe(1);
});

test("REQ-004 a blank bot name is rejected before the RPC call", async ({ page }) => {
  let created = 0;
  await seedRpc(page, { answers: {} });
  await page.route("**/rpc/bots/create", async (route) => {
    created += 1;
    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(newBot) });
  });
  await page.goto("/app/");
  await page.getByRole("button", { name: "New bot" }).click();
  await page.getByRole("textbox", { name: "Name" }).fill("   ");
  await expect(page.getByRole("button", { name: "Create bot" })).toBeDisabled();
  expect(created).toBe(0);
  await expect(page.getByRole("textbox", { name: "Name" })).toBeVisible();
});

test("REQ-004 the new-bot action stays available when the roster cannot answer", async ({ page }) => {
  await seedRpc(page, { answers: { "bots/list": { bots: [] } } });
  await page.route("**/rpc/bots/list", async (route) => {
    await route.fulfill({
      status: 503,
      contentType: "application/json",
      body: JSON.stringify({ code: "UNAVAILABLE", error: "Not now." }),
    });
  });
  await page.goto("/app/");
  await expect(page.getByRole("button", { name: "New bot" })).toBeVisible();
});

test("REQ-061 a detected executor is attached only after an explicit click", async ({ page }) => {
  let attaches = 0;
  await seedRpc(page, {
    answers: {
      "bots/detectedExecutors": { executors: ["opencode"] },
      "bots/connectAcp": {
        botId: "bot_golden",
        executor: "opencode",
        attachedAt: "2026-11-18T00:00:00Z",
      },
    },
  });
  await page.route("**/rpc/bots/connectAcp", async (route) => {
    attaches += 1;
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        botId: "bot_golden",
        executor: "opencode",
        attachedAt: "2026-11-18T00:00:00Z",
      }),
    });
  });
  await page.goto("/app/");
  await expect(page.getByText("opencode — Detected on this machine")).toBeVisible();
  expect(attaches).toBe(0);
  await page.getByRole("button", { name: "Attach" }).click();
  expect(attaches).toBe(1);
  await expect(page.getByRole("button", { name: "Attached" })).toBeVisible();
});
