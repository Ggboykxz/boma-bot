/**
 * Regenerates the README screenshots (`docs/screenshots/`) from the static export.
 *
 * The shots render with hermetic RPC answers — the same canned state the Playwright suite
 * replays — so nothing here needs a backend and every pixel comes from this repository. Run:
 *
 *   npm run build && node scripts/screenshots.cjs
 *
 * The script serves `out/` on loopback, captures at 1440x900 in the dark colour scheme with
 * reduced motion, and writes into `docs/screenshots/`.
 */
const { spawn } = require("child_process");
const path = require("path");

const REPO_ROOT = path.join(__dirname, "..", "..", "..");
const OUT_DIR = path.join(__dirname, "..", "out");
const SHOTS_DIR = path.join(REPO_ROOT, "docs", "screenshots");
const PORT = Number(process.env.BOMA_SHOTS_PORT ?? 3213);
const BASE = `http://127.0.0.1:${PORT}`;
const EXPECTED = ["apercu-landing.png", "apercu-fil.png", "apercu-groupe.png", "apercu-activite.png"];

const SEED_BOT = {
  id: "bot_golden",
  name: "Boma Bot",
  threadId: "thr_golden",
  createdAt: "2026-01-01T00:00:00Z",
};

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

const ROSTER = {
  bots: [
    {
      id: "bot_golden",
      name: "Boma Bot",
      threadId: "thr_golden",
      archivedAt: null,
      snoozedUntil: null,
      unreadCount: 0,
    },
    {
      id: "bot_analyst",
      name: "Analyst",
      threadId: "thr_analyst",
      archivedAt: null,
      snoozedUntil: null,
      unreadCount: 2,
    },
    {
      id: "bot_writer",
      name: "Writer",
      threadId: "thr_writer",
      archivedAt: null,
      snoozedUntil: "2999-01-01T00:00:00Z",
      unreadCount: 4,
    },
  ],
};

const HEAD = {
  threadId: "thr_golden",
  lastSeq: 5,
  lastMessageAt: "2026-01-01T00:00:05Z",
  readSeq: 5,
  unreadCount: 0,
};

const GROUP = {
  id: "group_golden",
  name: "Triage",
  threadId: "thr_group_golden",
  memberBotIds: ["bot_analyst", "bot_writer"],
  memberBots: [
    { id: "bot_analyst", name: "Analyst" },
    { id: "bot_writer", name: "Writer" },
  ],
  createdAt: "2026-01-01T00:00:00Z",
};

// Newest first, the way the server answers.
const GROUP_MESSAGES = [
  {
    id: "msg_group_2",
    seq: 2,
    role: "bot",
    botId: "bot_analyst",
    blocks: [{ type: "text", text: "The thread is calm: three PRs merged." }],
    runId: "run_group_1",
    createdAt: "2026-01-01T00:00:02Z",
  },
  {
    id: "msg_group_1",
    seq: 1,
    role: "user",
    blocks: [{ type: "text", text: "@analyst summarise the thread" }],
    createdAt: "2026-01-01T00:00:01Z",
  },
];

const RUN_STATUSES = [
  "queued",
  "leased",
  "running",
  "waiting_input",
  "waiting_takeover",
  "completed",
  "failed",
  "cancelled",
];

const ACTIVITY_RUNS = RUN_STATUSES.map((status, index) => ({
  id: `run_${status}`,
  status,
  trigger: index % 2 === 0 ? "user" : "routine",
  promptSnippet: `Run number ${index + 1}`,
  startedAt: "2026-01-01T00:00:00Z",
  ...(status === "completed" || status === "failed" || status === "cancelled"
    ? { completedAt: "2026-01-01T00:00:10Z" }
    : {}),
}));

/** A marker for a request the seeded page must keep open, so the static render stays on screen. */
const NEVER = Symbol("never");

/** The canned answers the interception serves, per page. */
const ANSWERS = {
  landing: {
    // The probe never answers: the landmark (title, body, call to action) is what a visitor
    // sees before any decision is made, and it is what this shot captures.
    "bots/list": NEVER,
  },
  fil: {
    "bots/get": SEED_BOT,
    "threads/messages": { messages: SEED_MANY_MESSAGES },
    "threads/head": HEAD,
    "threads/markRead": { ...HEAD },
    "bots/list": ROSTER,
    "threads/snooze": { threadId: "thr_golden", snoozedUntil: null },
    "threads/unsnooze": { threadId: "thr_golden", snoozedUntil: null },
  },
  groupe: {
    "groups/get": GROUP,
    "groups/list": {
      groups: [
        {
          id: "group_golden",
          name: "Triage",
          threadId: "thr_group_golden",
          ownerBotId: "bot_analyst",
          memberCount: 2,
          createdAt: "2026-01-01T00:00:00Z",
        },
      ],
    },
    "groups/messages": { messages: GROUP_MESSAGES },
    "bots/list": ROSTER,
    "threads/send": {
      messageId: "msg_sent",
      runId: "run_sent",
      status: "queued",
      runIds: ["run_sent"],
    },
  },
  activite: {
    "runs/list": { runs: ACTIVITY_RUNS },
    "runs/retry": { ok: true },
    "bots/list": ROSTER,
    "groups/list": {
      groups: [
        {
          id: "group_golden",
          name: "Triage",
          threadId: "thr_group_golden",
          ownerBotId: "bot_analyst",
          memberCount: 2,
          createdAt: "2026-01-01T00:00:00Z",
        },
      ],
    },
  },
};

const PAGES = [
  { key: "landing", path: "/", file: "apercu-landing.png" },
  { key: "fil", path: "/app/bot_golden/", file: "apercu-fil.png" },
  { key: "groupe", path: "/app/g/group_golden/", file: "apercu-groupe.png" },
  { key: "activite", path: "/app/activity/", file: "apercu-activite.png" },
];

function serve() {
  const server = spawn("python3", [
    "-m",
    "http.server",
    String(PORT),
    "--bind",
    "127.0.0.1",
    "--directory",
    OUT_DIR,
  ], { stdio: "ignore" });
  return server;
}

async function waitForServer() {
  for (let attempt = 0; attempt < 50; attempt += 1) {
    try {
      const response = await fetch(`${BASE}/index.html`);
      if (response.ok) {
        return;
      }
    } catch {
      // not up yet
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error("the static server never came up");
}

(async () => {
  const server = serve();
  try {
    await waitForServer();
    const { chromium } = require("playwright");
    const browser = await chromium.launch();
    for (const { key, path: pagePath, file } of PAGES) {
      const context = await browser.newContext({
        viewport: { width: 1440, height: 900 },
        deviceScaleFactor: 2,
        colorScheme: "dark",
        reducedMotion: "reduce",
        serviceWorkers: "block",
      });
      const page = await context.newPage();
      const answers = ANSWERS[key];
      await page.route("**/rpc/**", async (route) => {
        const procedure = new URL(route.request().url()).pathname.replace(/^\/rpc\//, "");
        const answer = answers[procedure];
        if (answer === NEVER) {
          // Leave the request hanging: the probe decides after the shot is taken.
          return;
        }
        await route.fulfill({
          status: answer === undefined ? 404 : 200,
          contentType: "application/json",
          body: JSON.stringify(
            answer ?? { code: "NOT_FOUND", error: "Not found." },
          ),
        });
      });
      await page.goto(`${BASE}${pagePath}`, { waitUntil: "domcontentloaded" });
      // Let hydration, fonts and the first frames settle before the shutter fires.
      await page.waitForTimeout(900);
      console.log(`capturing ${file}`);
      await page.screenshot({
        path: `${SHOTS_DIR}/${file}`,
        fullPage: false,
      });
      await context.close();
    }
    await browser.close();
  } finally {
    server.kill();
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
