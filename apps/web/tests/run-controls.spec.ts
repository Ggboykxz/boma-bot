import { expect, seedRpc, test } from "./fixtures";

/**
 * Pausing and resuming a queued run from the activity screen (spec/01 REQ-067).
 *
 * A paused run leaves the FIFO and returns at the same position; the screen must never leave a
 * member guessing which state the run is in, and a running run offers no pause.
 */

const queuedRun = {
  id: "run_queued",
  threadId: "thr_golden",
  botId: "bot_golden",
  status: "queued",
  trigger: "user",
  promptSnippet: "Plan my week",
  createdAt: "2026-11-18T00:00:00Z",
};

test("REQ-067 a queued run can be paused and resumed from the activity list", async ({ page }) => {
  let paused = 0;
  let resumed = 0;
  await seedRpc(page, { answers: { "runs/list": { runs: [queuedRun] } } });
  await page.route("**/rpc/runs/pause", async (route) => {
    paused += 1;
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ id: "run_queued", status: "paused" }),
    });
  });
  await page.route("**/rpc/runs/resume", async (route) => {
    resumed += 1;
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ id: "run_queued", status: "queued" }),
    });
  });

  await page.goto("/app/activity/");
  await expect(page.getByText("Plan my week")).toBeVisible();
  await page.getByRole("button", { name: "Pause" }).click();
  expect(paused).toBe(1);
  await expect(page.getByRole("status").filter({ hasText: "Paused" })).toBeVisible();
  await page.getByRole("button", { name: "Resume" }).click();
  expect(resumed).toBe(1);
  await expect(page.getByRole("status").filter({ hasText: "Waiting to start" })).toBeVisible();
});

test("REQ-067 a running run offers no pause control", async ({ page }) => {
  await seedRpc(page, {
    answers: { "runs/list": { runs: [{ ...queuedRun, id: "run_running", status: "running" }] } },
  });
  await page.goto("/app/activity/");
  await expect(page.getByRole("button", { name: "Pause" })).toHaveCount(0);
});
