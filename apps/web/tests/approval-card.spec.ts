import { expect, seedRpc, test } from "./fixtures";

/**
 * Contextual approval cards (REQ-007, REQ-017).
 *
 * The card is the only place a member authorises an external effect. Its accessible name must be
 * a sentence, its effect/reversal must be readable without the transcript, and answering must be
 * an explicit, auditable choice.
 */

const approvalCard = {
  id: "msg_card_approval",
  seq: 1,
  role: "user",
  blocks: [{ type: "card", cardId: "card_approval", kind: "approval" }],
  createdAt: "2026-01-01T00:00:01Z",
};

async function seedApproval(page: import("@playwright/test").Page) {
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
      "threads/messages": { messages: [approvalCard] },
      "threads/answer": { ok: true },
    },
  });
}

test("REQ-007 the approval card names its region and explains effect and reversal", async ({
  page,
}) => {
  await seedApproval(page);
  await page.goto("/app/bot_golden/");
  const card = page.getByRole("region", { name: "Pending decision" });
  await expect(card).toBeVisible();
  await expect(card).toContainText("Approve lets the action run");
  await expect(card).toContainText("Deny stops it");
  await expect(card.getByRole("status")).toContainText("Waiting for your decision");
});

test("REQ-017 an approval answer records the explicit choice and unlocks the composer", async ({
  page,
}) => {
  await seedApproval(page);
  const answers: unknown[] = [];
  await page.route("**/rpc/threads/answer", async (route) => {
    answers.push(route.request().postDataJSON());
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ ok: true }),
    });
  });
  await page.goto("/app/bot_golden/");
  await expect(page.getByRole("textbox", { name: "Message" })).toBeDisabled();
  await page.getByRole("button", { name: "Deny" }).click();
  expect(answers).toEqual([{ cardId: "card_approval", answer: "deny" }]);
  await expect(page.getByRole("region", { name: "Pending decision" })).toContainText("Denied");
});
