#!/usr/bin/env node
// UI/UX + motion audit harness (2026-11-16). One-shot, re-runnable against a local stack.
//
//   BOMA_AUDIT_BASE=http://127.0.0.1:3010 node tools/ui-audit.mjs
//
// Walks every route of the static export served by the Rust binary (BOMA_WEB_ROOT, REQ-055),
// records console/pageerror/request failures, runs axe-core, screenshots full pages, and probes
// motion behaviour (getAnimations, prefers-reduced-motion), focus visibility and CLS/LCP.
// Output: docs/audits/2026-11-16/ui-audit-results.json + docs/screenshots/ui-audit-2026-11-16/.
// Nothing here modifies the product; it measures it.

import { createRequire } from "node:module";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const webRequire = createRequire(resolve(root, "apps/web/package.json"));
const { chromium } = webRequire("@playwright/test");
const axeSource = readFileSync(resolve(root, "apps/web/node_modules/axe-core/axe.min.js"), "utf8");

const BASE = process.env.BOMA_AUDIT_BASE ?? "http://127.0.0.1:3010";
const BOT_ID = process.env.BOMA_AUDIT_BOT_ID ?? "bot_YXyOa6eMBbbvbzg5xbqkIzttKQ";
const SHOT_DIR = resolve(process.env.BOMA_AUDIT_SHOT_DIR ?? resolve(root, "docs/screenshots/ui-audit-2026-11-16"));
const OUT_JSON = resolve(process.env.BOMA_AUDIT_OUT ?? resolve(root, "docs/audits/2026-11-16/ui-audit-results.json"));

mkdirSync(dirname(OUT_JSON), { recursive: true });
mkdirSync(SHOT_DIR, { recursive: true });

const routes = [
  ["landing", "/"],
  ["sign-in", "/sign-in"],
  ["sign-up", "/sign-up"],
  ["forgot-password", "/forgot-password"],
  ["reset-password", "/reset-password"],
  ["onboarding", "/onboarding"],
  ["onboarding-1", "/onboarding/1"],
  ["onboarding-2", "/onboarding/2"],
  ["onboarding-3", "/onboarding/3"],
  ["invite", "/invite"],
  ["threads-home", "/app/"],
  ["room", `/app/${BOT_ID}`],
  ["artifacts", "/app/artifacts"],
  ["board", "/app/board"],
  ["activity", "/app/activity"],
  ["admin-access", "/admin/access"],
  ["admin-plugins", "/admin/plugins"],
  ["integrations-setup", "/integrations/setup"],
  ["shared-page-valid", "/p/token_golden"],
  ["shared-page-invalid", "/p/token_unknown"],
  ["not-found", "/definitely-not-a-page"],
];

const benignRequest = (url) =>
  /threads\/subscribe/.test(url); // SSE long-poll: its abort on navigation is normal

const results = {
  generatedAt: new Date().toISOString(),
  base: BASE,
  botId: BOT_ID,
  routes: [],
  motion: {},
  focus: {},
  responsiveness: {},
  performance: {},
};

const browser = await chromium.launch({
  executablePath:
    process.env.BOMA_AUDIT_CHROMIUM ??
    "/nix/store/0da9661p5xsbjyrnidgkksmiy3s4xh6v-chromium-unwrapped-143.0.7499.146/libexec/chromium/chromium",
  args: ["--no-sandbox", "--disable-gpu", "--disable-dev-shm-usage", "--no-first-run"],
});

const ROUTE_FILTER = process.env.BOMA_AUDIT_ROUTES?.split(",").map((s) => s.trim()).filter(Boolean);

for (const [slug, route] of routes) {
  if (ROUTE_FILTER && !ROUTE_FILTER.includes(slug)) continue;
  const context = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const page = await context.newPage();
  const consoleMsgs = [];
  const pageErrors = [];
  const failedRequests = [];
  const badResponses = [];
  page.on("console", (msg) => {
    if (msg.type() === "error" || msg.type() === "warning") {
      consoleMsgs.push({ type: msg.type(), text: msg.text().slice(0, 300), location: msg.location()?.url ?? null });
    }
  });
  page.on("pageerror", (error) => pageErrors.push(String(error).slice(0, 300)));
  page.on("requestfailed", (request) => {
    const url = request.url();
    const failure = request.failure()?.errorText ?? "unknown";
    if (!benignRequest(url)) failedRequests.push({ url, failure, resourceType: request.resourceType() });
  });
  page.on("response", (response) => {
    const url = response.url();
    if (response.status() >= 400) badResponses.push({ url, status: response.status() });
  });

  let status = 0;
  let navError = null;
  try {
    const response = await page.goto(BASE + route, { waitUntil: "load", timeout: 30_000 });
    status = response?.status() ?? 0;
    await page.waitForTimeout(1800);
  } catch (error) {
    navError = String(error).slice(0, 200);
  }

  const screenshot = `${slug}.png`;
  await page.screenshot({ path: resolve(SHOT_DIR, screenshot), fullPage: true }).catch(() => {});

  // axe-core
  let axe = null;
  try {
    await page.addScriptTag({ content: axeSource });
    axe = await page.evaluate(async () => {
      // eslint-disable-next-line no-undef
      const report = await window.axe.run(document, { resultTypes: ["violations"] });
      return report.violations.map((v) => ({
        id: v.id,
        impact: v.impact,
        help: v.help,
        nodes: v.nodes.length,
        targets: v.nodes.slice(0, 4).map((n) => n.target),
      }));
    });
  } catch (error) {
    axe = [{ id: "axe-failed", impact: null, help: String(error).slice(0, 160), nodes: 0, targets: [] }];
  }

  results.routes.push({
    slug,
    route,
    status,
    navError,
    screenshot,
    console: consoleMsgs,
    pageerrors: pageErrors,
    failedRequests,
    badResponses,
    axeViolations: axe,
  });
  await context.close();
  console.log(`done ${slug}`);
}

// --- Motion probes (landing + room), normal vs reduced motion ---
// getAnimations() only sees animations that exist *right now*; entrance animations may already be
// over by the time we ask. So the recorder starts in an init script and samples every 50 ms from
// document start for 4 s, accumulating what it saw.
async function motionProbe(path, reduced) {
  const context = await browser.newContext(
    reduced ? { viewport: { width: 1280, height: 800 }, reducedMotion: "reduce" } : { viewport: { width: 1280, height: 800 } },
  );
  const page = await context.newPage();
  await page.addInitScript(() => {
    window.__motionLog = [];
    window.__reducedMotionMatch = matchMedia("(prefers-reduced-motion: reduce)").matches;
    const seen = new Set();
    const tick = () => {
      for (const a of document.getAnimations({ subtree: true })) {
        const timing = a.effect?.getComputedTiming?.() ?? {};
        const target = a.effect?.target
          ? `${a.effect.target.tagName.toLowerCase()}.${String(a.effect.target.className).slice(0, 70)}`
          : "";
        const signature = JSON.stringify([
          a.constructor.name,
          a.animationName ?? a.transitionProperty ?? null,
          timing.duration,
          timing.delay,
          target,
        ]);
        if (seen.has(signature)) continue;
        seen.add(signature);
        window.__motionLog.push({
          kind: a.constructor.name,
          name: a.animationName ?? a.transitionProperty ?? null,
          duration: typeof timing.duration === "number" ? timing.duration : String(timing.duration ?? "auto"),
          delay: timing.delay ?? null,
          endTime: typeof timing.endTime === "number" ? timing.endTime : null,
          playState: a.playState,
          target: target.slice(0, 100),
        });
      }
    };
    const iv = setInterval(tick, 50);
    setTimeout(() => clearInterval(iv), 4000);
  });
  await page.goto(BASE + path, { waitUntil: "load", timeout: 30_000 });
  await page.waitForTimeout(4500);
  const anims = await page.evaluate(() => ({ entries: window.__motionLog, reducedMatches: window.__reducedMotionMatch }));
  await context.close();
  return anims;
}

results.motion.landing = await motionProbe("/", false);
results.motion.landingReduced = await motionProbe("/", true);
results.motion.room = await motionProbe(`/app/${BOT_ID}`, false);
results.motion.roomReduced = await motionProbe(`/app/${BOT_ID}`, true);

// --- Focus visibility walk on /sign-in ---
{
  const context = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const page = await context.newPage();
  await page.goto(BASE + "/sign-in", { waitUntil: "load" });
  await page.waitForTimeout(800);
  const stops = [];
  for (let i = 0; i < 10; i++) {
    await page.keyboard.press("Tab");
    const stop = await page.evaluate(() => {
      const el = document.activeElement;
      if (!el || el === document.body) return null;
      const style = getComputedStyle(el);
      return {
        tag: el.tagName.toLowerCase(),
        id: el.id || null,
        role: el.getAttribute("role"),
        name: (el.getAttribute("aria-label") || el.textContent || el.value || el.getAttribute("placeholder") || "").trim().slice(0, 60),
        outline: style.outlineStyle + " " + style.outlineWidth + " " + style.outlineColor,
        outlineOffset: style.outlineOffset,
        boxShadow: style.boxShadow,
      };
    });
    stops.push(stop);
  }
  results.focus.signIn = stops;
  await context.close();
}

// --- Responsiveness: 390x844 screenshots of the three heaviest screens ---
for (const [slug, route] of [["landing-mobile", "/"], ["threads-home-mobile", "/app/"], ["room-mobile", `/app/${BOT_ID}`]]) {
  const context = await browser.newContext({ viewport: { width: 390, height: 844 } });
  const page = await context.newPage();
  await page.goto(BASE + route, { waitUntil: "load" });
  await page.waitForTimeout(1500);
  await page.screenshot({ path: resolve(SHOT_DIR, `${slug}.png`), fullPage: true });
  await context.close();
}

// --- Performance on landing: CLS + LCP ---
{
  const context = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const page = await context.newPage();
  await page.addInitScript(() => {
    window.__cls = 0;
    window.__lcp = null;
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        if (!entry.hadRecentInput) window.__cls += entry.value;
      }
    }).observe({ type: "layout-shift", buffered: true });
    new PerformanceObserver((list) => {
      const entries = list.getEntries();
      window.__lcp = entries[entries.length - 1]?.startTime ?? null;
    }).observe({ type: "largest-contentful-paint", buffered: true });
  });
  await page.goto(BASE + "/", { waitUntil: "load" });
  await page.waitForTimeout(3000);
  results.performance.landing = await page.evaluate(() => ({ cls: window.__cls, lcp: window.__lcp }));
  await context.close();
}

await browser.close();
writeFileSync(OUT_JSON, JSON.stringify(results, null, 2));
console.log(`audit written to ${OUT_JSON}`);
