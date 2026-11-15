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
import { spawn } from "node:child_process";
import path from "node:path";

/** Where the app points: loopback by default, any origin the operator chose at package time. */
const DEPLOYMENT_URL = process.env.BOMA_DESKTOP_URL ?? "http://127.0.0.1:3000";

/** The one partition this app owns; nothing else in Electron shares it. */
const DESKTOP_PARTITION = "persist:boma";

/** The path of the compose file the "run a local stack" action drives. */
const COMPOSE_FILE =
  process.env.BOMA_COMPOSE_FILE ?? path.join(process.cwd(), "compose.yaml");

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
  const { response } = await dialog.showMessageBox(window, {
    type: "question",
    buttons: ["Cancel", "Run it"],
    defaultId: 1,
    title: "Run a local stack",
    message: "Start the local Compose stack with docker compose?",
    detail: `This runs: docker compose -f ${COMPOSE_FILE} up -d\n\nIt will pull images and build the binary once. Nothing is uploaded to the network.`,
  });
  if (response !== 1) {
    return;
  }
  const child = spawn("docker", ["compose", "-f", COMPOSE_FILE, "up", "-d"], {
    cwd: path.dirname(COMPOSE_FILE),
    env: { ...process.env },
  });
  child.stderr.on("data", () => {
    // The docker CLI reports progress on stderr; the exit code is the signal.
  });
  child.on("exit", (code) => {
    dialog.showMessageBox(window, {
      type: code === 0 ? "info" : "warning",
      buttons: ["OK"],
      title: "Local stack",
      message: code === 0 ? "The stack is up; the app will reconnect to it." : `docker compose exited with ${code}. See the logs.`,
    });
    window.reload();
  });
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

  window.loadURL(DEPLOYMENT_URL).catch(() => {
    // The loopback origin may be down on first launch (fresh device, stack never started):
    // say so plainly instead of a blank window.
    dialog.showMessageBox(window, {
      type: "warning",
      buttons: ["Retry", "Quit"],
      defaultId: 0,
      title: "Boma Bot is not reachable",
      message: `${DEPLOYMENT_URL} did not answer.`,
      detail: "Start the deployment (docker compose up -d) or set BOMA_DESKTOP_URL to the right origin, then relaunch.",
    }).then(({ response }) => {
      if (response === 1) {
        app.quit();
        return;
      }
      window.loadURL(DEPLOYMENT_URL).catch(() => app.quit());
    });
  });

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
