import { readFileSync } from "node:fs";
import path from "node:path";

import { expect, test as base, type Page } from "@playwright/test";

/**
 * Shared fixtures for the web suites.
 *
 * Two invariants live here rather than in every spec:
 *
 * 1. **No test touches the network.** Playwright routes every request through a guard that only
 *    lets loopback through. A request to a CDN, a font host or a real deployment is aborted and
 *    fails the test afterwards, so a stray `<img src="https://…">` cannot make CI depend on the
 *    internet.
 * 2. **Golden state comes from the manifest.** `golden-status.json` decides whether a fixture is
 *    replayed or skipped, and a skip always carries its milestone and reason. The suite never
 *    decides on its own that a fixture does not matter.
 */

/** Repository root, from `apps/web` (the directory Playwright runs in). */
export const REPO_ROOT = path.resolve(process.cwd(), "../..");
export const REPO_GOLDEN_UI = path.join(REPO_ROOT, "spec", "golden", "ui");

export type GoldenStatus = "implemented" | "pending";

export interface ManifestEntry {
  status: GoldenStatus;
  req: string;
  reason: string | null;
}

export interface Manifest {
  api: Record<string, ManifestEntry>;
  ui: Record<string, ManifestEntry>;
}

export interface UiMedia {
  colorScheme: "light" | "dark";
  reducedMotion: "reduce" | "no-preference";
}

export interface UiState {
  route: string;
  seed: string;
  role: string;
  viewport: string;
  media: UiMedia;
}

export interface UiExpectation {
  role: string;
  state?: string;
  count?: number;
}

export interface UiFixture {
  state: UiState;
  expect: UiExpectation[];
  notes: string;
}

/** Reads the one manifest that also drives the Rust golden runner. */
export function loadManifest(): Manifest {
  const file = path.join(REPO_ROOT, "golden-status.json");
  return JSON.parse(readFileSync(file, "utf8")) as Manifest;
}

/** Reads one UI fixture by file stem. */
export function loadUiFixture(stem: string): UiFixture {
  const file = path.join(REPO_GOLDEN_UI, `${stem}.json`);
  return JSON.parse(readFileSync(file, "utf8")) as UiFixture;
}

/** `1280x800` to a Playwright viewport. */
export function parseViewport(viewport: string): { width: number; height: number } {
  const [width, height] = viewport.split("x").map(Number);
  if (!width || !height) {
    throw new Error(`the fixture viewport is not WIDTHxHEIGHT: ${viewport}`);
  }
  return { width, height };
}

/**
 * Seed registry.
 *
 * M0 has no application data, so every seed is unimplemented and an `implemented` fixture that
 * needs one fails loudly instead of asserting against the placeholder shell. Each seed lands with
 * the milestone that owns its screen; the key is the fixture's `state.seed`.
 */
const SEEDS: Record<string, (page: Page) => Promise<void>> = {};

/** Applies a fixture's seed, or explains which milestone still owes it. */
export async function applySeed(page: Page, seed: string): Promise<void> {
  const apply = SEEDS[seed];
  if (!apply) {
    throw new Error(
      `the seed \`${seed}\` has no implementation yet: add it to tests/fixtures.ts with the ` +
        `milestone that builds the screen. A golden marked implemented cannot be replayed without it.`,
    );
  }
  await apply(page);
}

type Role = Parameters<Page["getByRole"]>[0];

/** Asserts one fixture's `expect` list, in document order. */
export async function assertExpectations(
  page: Page,
  expectations: UiExpectation[],
): Promise<void> {
  for (const assertion of expectations) {
    const locator = page.getByRole(assertion.role as Role);
    const label = `role=${assertion.role}`;

    if (assertion.count !== undefined) {
      await expect(locator, `${label} count`).toHaveCount(assertion.count);
    }

    switch (assertion.state) {
      case "visible":
        await expect(locator.first(), `${label} visibility`).toBeVisible();
        break;
      case "absent":
        await expect(locator, `${label} absent`).toHaveCount(0);
        break;
      case "enabled":
        await expect(locator.first(), `${label} enabled`).toBeEnabled();
        break;
      case "disabled":
        await expect(locator.first(), `${label} disabled`).toBeDisabled();
        break;
      case "hidden":
        await expect(locator.first(), `${label} hidden`).toBeHidden();
        break;
      case undefined:
        if (assertion.count === undefined) {
          await expect(locator.first(), `${label} exists`).toBeVisible();
        }
        break;
      default:
        throw new Error(`the fixture uses an unknown state: ${assertion.state}`);
    }
  }
}

const LOOPBACK_HOSTS = new Set(["127.0.0.1", "localhost", "::1", "[::1]"]);
const PASSTHROUGH_PROTOCOLS = new Set(["data:", "blob:", "about:"]);

export const test = base.extend({
  context: async ({ context }, use) => {
    const blocked: string[] = [];

    await context.route("**/*", async (route) => {
      const request = route.request();
      const url = new URL(request.url());

      if (PASSTHROUGH_PROTOCOLS.has(url.protocol)) {
        await route.continue();
        return;
      }

      if (!LOOPBACK_HOSTS.has(url.hostname)) {
        blocked.push(request.url());
        await route.abort("blockedbyclient");
        return;
      }

      await route.continue();
    });

    await use(context);

    if (blocked.length > 0) {
      throw new Error(
        `a test reached a non-loopback host; the suite must stay hermetic:\n${blocked.join("\n")}`,
      );
    }
  },
});

export { expect };
