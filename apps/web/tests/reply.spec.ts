import { expect, test, applySeed } from "./fixtures";

test("REQ-008 a reply carries the answered message and its quote", async ({ page }) => {
  await page.goto("/app/bot_golden");
  await applySeed(page, "thread-with-one-user-message");

  const sent: Array<Record<string, unknown>> = [];
  await page.route("**/rpc/threads/send", async (route) => {
    const body = route.request().postDataJSON() as Record<string, unknown>;
    sent.push(body);
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ messageId: "msg_sent", runId: "run_sent", status: "queued" }),
    });
  });

  // One user message in the transcript, "Plan my week.": its role label is the reply target.
  const log = page.getByRole("log");
  await expect(log).toBeVisible();
  await expect(log).toContainText("Plan my week.");

  // Start a reply from that message, then write a short answer.
  await page.getByRole("button", { name: "Reply" }).first().click();
  await expect(page.getByText("Replying to:")).toBeVisible();

  const composer = page.getByLabel(/Message/);
  await composer.fill("On it, Monday.");
  await page.getByRole("button", { name: /Send/i }).click();

  await expect(sent.length).toBeGreaterThan(0);
  const last = sent[sent.length - 1];
  expect(last.replyToMessageId).toBeTruthy();
  expect(last.replyQuote).toContain("Plan my week.");
});
