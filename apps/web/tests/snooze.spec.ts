import { expect, test, applySeed } from "./fixtures";

const ROSTER_REGULAR = {
  id: "bot_golden",
  name: "Boma Bot",
  threadId: "thr_golden",
  archivedAt: null,
  snoozedUntil: null,
  unreadCount: 2,
};

function rosterEntry(snoozed: boolean) {
  return {
    ...ROSTER_REGULAR,
    id: "bot_quiet",
    name: "Quiet Bot",
    threadId: "thr_quiet",
    snoozedUntil: snoozed ? "2999-01-01T00:00:00Z" : null,
    unreadCount: 4,
  };
}

/** Registers a stateful roster after the seed: unsnoozing moves the bot back to the regular list. */
async function withStatefulSnooze(page: import("@playwright/test").Page) {
  let quietSnoozed = true;
  await page.route("**/rpc/threads/unsnooze", async (route) => {
    quietSnoozed = false;
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ threadId: "thr_quiet", snoozedUntil: null }),
    });
  });
  await page.route("**/rpc/threads/snooze", async (route) => {
    quietSnoozed = true;
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ threadId: "thr_golden", snoozedUntil: "2999-01-01T08:00:00.000Z" }),
    });
  });
  await page.route("**/rpc/bots/list", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ bots: [ROSTER_REGULAR, rosterEntry(quietSnoozed)] }),
    });
  });
  return {
    isSnoring: () => quietSnoozed,
  };
}

test("REQ-009 the rail hides a snoozed bot and the shelf brings it back", async ({ page }) => {
  await page.goto("/app/bot_golden");
  await applySeed(page, "roster-with-snoozed-bot");
  await withStatefulSnooze(page);

  // The regular list shows the golden bot with its unread badge.
  const regular = page.getByRole("link", { name: /Boma Bot/ });
  await expect(regular).toBeVisible();
  await expect(regular).toContainText("2");
  // The snoozed bot is not in the regular list.
  await expect(page.getByRole("link", { name: /Quiet Bot/ })).toHaveCount(0);

  // The shelf opens and shows the snoozed bot, with its own badge and an unsnooze button.
  const shelf = page.getByRole("button", { name: "Snoozed (1)" });
  await expect(shelf).toHaveAttribute("aria-expanded", "false");
  await shelf.click();
  await expect(shelf).toHaveAttribute("aria-expanded", "true");
  const quiet = page.getByRole("link", { name: /Quiet Bot/ });
  await expect(quiet).toBeVisible();
  await expect(quiet).toContainText("4");

  // Unsnoozing moves the bot back to the regular list and empties the shelf.
  await page.getByRole("button", { name: "Unsnooze" }).first().click();
  await expect(page.getByRole("link", { name: /Quiet Bot/ })).toBeVisible();
  await expect(page.getByRole("button", { name: /Snoozed/ })).toHaveCount(0);
});

test("REQ-009 the thread header snoozes the conversation and the rail follows", async ({
  page,
}) => {
  await page.goto("/app/bot_golden");
  await applySeed(page, "roster-with-snoozed-bot");
  await withStatefulSnooze(page);

  // The toggle starts available as "Snooze this conversation".
  const toggle = page.getByRole("button", { name: "Snooze this conversation" });
  await expect(toggle).toBeVisible();
  await expect(toggle).toHaveAttribute("aria-pressed", "false");

  // After snoozing, the label flips and the roster moves the bot to the shelf.
  await toggle.click();
  const unsnooze = page.getByRole("button", { name: "End the snooze" });
  await expect(unsnooze).toBeVisible();
  await expect(unsnooze).toHaveAttribute("aria-pressed", "true");

  await unsnooze.click();
  await expect(page.getByRole("button", { name: "Snooze this conversation" })).toBeVisible();
});
