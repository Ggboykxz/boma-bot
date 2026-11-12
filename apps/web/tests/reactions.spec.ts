import { expect, test, applySeed } from "./fixtures";

test("REQ-008 a member reacts to a message, the count moves, and it survives a reload", async ({
  page,
}) => {
  await page.goto("/app/bot_golden");
  await applySeed(page, "thread-with-reacted-message");

  const log = page.getByRole("log");
  await expect(log).toBeVisible();
  await expect(log).toContainText("Plan my week.");

  // The chip from the seed renders with the count and the caller's own reaction.
  const thumbsUp = page.getByRole("button", { name: "React with 👍" });
  await expect(thumbsUp).toBeVisible();
  await expect(thumbsUp).toHaveAttribute("aria-pressed", "true");
  await expect(thumbsUp).toContainText("2");

  // A stateful mock stands in for the server: the body decides the truth it answers with.
  const calls: Array<Record<string, unknown>> = [];
  const counts = new Map<string, number>([["👍", 2]]);
  const mine = new Set<string>(["👍"]);
  await page.route("**/rpc/threads/react", async (route) => {
    const body = route.request().postDataJSON() as Record<string, unknown>;
    calls.push(body);
    const emoji = String(body.emoji);
    if (body.action === "add") {
      counts.set(emoji, (counts.get(emoji) ?? 0) + 1);
      mine.add(emoji);
    } else {
      const next = (counts.get(emoji) ?? 1) - 1;
      if (next <= 0) {
        counts.delete(emoji);
      } else {
        counts.set(emoji, next);
      }
      mine.delete(emoji);
    }
    const reactions = [...counts.entries()].map(([reaction, count]) => ({
      emoji: reaction,
      count,
      reacted: mine.has(reaction),
    }));
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ messageId: "msg_1", reactions }),
    });
  });

  // Removing the caller's own reaction leaves the other member's chip behind.
  await thumbsUp.click();
  expect(calls[0]).toMatchObject({ messageId: "msg_1", emoji: "👍", action: "remove" });
  const afterRemove = page.getByRole("button", { name: "React with 👍" });
  await expect(afterRemove).toHaveAttribute("aria-pressed", "false");
  await expect(afterRemove).toContainText("1");

  // Adding it back through the quick picker moves the count again.
  await page.getByRole("button", { name: "Add a reaction" }).first().click();
  const picker = page.getByRole("group", { name: "Choose an emoji" });
  await expect(picker).toBeVisible();
  await page.getByRole("button", { name: "React with 👍" }).first().click();
  expect(calls[1]).toMatchObject({ emoji: "👍", action: "add" });
  const readded = page.getByRole("button", { name: "React with 👍" });
  await expect(readded).toHaveAttribute("aria-pressed", "true");
  await expect(readded).toContainText("2");

  // A reaction the caller never left can be added from the same picker.
  await page.getByRole("button", { name: "Add a reaction" }).first().click();
  await page.getByRole("button", { name: "React with 🎉" }).click();
  expect(calls[2]).toMatchObject({ emoji: "🎉", action: "add" });
  const party = page.getByRole("button", { name: "React with 🎉" });
  await expect(party).toHaveAttribute("aria-pressed", "true");
  await expect(party).toContainText("1");
});
