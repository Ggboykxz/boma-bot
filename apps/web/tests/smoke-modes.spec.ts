import { AxeBuilder } from "@axe-core/playwright";

import { expect, test } from "@playwright/test";

/**
 * Cross-mode smoke: the same four checks run against `next dev` (the repo's usual test surface)
 * and against the built static export served by the Rust binary with a real local-first stack.
 *
 *   npm test -- tests/smoke-modes.spec.ts                     # dev (config webServer)
 *   BOMA_SMOKE_BASE=http://127.0.0.1:3010 npx playwright test --config ../../tests/playwright.smoke.config.ts
 *
 * The prod pass needs a seeded bot: `BOMA_SMOKE_BOT` holds its id. In dev the fixtures already
 * replay a bot at `/app/bot_golden`, so the page-level assertions stay identical.
 */

const BASE = process.env.BOMA_SMOKE_BASE ?? "http://127.0.0.1:3100";
const PROD = process.env.BOMA_SMOKE_PROD === "1";
const BOT = process.env.BOMA_SMOKE_BOT ?? "bot_golden";
const ROOM = `/app/${BOT}`;

/** A page error is never acceptable in either mode. */
function watchErrors(page: import("@playwright/test").Page) {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(String(error)));
  return errors;
}

test("smoke: the deployment answers and the app shell renders", async ({ browser }) => {
  const context = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const page = await context.newPage();
  const errors = watchErrors(page);

  if (PROD) {
    // The real backend is the contract: /health, /ready and the capability report.
    const health = await page.request.get(`${BASE}/health`);
    expect(health.status()).toBe(200);
    const ready = await page.request.get(`${BASE}/ready`);
    expect(ready.status()).toBe(200);
    const caps = await page.request.get(`${BASE}/api/auth/capabilities`);
    expect(caps.status()).toBe(200);
    expect((await caps.json()).localFirst).toBe(true);
  }

  const response = await page.goto(`${BASE}/app/`);
  expect(response?.status()).toBe(200);
  await expect(page.getByRole("navigation", { name: "Primary" })).toBeVisible();
  await expect(page.getByRole("main")).toBeVisible();
  await expect(page.getByRole("button", { name: "Toggle dark mode" })).toBeVisible();
  expect(errors).toEqual([]);
  await context.close();
});

test("smoke: a room loads, sends a message and shows the answer", async ({ browser }) => {
  const context = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const page = await context.newPage();
  const errors = watchErrors(page);
  await page.goto(`${BASE}${ROOM}`);

  if (PROD) {
    // Seed one turn through the real API, then assert the app renders it.
    const sent = await page.request.post(`${BASE}/rpc/threads/send`, {
      data: { botId: BOT, text: "Smoke: say hello", clientNonce: `smoke-${Date.now()}` },
    });
    expect(sent.status()).toBe(200);
    await page.reload();
    await expect(page.getByRole("log", { name: "Conversation" }).getByText("Smoke: say hello")).toBeVisible();
  }

  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  await expect(page.getByRole("textbox", { name: "Message" })).toBeVisible();
  expect(errors).toEqual([]);
  await context.close();
});

test("smoke: no critical or serious axe finding on the two entry screens", async ({ browser }) => {
  const context = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const page = await context.newPage();
  for (const route of ["/app/", ROOM]) {
    await page.goto(`${BASE}${route}`);
    // Entrance animations are 220–400 ms; axe must scan a settled frame, not a mid-fade one.
    await page.waitForTimeout(600);
    const results = await new AxeBuilder({ page })
      .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"])
      .analyze();
    const serious = results.violations.filter(
      (violation) => violation.impact === "critical" || violation.impact === "serious",
    );
    expect(serious.map((v) => `${v.id}: ${v.nodes.length}`)).toEqual([]);
  }
  await context.close();
});
