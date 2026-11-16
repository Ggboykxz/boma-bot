//! The Boma Bot desktop shell (REQ-055).
//!
//! Thin by design: the product is the web app at the deployment origin, and this shell only
//! owns what a browser cannot — one isolated session partition per device, native menu
//! affordances (sign out, local stack), and, when the owner wires signing, the update feed.
//!
//! What it does NOT do: proxy the API, inject Node into the renderer, write to disk outside
//! Electron's own profile directories, or talk to any network but the deployment's and the
//! owner's update feed.

import { app, BrowserWindow, Menu, dialog, session, shell } from "electron";
import { autoUpdater } from "electron-updater";
import { spawn, spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import path from "node:path";

/** Where the app points: loopback by default, any origin the operator chose at package time. */
const DEPLOYMENT_URL = process.env.BOMA_DESKTOP_URL ?? "http://127.0.0.1:3000";

/** The release page, for the case where no local stack can be started. */
const RELEASES_URL = "https://github.com/Ggboykxz/boma-bot/releases/latest";

/** Whether the deployment answers a plain health check. */
async function isDeploymentReachable(url: string): Promise<boolean> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 2_000);
  try {
    const response = await fetch(`${url}/health`, { signal: controller.signal });
    return response.ok;
  } catch {
    return false;
  } finally {
    clearTimeout(timer);
  }
}

/** Whether the docker CLI exists on this PATH. */
function dockerAvailable(): boolean {
  try {
    return spawnSync("docker", ["--version"], { stdio: "ignore" }).status === 0;
  } catch {
    return false;
  }
}

/** The compose file to drive, in preference order; `undefined` when none is found. */
function findComposeFile(): string | undefined {
  const fromEnv = process.env.BOMA_COMPOSE_FILE;
  if (fromEnv && pathExists(fromEnv)) {
    return fromEnv;
  }
  const fromCwd = path.join(process.cwd(), "compose.yaml");
  if (pathExists(fromCwd)) {
    return fromCwd;
  }
  const fromHere = path.join(__dirname, "..", "..", "compose.yaml");
  if (pathExists(fromHere)) {
    return fromHere;
  }
  return undefined;
}

function pathExists(candidate: string): boolean {
  return existsSync(candidate);
}

/** Starts the local stack and waits for readiness; the loopback-only, no-account posture. */
async function startLocalStack(): Promise<boolean> {
  const compose = findComposeFile();
  if (!compose || !dockerAvailable()) {
    return false;
  }
  const child = spawn(
    "docker",
    ["compose", "-f", compose, "up", "-d"],
    {
      cwd: path.dirname(compose),
      env: { ...process.env, BOMA_LOCAL_FIRST: "true", BOMA_BIND_ADDR: "127.0.0.1:3000" },
    },
  );
  const code: number | null = await new Promise((resolve) => {
    child.on("exit", resolve);
  });
  if (code !== 0) {
    return false;
  }
  for (let attempt = 0; attempt < 180; attempt += 1) {
    if (await isDeploymentReachable(DEPLOYMENT_URL)) {
      return true;
    }
    await new Promise((resolve) => setTimeout(resolve, 1_000));
  }
  return false;
}

/** The one partition this app owns; nothing else in Electron shares it. */
const DESKTOP_PARTITION = "persist:boma";

/** Whether the signed update feed exists; off until the owner provisions one (ADR-016, DEC). */
const SIGNED_UPDATE = process.env.BOMA_DESKTOP_SIGNED === "true";

function makeWindow(): BrowserWindow {
  const partition = session.fromPartition(DESKTOP_PARTITION, { cache: true });
  const window = new BrowserWindow({
    width: 1280,
    height: 840,
    minWidth: 800,
    minHeight: 560,
    title: "Boma Bot",
    icon: path.join(__dirname, "..", "build", "icon.png"),
    webPreferences: {
      partition: DESKTOP_PARTITION,
      contextIsolation: true,
      sandbox: true,
      nodeIntegration: false,
      // No spellcheck prompts injecting nothing in; the partition keeps cookies and storage
      // isolated from anything else the OS runs in a browser.
      spellcheck: false,
    },
  });

  // New windows open in the system browser; the shell never hosts two Boma Bot windows over
  // one session.
  window.webContents.setWindowOpenHandler(({ url }) => {
    if (url.startsWith("https://") || url.startsWith("http://")) {
      void shell.openExternal(url);
    }
    return { action: "deny" };
  });

  // Navigation stays on the deployment origin; anything else goes to the browser.
  window.webContents.on("will-navigate", (event, url) => {
    if (!url.startsWith(DEPLOYMENT_URL)) {
      event.preventDefault();
      void shell.openExternal(url);
    }
  });

  return window;
}

async function signOut(window: BrowserWindow): Promise<void> {
  const partition = session.fromPartition(DESKTOP_PARTITION, { cache: true });
  await partition.clearStorageData({
    storages: ["cookies", "localstorage", "indexdb", "filesystem", "cachestorage"],
  });
  // The preload surface does not exist today; a reload on a cleared partition is signed out.
  window.reload();
}

async function runLocalStack(window: BrowserWindow): Promise<void> {
  const started = await startLocalStack();
  if (!started) {
    dialog.showMessageBox(window, {
      type: "warning",
      buttons: ["OK"],
      defaultId: 0,
      title: "Local stack",
      message: "docker compose could not start the local stack.",
      detail:
        "Is docker running? If so, run `docker compose up -d` in the repository yourself — the stack then answers on http://127.0.0.1:3000.",
    });
    return;
  }
  dialog.showMessageBox(window, {
    type: "info",
    buttons: ["OK"],
    defaultId: 0,
    title: "Local stack",
    message: "The stack is up; the app will reconnect to it.",
  });
  window.reload();
}

function template(window: BrowserWindow): Electron.MenuItemConstructorOptions[] {
  return [
    {
      label: "File",
      submenu: [
        {
          label: "Sign out",
          accelerator: "CmdOrCtrl+Shift+Q",
          click: () => {
            void signOut(window);
          },
        },
        { type: "separator" },
        {
          label: "Restart",
          accelerator: "CmdOrCtrl+R",
          click: () => {
            window.reload();
          },
        },
        { type: "separator" },
        { role: "quit" },
      ],
    },
    {
      label: "Stack",
      submenu: [
        {
          label: "Run a local stack…",
          click: () => {
            void runLocalStack(window);
          },
        },
      ],
    },
    {
      label: "View",
      submenu: [
        { role: "reload" },
        { role: "forceReload" },
        { type: "separator" },
        { role: "resetZoom" },
        { role: "zoomIn" },
        { role: "zoomOut" },
        { type: "separator" },
        { role: "togglefullscreen" },
      ],
    },
  ];
}

function enableSignedUpdateFeed(): void {
  if (!SIGNED_UPDATE) {
    // Unsigned auto-update is remote code execution; no silent path is offered at all.
    return;
  }
  autoUpdater.setFeedURL({ provider: "github", owner: "Ggboykxz", repo: "boma-bot" });
  autoUpdater.autoDownload = true;
  autoUpdater.autoInstallOnAppQuit = true;
  autoUpdater.checkForUpdatesAndNotify().catch(() => {
    // A failed check is not fatal; the restart cycle handles the next one.
  });
}

app.whenReady().then(() => {
  const window = makeWindow();
  Menu.setApplicationMenu(Menu.buildFromTemplate(template(window)));
  if (SIGNED_UPDATE) {
    enableSignedUpdateFeed();
  }

  (async () => {
    if (await isDeploymentReachable(DEPLOYMENT_URL)) {
      await window.loadURL(DEPLOYMENT_URL).catch(() => app.quit());
      return;
    }
    // First launch on a fresh device: offer the one-click local stack before erroring out.
    const start = await startLocalStack();
    if (start) {
      await window.loadURL(DEPLOYMENT_URL).catch(() => app.quit());
      return;
    }
    // Still nothing: say exactly what to do, in one screen, with the two supported paths.
    const { response } = await dialog.showMessageBox(window, {
      type: "warning",
      buttons: ["Start local stack", "Open downloads", "Quit"],
      defaultId: 0,
      title: "Boma Bot is not reachable",
      message: `${DEPLOYMENT_URL} did not answer.`,
      detail:
        "This app is the window onto a deployment, not the deployment itself.\n\n" +
        "Run a local stack (one click, entirely on this machine), or install the server bundle and set BOMA_DESKTOP_URL to its origin.\n\n" +
        "Downloads: https://github.com/Ggboykxz/boma-bot/releases/latest",
    });
    if (response === 0) {
      // Retry the one-click path; a docker daemon may have needed the nudge.
      const ok = await startLocalStack();
      if (ok) {
        await window.loadURL(DEPLOYMENT_URL).catch(() => app.quit());
        return;
      }
    } else if (response === 1) {
      await shell.openExternal(RELEASES_URL);
    }
    app.quit();
  })().catch(() => app.quit());

  app.on("activate", () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      Menu.setApplicationMenu(Menu.buildFromTemplate(template(makeWindow())));
    }
  });
});

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") {
    app.quit();
  }
});
