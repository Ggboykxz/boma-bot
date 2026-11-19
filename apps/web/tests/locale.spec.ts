import { SUPPORTED_LOCALES, t } from "../lib/i18n";
import { expect, test } from "./fixtures";

/**
 * REQ-057 — fourteen locales ship; a missing string falls back to English, never to the key.
 * The ten additional catalogues are intentionally partial: their core navigation and action
 * strings are translated, the long tail falls back until a translator fills it.
 */

test("REQ-057 fourteen locales ship and missing strings fall back to English", () => {
  expect(SUPPORTED_LOCALES).toHaveLength(14);
  for (const locale of SUPPORTED_LOCALES) {
    expect(t(locale, "app.name")).toBe("Boma Bot");
  }
  expect(t("de", "shell.nav.threads")).toBe("Unterhaltungen");
  expect(t("de", "board.filter.assignee")).toBe("Filter by assignee");
});
