import { expect, test } from "./fixtures";
import { t } from "../lib/i18n";

/**
 * `/app/board` (spec/08-ui-ux.md §8.6, REQ-038).
 *
 * When the board surface answers, the kanban renders with tickets in their columns; creating
 * and moving a ticket go through the same RPCs. When the surface is absent (the flag off),
 * the route renders the not-found shape — spec: absent, not locked.
 */

const TICKET = {
  id: "tic_golden",
  key: "BOT-1",
  title: "Audit the week's bills",
  description: "Read the inbox receipts folder; report suspicious amounts.",
  state: "todo",
  assigneeBotId: "bot_golden",
  commentCount: 2,
  createdAt: "2026-01-01T00:00:00Z",
  updatedAt: "2026-01-01T00:00:00Z",
};

async function seedBots(page: import("@playwright/test").Page) {
  await page.route("**/rpc/bots/list", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        bots: [
          {
            id: "bot_golden",
            name: "Analyst",
            threadId: "thr_golden",
            archivedAt: null,
            snoozedUntil: null,
            unreadCount: 0,
          },
        ],
      }),
    });
  });
}

test("REQ-038 the board renders tickets in their columns when the flag is on", async ({ page }) => {
  await seedBots(page);
  await page.route("**/rpc/boards/get", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ tickets: [TICKET] }),
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

  await page.goto("/app/board/");
  await expect(page.getByRole("heading", { name: t("en", "board.title") })).toBeVisible();
  await expect(page.getByText("Audit the week's bills")).toBeVisible();
  const card = page.getByRole("button", { name: /BOT-1/ });
  await expect(card).toBeVisible();
  await expect(card.getByText("Analyst")).toBeVisible();
});

test("REQ-038 the board renders the not-found shape when the flag is off", async ({ page }) => {
  await seedBots(page);
  await page.route("**/rpc/boards/get", async (route) => {
    await route.fulfill({
      status: 404,
      contentType: "application/json",
      body: JSON.stringify({ code: "NOT_FOUND", error: "Not found." }),
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

  await page.goto("/app/board/");
  await expect(page.getByText(t("en", "board.absent"))).toBeVisible();
});
