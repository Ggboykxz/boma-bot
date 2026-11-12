import { expect, test } from "./fixtures";
import { direction, t } from "../lib/i18n";

/**
 * The shell contract (REQ-057).
 *
 * REQ-057 asks for real internationalisation: catalogue lookups with an English fallback and a
 * direction derived from the locale. These tests pin the two rules before a single screen exists,
 * because both are expensive to retrofit once strings are hard-coded.
 */

test("REQ-057 the shell declares a language, a direction and its landmarks", async ({ page }) => {
  await page.goto("/app/");

  await expect(page.locator("html")).toHaveAttribute("lang", "en");
  await expect(page.locator("html")).toHaveAttribute("dir", "ltr");
  await expect(page.getByRole("navigation", { name: "Primary" })).toBeVisible();
  await expect(page.getByRole("main")).toBeVisible();
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
});

test("REQ-057 a missing translation falls back to English, never to a blank", () => {
  // Present in English only: the Hebrew and French catalogues must fall back rather than invent.
  expect(t("he", "shell.nav.settings")).toBe("Settings");
  expect(t("fr", "shell.nav.settings")).toBe("Settings");

  // Present in every catalogue: no fallback, the translation is used.
  expect(t("fr", "shell.nav.threads")).toBe("Fils");
  expect(t("he", "shell.nav.threads")).toBe("שיחות");

  // Unknown locale: English.
  expect(t("xx", "shell.nav.threads")).toBe("Threads");

  // Unknown key: the key itself, which is visible in a screenshot and in a test.
  expect(t("en", "no.such.key")).toBe("no.such.key");
});

test("REQ-057 direction follows the locale, including a region subtag", () => {
  expect(direction("he")).toBe("rtl");
  expect(direction("he-IL")).toBe("rtl");
  expect(direction("ar-EG")).toBe("rtl");
  expect(direction("fa-IR")).toBe("rtl");
  expect(direction("fr")).toBe("ltr");
  expect(direction("en-US")).toBe("ltr");
});
