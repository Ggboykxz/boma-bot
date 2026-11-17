// Windows packaged-app boot test (M35). Drives the portable build on a real Windows runner:
// start → embedded Postgres → migrate → serve + worker → RPC round-trip → screenshots → logs.
// Run from apps/web with:
//   node tools/windows-boot-smoke.mjs <path-to-boma-desktop.exe>
//
// The app owns its userData under %APPDATA%/Boma Bot/boma/. On a failure, this script prints the
// tails of boma.log and postgres.log and exits non-zero — CI keeps both as artifacts.

import { spawn } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { chromium } = require("@playwright/test");
const AxeBuilder = require("@axe-core/playwright").default;

const exe = process.argv[2];
if (!exe || !existsSync(exe)) {
  console.error(`usage: node tools/windows-boot-smoke.mjs <boma-desktop.exe>`);
  process.exit(2);
}

const userData = path.join(process.env.APPDATA ?? "", "Boma Bot", "boma");
const portFile = path.join(userData, "port");
const bomaLog = path.join(userData, "boma.log");
const pgLog = path.join(userData, "postgres.log");
const shots = path.resolve("test-results/windows-boot");
mkdirSync(shots, { recursive: true });

// A previous run on a self-hosted runner might have left a cluster behind.
if (existsSync(userData)) {
  console.log(`clearing previous userData: ${userData}`);
  // Windows refuses to delete open files; the runner is fresh in CI. Only clear the port file.
  try {
    const { rmSync } = await import("node:fs");
    rmSync(userData, { recursive: true, force: true });
  } catch (error) {
    console.warn(`could not clear userData: ${String(error)}`);
  }
}

function tail(file, lines = 40) {
  try {
    const all = readFileSync(file, "utf8").split("\n");
    return all.slice(Math.max(0, all.length - lines)).join("\n");
  } catch {
    return "(no log)";
  }
}

function reportLogs() {
  writeFileSync(path.join(shots, "boma.log.txt"), tail(bomaLog, 400));
  writeFileSync(path.join(shots, "postgres.log.txt"), tail(pgLog, 400));
  console.log("--- boma.log tail ---\n" + tail(bomaLog));
  console.log("--- postgres.log tail ---\n" + tail(pgLog));
}

const child = spawn(exe, [], { stdio: ["ignore", "pipe", "pipe"], detached: false });
let appOutput = "";
for (const stream of [child.stdout, child.stderr]) {
  stream?.on("data", (chunk) => {
    appOutput += chunk.toString();
  });
}
child.on("exit", (code) => {
  console.log(`the desktop exited early (code ${code}):\n${appOutput}`);
});

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function readPort() {
  try {
    const port = Number(readFileSync(portFile, "utf8").trim());
    return Number.isInteger(port) && port > 0 ? port : null;
  } catch {
    return null;
  }
}

async function waitForPort(timeoutMs = 150_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const port = await readPort();
    if (port) {
      return port;
    }
    if (appOutput.includes("migration did not apply") || tail(bomaLog, 200).includes("migration")) {
      // Let the dialog settle a beat, then dump logs.
      await sleep(500);
      throw new Error(`the desktop reported a migration failure; see the log tails below`);
    }
    await sleep(1_000);
  }
  throw new Error(`no port file after ${timeoutMs} ms; userData entries: ${
    existsSync(userData) ? readdirSync(userData).join(", ") : "(none)"
  }`);
}

let failed = null;
try {
  const port = await waitForPort();
  const base = `http://127.0.0.1:${port}`;
  console.log(`the app is serving on ${base}`);

  for (const route of ["/health", "/ready", "/api/auth/capabilities"]) {
    const response = await fetch(`${base}${route}`);
    if (!response.ok) {
      throw new Error(`${route}: ${response.status}`);
    }
  }
  const caps = await (await fetch(`${base}/api/auth/capabilities`)).json();
  if (caps.localFirst !== true) {
    throw new Error(`localFirst is ${caps.localFirst}, expected true`);
  }

  // A real turn through the API — the server half of the same use case the UI calls.
  const roster = await (await fetch(`${base}/rpc/bots/list`)).json();
  let bot = roster.bots?.[0];
  if (!bot) {
    const created = await fetch(`${base}/rpc/bots/create`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ name: "Smoke Bot", description: "Windows boot smoke" }),
    });
    if (!created.ok) {
      throw new Error(`bots/create: ${created.status}`);
    }
    bot = await created.json();
  }
  const sent = await fetch(`${base}/rpc/threads/send`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ botId: bot.id, text: "Windows boot smoke", clientNonce: `win-${Date.now()}` }),
  });
  if (!sent.ok) {
    throw new Error(`threads/send: ${sent.status}`);
  }

  // The renderer: load the app and capture the real state, not a mocked fixture.
  const browser = await chromium.launch();
  const context = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(String(error)));

  for (const [route, shot] of [
    ["/app/", "threads.png"],
    [`/app/${bot.id}/`, "room.png"],
  ]) {
    await page.goto(`${base}${route}`, { waitUntil: "load" });
    await page.waitForTimeout(800);
    await page.screenshot({ path: path.join(shots, shot), fullPage: true });
    const report = await new AxeBuilder({ page })
      .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"])
      .analyze();
    const serious = report.violations
      .filter((v) => v.impact === "critical" || v.impact === "serious")
      .map((v) => `${v.id} × ${v.nodes.length}`);
    if (serious.length > 0) {
      throw new Error(`${route}: axe ${serious.join(", ")}`);
    }
  }

  // A real turn from the UI, not just the API: type in the composer and send.
  const composer = page.getByRole("textbox", { name: "Message" });
  await composer.fill("Windows packaged boot: typed from the renderer");
  await composer.press("Enter");
  await page.waitForTimeout(2_000);
  await page.screenshot({ path: path.join(shots, "room-after-send.png"), fullPage: true });
  if (errors.length > 0) {
    throw new Error(`renderer errors:\n${errors.join("\n")}`);
  }
  await browser.close();
  console.log("Windows packaged boot: OK (health, ready, capabilities, RPC, renderer, axe, send)");
} catch (error) {
  failed = error;
} finally {
  reportLogs();
  try {
    process.kill(child.pid, "SIGTERM");
  } catch {
    // Already gone.
  }
}

if (failed) {
  console.error(`FAILED: ${String(failed)}`);
  process.exit(1);
}
