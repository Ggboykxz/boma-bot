import { expect, test } from "./fixtures";
import { t } from "../lib/i18n";

/**
 * The auth cluster (spec/08-ui-ux.md §8.2, REQ-040, REQ-041).
 *
 * The capabilities are the source of what renders — the same shape the binary serves. Sign-up,
 * sign-in and the recovery flow are exercised end to end in a browser fixture; the network
 * never leaves the loopback routes Playwright seeds.
 */

const CAPABILITIES = {
  version: "0.1.0",
  runtime: "sandboxed",
  signup: "open",
  password: true,
  emailCode: false,
  passkey: false,
  sso: [],
  provider: null,
  revision: null,
};

async function seedCapabilities(page: import("@playwright/test").Page) {
  await page.route("**/api/auth/capabilities", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(CAPABILITIES),
    });
  });
}

async function seedSignIn(page: import("@playwright/test").Page, ok: boolean) {
  await page.route("**/api/auth/sign-in", async (route) => {
    if (ok) {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        headers: { "set-cookie": "session=abc123; Path=/; HttpOnly; Secure; SameSite=Lax" },
        body: "{}",
      });
      return;
    }
    await route.fulfill({
      status: 401,
      contentType: "application/json",
      body: JSON.stringify({ code: "INVALID_CREDENTIALS", error: "The credentials did not match." }),
    });
  });
}

test("REQ-040 the sign-in screen renders the capabilities-driven password form", async ({ page }) => {
  await seedCapabilities(page);
  await seedSignIn(page, true);

  await page.goto("/sign-in/");
  await expect(page.getByRole("heading", { name: t("en", "signin.title") })).toBeVisible();
  await expect(page.getByLabel(t("en", "signin.email"))).toBeVisible();
  await expect(page.getByLabel(t("en", "signin.password"))).toBeVisible();

  await page.getByLabel(t("en", "signin.email")).fill("person@example.com");
  await page.getByLabel(t("en", "signin.password")).fill("correct horse battery");
  await page.getByRole("button", { name: t("en", "signin.submit") }).click();
  await page.waitForURL("**/app/**", { timeout: 10_000 });
});

test("REQ-040 an invalid sign-in shows an error that never says which side was wrong", async ({ page }) => {
  await seedCapabilities(page);
  await seedSignIn(page, false);

  await page.goto("/sign-in/");
  await page.getByLabel(t("en", "signin.email")).fill("person@example.com");
  await page.getByLabel(t("en", "signin.password")).fill("wrong password");
  await page.getByRole("button", { name: t("en", "signin.submit") }).click();

  await expect(page.getByText(t("en", "signin.error.credentials"), { exact: true })).toBeVisible();
  await expect(page).toHaveURL(/\/sign-in\//);
});

test("REQ-040 a closed sign-up tells the visitor instead of failing them in silence", async ({ page }) => {
  await page.route("**/api/auth/capabilities", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ ...CAPABILITIES, signup: "closed" }),
    });
  });

  await page.goto("/sign-up/");
  await expect(page.getByText(t("en", "signup.closed"))).toBeVisible();
  await expect(page.getByRole("button", { name: t("en", "signup.submit") })).toHaveCount(0);
});

test("REQ-040 the forgot-password screen answers the same words regardless of whether the address exists", async ({ page }) => {
  await page.route("**/api/auth/forgot-password", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ ok: true }),
    });
  });

  for (const email of ["nobody@example.com", "person@example.com"]) {
    await page.goto("/forgot-password/");
    await page.getByLabel(t("en", "signin.email")).fill(email);
    await page.getByRole("button", { name: t("en", "forgot.submit") }).click();
    await expect(page.getByText(t("en", "forgot.sent"))).toBeVisible();
  }
});

test("REQ-040 the reset screen refuses a mismatch locally, then consumes the link", async ({ page }) => {
  let posted: { token?: string; password?: string } | null = null;
  await page.route("**/api/auth/reset-password", async (route) => {
    posted = route.request().postDataJSON();
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      headers: { "set-cookie": "session=reset-ok; Path=/; HttpOnly; Secure; SameSite=Lax" },
      body: "{}",
    });
  });

  await page.goto("/reset-password/?token=reset-for-sure-token");
  await expect(page.getByRole("heading", { name: t("en", "reset.title") })).toBeVisible();

  await page.getByLabel(t("en", "reset.password"), { exact: true }).fill("new password that passes");
  await page.getByLabel(t("en", "reset.confirm")).fill("different password 123");
  await page.getByRole("button", { name: t("en", "reset.submit") }).click();
  await expect(page.getByText(t("en", "reset.error.mismatch"), { exact: true })).toBeVisible();
  expect(posted).toBeNull();

  await page.getByLabel(t("en", "reset.confirm")).fill("new password that passes");
  await page.getByRole("button", { name: t("en", "reset.submit") }).click();
  await expect.poll(() => posted).toMatchObject({ token: "reset-for-sure-token" });
  await page.waitForURL("**/app/**", { timeout: 10_000 });
});

test("REQ-041 a 401 from any RPC lands the visitor on /sign-in with the path intact", async ({ page }) => {
  await page.route("**/rpc/bots/list", async (route) => {
    await route.fulfill({
      status: 401,
      contentType: "application/json",
      body: JSON.stringify({ code: "UNAUTHORIZED", error: "Not signed in." }),
    });
  });

  await page.goto("/app/");
  await page.waitForURL("**/sign-in**", { timeout: 10_000 });
  expect(page.url()).toContain("next=");
});
