import AxeBuilder from "@axe-core/playwright";

import { expect, test } from "./fixtures";

/**
 * The accessibility gate: zero critical or serious axe violations on every screen, in both colour
 * schemes and in both directions (AGENTS.md "Accessibility", spec/08-ui-ux.md).
 *
 * The gate is cross-cutting rather than a numbered behaviour, so the tests carry REQ-057, the
 * requirement that makes direction and locale part of the markup; a regression here is almost
 * always a locale, label or contrast regression.
 */

const ROUTES = ["/", "/app/"] as const;
const TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"];

async function analyse(page: import("@playwright/test").Page) {
  const results = await new AxeBuilder({ page }).withTags(TAGS).analyze();
  return results.violations
    .filter((violation) => violation.impact === "critical" || violation.impact === "serious")
    .map((violation) => `${violation.id} (${violation.impact}): ${violation.nodes.length} node(s)`);
}

for (const route of ROUTES) {
  for (const scheme of ["light", "dark"] as const) {
    test(`REQ-057 ${route} has no critical or serious axe violation (${scheme})`, async ({
      page,
    }) => {
      await page.emulateMedia({ colorScheme: scheme });
      await page.goto(route);

      if (scheme === "dark") {
        // BoardUI flips tokens from the `.dark` class, not from the media query alone.
        await page.evaluate(() => document.documentElement.classList.add("dark"));
      }

      expect(await analyse(page)).toEqual([]);
    });
  }
}

test("REQ-057 the shell mirrors in RTL without an accessibility regression", async ({ page }) => {
  await page.goto("/app/");
  await page.evaluate(() => {
    document.documentElement.lang = "he";
    document.documentElement.dir = "rtl";
  });

  const mirrored = await page.evaluate(
    () => getComputedStyle(document.querySelector("nav")!).direction,
  );
  expect(mirrored).toBe("rtl");
  expect(await analyse(page)).toEqual([]);
});
