import { readdirSync } from "node:fs";

import {
  REPO_GOLDEN_UI,
  applySeed,
  assertExpectations,
  expect,
  loadManifest,
  loadUiFixture,
  parseViewport,
  test,
} from "./fixtures";

/**
 * The UI golden runner (spec/08-ui-ux.md §8.10).
 *
 * It is driven by the same manifest as the Rust runner: a fixture is either replayed against the
 * real screen or skipped with a printed milestone and reason. A golden is never silently ignored,
 * and a new fixture that is absent from `golden-status.json` fails the manifest test below.
 */

const manifest = loadManifest();
const stems = readdirSync(REPO_GOLDEN_UI)
  .filter((name) => name.endsWith(".json"))
  .map((name) => name.slice(0, -".json".length))
  .sort();

// M0 declares no UI requirement of its own; REQ-057 is the only one the shell implements, and the
// harness itself must still be named after a requirement to be counted by tools/trace_check.py.
test("REQ-057 every UI golden is replayed or explicitly pending", () => {
  const declared = new Set(Object.keys(manifest.ui));
  const onDisk = new Set(stems);

  for (const stem of onDisk) {
    expect(declared.has(stem), `${stem} is on disk but absent from golden-status.json`).toBe(true);
  }
  for (const stem of declared) {
    expect(onDisk.has(stem), `${stem} is in golden-status.json but absent from disk`).toBe(true);
  }

  for (const [stem, entry] of Object.entries(manifest.ui)) {
    expect(entry.req, `${stem} must name a REQ id`).toMatch(/^REQ-\d{3}$/);
    if (entry.status === "pending") {
      expect(entry.reason?.trim(), `${stem} is pending and must say why`).toBeTruthy();
      expect(entry.reason, `${stem} must name its milestone`).toMatch(/^M\d+ - /);
    } else {
      expect(entry.reason, `${stem} is implemented; a stale reason hides regressions`).toBeNull();
    }
  }
});

for (const stem of stems) {
  const entry = manifest.ui[stem];

  test(`UI golden ${stem}`, async ({ page }) => {
    expect(entry, `${stem} has no manifest entry`).toBeDefined();
    if (!entry) {
      return;
    }

    if (entry.status === "pending") {
      test.skip(true, `${entry.req}: ${entry.reason ?? "no reason recorded"}`);
      return;
    }

    const fixture = loadUiFixture(stem);

    await page.setViewportSize(parseViewport(fixture.state.viewport));
    await page.emulateMedia({
      colorScheme: fixture.state.media.colorScheme,
      reducedMotion: fixture.state.media.reducedMotion,
    });
    await page.goto(fixture.state.route);
    await applySeed(page, fixture.state.seed);
    await assertExpectations(page, fixture.expect);
  });
}
