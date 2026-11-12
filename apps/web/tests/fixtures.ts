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
 * The answers a seed serves.
 *
 * A seed describes the state the API is in: the reply of each procedure, and the frames the
 * thread's stream replays when it opens. The keys are `namespace/procedure`, exactly like the
 * paths the client calls.
 */
export interface RpcSeed {
  /** The JSON body each procedure answers with. */
  answers: Record<string, unknown>;
  /** The frames the stream replays when it opens. */
  stream?: unknown[];
  /** Whether the stream refuses to open, the way a stopped server does. */
  streamUnavailable?: boolean;
}

/**
 * Applies a seed to a page.
 *
 * The calls are intercepted, never sent: the suite stays hermetic and never touches a backend.
 * The stream is intercepted in the page itself, because a server-sent event stream has to stay
 * open for the client to read it — a network answer ends the stream.
 */
export async function seedRpc(page: Page, seed: RpcSeed): Promise<void> {
  await page.addInitScript((frames: unknown[]) => {
    const listeners: Array<(frame: unknown) => void> = [];
    const scope = window as unknown as { __bomaPushFrame?: (frame: unknown) => void };

    scope.__bomaPushFrame = (frame: unknown) => {
      for (const listener of [...listeners]) {
        listener(frame);
      }
    };

    const original = window.fetch.bind(window);

    window.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
      const url =
        typeof input === "string"
          ? input
          : input instanceof Request
            ? input.url
            : String(input);

      if (!url.includes("/rpc/threads/subscribe")) {
        return original(input, init);
      }

      const encoder = new TextEncoder();
      const stream = new ReadableStream<Uint8Array>({
        start(controller) {
          const push = (frame: unknown) => {
            const type =
              typeof (frame as { type?: unknown } | null)?.type === "string"
                ? (frame as { type: string }).type
                : "message";
            controller.enqueue(encoder.encode(`event: ${type}\ndata: ${JSON.stringify(frame)}\n\n`));
          };
          for (const frame of frames) {
            push(frame);
          }
          listeners.push(push);
        },
      });

      return new Response(stream, {
        status: 200,
        headers: { "content-type": "text/event-stream" },
      });
    }) as typeof window.fetch;
  }, seed.stream ?? []);

  await page.route("**/rpc/**", async (route) => {
    const url = new URL(route.request().url());
    const procedure = url.pathname.replace(/^\/rpc\//, "");
    const answer = seed.answers[procedure];

    if (answer === undefined) {
      await route.fulfill({
        status: 404,
        contentType: "application/json",
        body: JSON.stringify({ code: "NOT_FOUND", error: "Not found." }),
      });
      return;
    }

    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(answer),
    });
  });

  // The page may already be loaded: the interception has to be in place before the screen asks
  // for its state, so the page is read again.
  if (page.url() !== "about:blank") {
    await page.reload();
  }
}

/** Pushes one frame into the thread's stream, as the server would. */
export async function pushStreamFrame(page: Page, frame: unknown): Promise<void> {
  await page.evaluate((value) => {
    const scope = window as unknown as { __bomaPushFrame?: (frame: unknown) => void };
    scope.__bomaPushFrame?.(value);
  }, frame);
}

/** The bot the seeds answer with. */
const SEED_BOT = {
  id: "bot_golden",
  name: "Boma Bot",
  threadId: "thr_golden",
  createdAt: "2026-01-01T00:00:00Z",
};

/** One user message, and nothing else. */
const SEED_ONE_MESSAGE = [
  {
    id: "msg_1",
    seq: 1,
    role: "user",
    blocks: [{ type: "text", text: "Plan my week." }],
    createdAt: "2026-01-01T00:00:01Z",
  },
];

/** A message that carries reactions already (REQ-008). */
const SEED_REACTED_MESSAGE = [
  {
    id: "msg_1",
    seq: 1,
    role: "user",
    blocks: [{ type: "text", text: "Plan my week." }],
    createdAt: "2026-01-01T00:00:01Z",
    reactions: [{ emoji: "\u{1F44D}", count: 2, reacted: true }],
  },
];

/** A thread with a conversation in it. */
const SEED_MANY_MESSAGES = [
  {
    id: "msg_1",
    seq: 1,
    role: "user",
    blocks: [{ type: "text", text: "Plan my week." }],
    createdAt: "2026-01-01T00:00:01Z",
  },
  {
    id: "msg_2",
    seq: 2,
    role: "bot",
    blocks: [{ type: "text", text: "Here is the plan." }],
    createdAt: "2026-01-01T00:00:02Z",
  },
  {
    id: "msg_3",
    seq: 3,
    role: "bot",
    blocks: [{ type: "tool", name: "calendar", running: false }],
    createdAt: "2026-01-01T00:00:03Z",
  },
  {
    id: "msg_4",
    seq: 4,
    role: "user",
    blocks: [
      { type: "text", text: "Add the notes." },
      { type: "file", artifactId: "art_1", name: "notes.md" },
    ],
    createdAt: "2026-01-01T00:00:04Z",
  },
  {
    id: "msg_5",
    seq: 5,
    role: "bot",
    blocks: [{ type: "text", text: "The notes are in." }],
    createdAt: "2026-01-01T00:00:05Z",
  },
];

/** The eight statuses, in the order the domain spells them. */
const SEED_RUN_STATUSES = [
  "queued",
  "leased",
  "running",
  "waiting_input",
  "waiting_takeover",
  "completed",
  "failed",
  "cancelled",
] as const;

/** One run per status, in the order the states are written in the spec. */
const SEED_ONE_RUN_PER_STATUS = SEED_RUN_STATUSES.map((status, index) => ({
  id: `run_${status}`,
  status,
  trigger: index % 2 === 0 ? "user" : "routine",
  promptSnippet: `Run number ${index + 1}`,
  startedAt: "2026-01-01T00:00:00Z",
  ...(status === "completed" || status === "failed" || status === "cancelled"
    ? { completedAt: "2026-01-01T00:00:10Z" }
    : {}),
}));

/**
 * Seed registry.
 *
 * Every seed answers the RPC surface with a fixed state, so a golden asserts a state and not a
 * click path. The key is the fixture's `state.seed`.
 */
/** One message whose only block is a pending question card (REQ-007). */
const SEED_CARD_MESSAGE = {
  id: "msg_card_1",
  seq: 1,
  role: "user",
  blocks: [{ type: "card", kind: "question" }],
  createdAt: "2026-01-01T00:00:01Z",
};

const SEEDS: Record<string, (page: Page) => Promise<void>> = {
  /** One message, one row: the REQ-001 golden counts the rows. */
  "thread-with-one-user-message": async (page) => {
    await seedRpc(page, {
      answers: {
        "bots/get": SEED_BOT,
        "threads/messages": { messages: SEED_ONE_MESSAGE },
        "threads/send": { messageId: "msg_sent", runId: "run_sent", status: "queued" },
      },
    });
  },

  /** A message that already carries reactions, one of them from the caller (REQ-008). */
  "thread-with-reacted-message": async (page) => {
    await seedRpc(page, {
      answers: {
        "bots/get": SEED_BOT,
        "threads/messages": { messages: SEED_REACTED_MESSAGE },
        "threads/react": {
          messageId: "msg_1",
          reactions: [{ emoji: "\u{1F44D}", count: 2, reacted: true }],
        },
      },
    });
  },

  /** A thread with a conversation in it. */
  "thread-with-many-messages": async (page) => {
    await seedRpc(page, {
      answers: {
        "bots/get": SEED_BOT,
        "threads/messages": { messages: SEED_MANY_MESSAGES },
        "threads/send": { messageId: "msg_sent", runId: "run_sent", status: "queued" },
      },
    });
  },

  /** A bot that has not said anything yet. */
  "thread-empty": async (page) => {
    await seedRpc(page, {
      answers: {
        "bots/get": SEED_BOT,
        "threads/messages": { messages: [] },
        "threads/send": { messageId: "msg_sent", runId: "run_sent", status: "queued" },
      },
    });
  },

  /** A pending question card keeps the composer usable and the composer stays usable (REQ-007). */
  "pending-question-card": async (page) => {
    await seedRpc(page, {
      answers: {
        "bots/get": SEED_BOT,
        "threads/messages": { messages: [SEED_CARD_MESSAGE] },
      },
    });
  },

  /** One run per status: the REQ-012 golden counts the rows and the states. */
  "one-run-per-status": async (page) => {
    await seedRpc(page, {
      answers: {
        "runs/list": { runs: SEED_ONE_RUN_PER_STATUS },
        "runs/retry": { ok: true },
      },
    });
  },
};

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

/** The seeds the registry knows, by name. */
export function seedNames(): string[] {
  return Object.keys(SEEDS);
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
  // The parameter is named `next`, not `use`: the React hooks lint reads a `use` call as a hook
  // call, and this is a Playwright fixture, not a component.
  context: async ({ context }, next) => {
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

    await next(context);

    if (blocked.length > 0) {
      throw new Error(
        `a test reached a non-loopback host; the suite must stay hermetic:\n${blocked.join("\n")}`,
      );
    }
  },
});

export { expect };
