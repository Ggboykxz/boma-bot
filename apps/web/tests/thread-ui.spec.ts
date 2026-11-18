import { expect, seedRpc, test } from "./fixtures";

/**
 * The BoardUI chat states (REQ-012). The bespoke shimmer label used to be the only sign a run was
 * working; the free BoardUI components now carry that state without changing the run statuses.
 */

test("REQ-012 a running turn shows the BoardUI thinking indicator and composer loader", async ({
  page,
}) => {
  await seedRpc(page, {
    answers: {
      "bots/get": {
        id: "bot_golden",
        name: "Boma Bot",
        threadId: "thr_golden",
        archivedAt: null,
        snoozedUntil: null,
        unreadCount: 0,
      },
      "threads/messages": { messages: [] },
      "threads/send": { messageId: "msg_sent", runId: "run_sent", status: "running" },
    },
  });
  await page.goto("/app/bot_golden/");
  await page.getByRole("textbox", { name: "Message" }).fill("Plan my week");
  await page.getByRole("button", { name: "Send" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Working" })).toBeVisible();
  await expect(page.locator(".bui-composer-loader-rect").first()).toBeVisible();
});
