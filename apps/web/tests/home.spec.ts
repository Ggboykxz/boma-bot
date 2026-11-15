import { expect, test } from "./fixtures";

/**
 * The root screen (spec/08-ui-ux.md §8.1).
 *
 * Signed in goes to /app/; signed out goes to /sign-in. The landmark renders in both cases.
 */

test("REQ-041 the root redirects a signed-in member to /app", async ({ page }) => {
  await page.route("**/rpc/bots/list", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ bots: [] }),
    });
  });
  await page.goto("/");
  await page.waitForURL("**/app/", { timeout: 10_000 });
});

test("REQ-041 the root redirects a signed-out visitor to /sign-in", async ({ page }) => {
  await page.route("**/rpc/bots/list", async (route) => {
    await route.fulfill({
      status: 401,
      contentType: "application/json",
      body: JSON.stringify({ code: "UNAUTHORIZED", error: "Sign in to continue." }),
    });
  });
  await page.goto("/");
  await page.waitForURL("**/sign-in/", { timeout: 15_000 });
});
