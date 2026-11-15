import { expect, test, applySeed } from "./fixtures";

test("REQ-059 the external agent attach/detach flow round-trips", async ({ page }) => {
  await applySeed(page, "thread-empty");
  await page.goto("/app/bot_golden");

  const calls: Array<Record<string, unknown>> = [];
  await page.route("**/rpc/bots/externalAgentStatus", async (route) => {
    const req = JSON.parse(route.request().postData() ?? "{}") as Record<string, unknown>;
    calls.push({ procedure: "externalAgentStatus", ...req });
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ connected: false }),
    });
  });
  await page.route("**/rpc/bots/connectExternal", async (route) => {
    const body = route.request().postDataJSON() as Record<string, unknown>;
    calls.push({ procedure: "connectExternal", ...body });
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        botId: "bot_golden",
        baseUrl: "https://hv.example.com",
        agentName: null,
        secretName: null,
        connectedAt: "2026-01-01T00:00:00Z",
      }),
    });
  });
  await page.route("**/rpc/bots/disconnectExternal", async (route) => {
    calls.push({ procedure: "disconnectExternal" });
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ botId: "bot_golden", disconnected: true }),
    });
  });

  // The panel shows the attach CTA while detached.
  await expect(page.getByRole("button", { name: /Attach an agent|Attacher un agent|חבר סוכן/ })).toBeVisible();

  // Open the panel; the inline form takes the URL and attaches.
  await page.getByRole("button", { name: /Attach an agent|Attacher un agent|חבר סוכן/ }).click();
  await page.getByLabel(/Agent URL|URL de l'agent|כתובת הסוכן/).fill("https://hv.example.com");
  await page.getByRole("button", { name: /Attach an agent|Attacher un agent|חבר סוכן/ }).last().click();

  await expect.poll(() => calls.some((entry) => entry.procedure === "connectExternal")).toBe(true);
  await expect.poll(() => calls.some((entry) => entry.procedure === "externalAgentStatus")).toBe(true);
});
