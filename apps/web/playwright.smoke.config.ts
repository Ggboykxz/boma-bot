import { defineConfig, devices } from "@playwright/test";

/**
 * Prod smoke: no webServer — the target is the built export served by the Rust binary with a real
 * local-first stack. The base URL comes from `BOMA_SMOKE_BASE`.
 *
 *   BOMA_SMOKE_BASE=http://127.0.0.1:3010 BOMA_SMOKE_PROD=1 BOMA_SMOKE_BOT=<bot-id> \
 *     npx playwright test --config playwright.smoke.config.ts
 */
export default defineConfig({
  testDir: ".",
  testMatch: "smoke-modes.spec.ts",
  forbidOnly: Boolean(process.env.CI),
  retries: 0,
  reporter: [["list"]],
  use: {
    ...devices["Desktop Chrome"],
    baseURL: process.env.BOMA_SMOKE_BASE ?? "http://127.0.0.1:3010",
    screenshot: "only-on-failure",
  },
  projects: [{ name: "chromium" }],
});
