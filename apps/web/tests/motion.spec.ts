import { expect, seedRpc, test } from "./fixtures";

/**
 * Motion contract (REQ-057): entrances carry meaning; reduced motion keeps the fade and drops
 * the movement. The frequent actions (keyboard, list navigation) deliberately have no animation.
 */

test("REQ-057 reduced motion keeps the card fade but drops its movement", async ({ page }) => {
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
      "threads/messages": {
        messages: [
          {
            id: "msg_card_approval",
            seq: 1,
            role: "user",
            blocks: [{ type: "card", cardId: "card_approval", kind: "approval" }],
            createdAt: "2026-01-01T00:00:01Z",
          },
        ],
      },
    },
  });
  await page.goto("/app/bot_golden/");
  const card = page.locator(".boma-card-in").first();
  await expect(card).toBeVisible();
  const animationName = () => card.evaluate((el) => getComputedStyle(el).animationName);
  await expect.poll(animationName).toBe("boma-card-in");
  await page.emulateMedia({ reducedMotion: "reduce" });
  await expect.poll(animationName).toBe("boma-fade-in");
});
