//! The Boma Bot desktop shell (REQ-055).
//!
//! Thin by design: the product is the web app at the deployment origin. On this machine,
//! with no BOMA_DESKTOP_URL set, that origin is the embedded stack this app itself starts:
//! Postgres + migrations + the server and the worker, all inside the package (M35). With
//! BOMA_DESKTOP_URL set, the shell is a window onto that origin instead — the "your own
//! server" path — and the embedded stack is not started.
//!
//! What it does NOT do: proxy the API, inject Node into the renderer, write to disk outside
//! Electron's own profile directories, or talk to any network but the deployment's and the
//! owner's update feed.

import { app, BrowserWindow, Menu, dialog, session, shell } from "electron";
import { autoUpdater } from "electron-updater";
import path from "node:path";
import { appendFileSync, mkdirSync } from "node:fs";
import {
  startStack,
  stopStack,
  StackError,
  type StackHandle,
} from "./stack";

/** The release origin, when the operator pinned one. Unset means the embedded stack. */
const OVERRIDE_URL = process.env.BOMA_DESKTOP_URL;

/** Whether the signed update feed exists; off until the owner provisions one (ADR-016, DEC). */
const SIGNED_UPDATE = process.env.BOMA_DESKTOP_SIGNED === "true";

/** The one partition this app owns; nothing else in Electron shares it. */
const DESKTOP_PARTITION = "persist:boma";

/** The stack the window is bound to, when embedded mode is live. */
let liveStack: StackHandle | null = null;

/** Navigation must stay on the deployment origin; anything else goes to the browser. */
function guardNavigation(window: BrowserWindow, origin: string): void {
  window.webContents.setWindowOpenHandler(({ url }) => {
    if (url.startsWith("https://") || url.startsWith("http://")) {
      void shell.openExternal(url);
    }
    return { action: "deny" };
  });
  window.webContents.on("will-navigate", (event, url) => {
    if (!url.startsWith(origin)) {
      event.preventDefault();
      void shell.openExternal(url);
    }
  });
}

function makeWindow(): BrowserWindow {
  const partition = session.fromPartition(DESKTOP_PARTITION, { cache: true });
  const window = new BrowserWindow({
    width: 1280,
    height: 840,
    minWidth: 800,
    minHeight: 560,
    title: "Boma Bot",
    // No window icon here: electron-builder embeds it in the executable, and the compiled
    // `build/` assets are not packed into the asar. BrowserWindow would warn about a missing
    // file on every launch.
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
  return window;
}

/** The data folder the user can delete to reset the deployment. */
function dataFolder(): string {
  return path.join(app.getPath("userData"), "boma");
}

/**
 * Append a boot checkpoint to `boot.log` in the data folder. The bring-up runs before anything
 * is listening, so when a machine never reaches a healthy stack this file is the only record of
 * where it stopped. Best-effort by design: diagnostics never take the boot down.
 */
function checkpoint(name: string): void {
  try {
    const dir = dataFolder();
    mkdirSync(dir, { recursive: true });
    appendFileSync(path.join(dir, "boot.log"), `${new Date().toISOString()} ${name}\n`);
  } catch {
    // Ignore: a missing boot.log must not fail the launch.
  }
}

/** Where to read the failure, for a dialog or a support request. */
function logHint(): string {
  return `Logs: ${path.join(dataFolder(), "boma.log")} and ${path.join(dataFolder(), "postgres.log")}`;
}

function resetHint(): string {
  return `Delete \`${dataFolder()}\` and relaunch to reset the deployment.`;
}

/** A fatal, user-visible failure with a single button: quit. */
async function fatalAndQuit(window: BrowserWindow, message: string): Promise<void> {
  await stopStack(app.getPath("userData"));
  await dialog.showMessageBox(window, {
    type: "error",
    buttons: ["Quit"],
    defaultId: 0,
    title: "Boma Bot cannot start",
    message,
    detail: `${logHint()}\n\n${resetHint()}`,
  });
  app.quit();
}

/** The first-launch flow, with one retry owned by the user. */
async function boot(window: BrowserWindow): Promise<void> {
  checkpoint("boot start");
  if (OVERRIDE_URL) {
    if (await isReachable(OVERRIDE_URL)) {
      guardNavigation(window, OVERRIDE_URL);
      await window.loadURL(OVERRIDE_URL).catch(() => app.quit());
      return;
    }
    await fatalAndQuit(window, `${OVERRIDE_URL} did not answer.`);
    return;
  }

  try {
    const handle = await startEmbedded(window);
    checkpoint("embedded stack up");
    guardNavigation(window, handle.url);
    handle.on("fatal", (reason: string) => {
      void fatalAndQuit(window, reason);
    });
    liveStack = handle;
    await window.loadURL(handle.url).catch(() => app.quit());
  } catch (error) {
    checkpoint(`fatal: ${error instanceof Error ? error.message : String(error)}`);
    if (
      error instanceof StackError ||
      (error instanceof Error && "step" in error)
    ) {
      const stackError = error as StackError;
      const { response } = await dialog.showMessageBox(window, {
        type: "error",
        buttons: ["Try again", "Quit"],
        defaultId: 0,
        title: "Boma Bot cannot start",
        message: stackError.message,
        detail: `The failure was in ${stackError.step}.\n\n${stackError.log ?? ""}\n\n${logHint()}\n\n${resetHint()}`,
      });
      if (response === 0) {
        // One retry, user-driven.
        try {
          const handle = await startStack(app.getPath("userData"));
          guardNavigation(window, handle.url);
          handle.on("fatal", (reason: string) => {
            void fatalAndQuit(window, reason);
          });
          liveStack = handle;
          await window.loadURL(handle.url).catch(() => app.quit());
          return;
        } catch (retryError) {
          const stackError2 = retryError as StackError;
          await fatalAndQuit(
            window,
            `${stackError2.message} (failed again in ${stackError2.step})`,
          );
          return;
        }
      }
      app.quit();
      return;
    }
    await fatalAndQuit(window, String(error));
  }
}

async function startEmbedded(window: BrowserWindow): Promise<StackHandle> {
  return startStack(app.getPath("userData"));
}

async function isReachable(url: string): Promise<boolean> {
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

/** Stop the embedded stack, if any; idempotent. */
let stackStopped = false;
async function stopStackOnce(): Promise<void> {
  if (stackStopped) {
    return;
  }
  stackStopped = true;
  await stopStack(app.getPath("userData"));
}

/** Restart the embedded stack and reload the window (the Stack menu). */
async function restartStack(window: BrowserWindow): Promise<void> {
  await stopStackOnce();
  stackStopped = false;
  try {
    const handle = await startStack(app.getPath("userData"));
    guardNavigation(window, handle.url);
    handle.on("fatal", (reason: string) => {
      void fatalAndQuit(window, reason);
    });
    liveStack = handle;
    await window.loadURL(handle.url);
  } catch (error) {
    await fatalAndQuit(window, error instanceof StackError ? error.message : String(error));
  }
}

async function signOut(window: BrowserWindow): Promise<void> {
  const partition = session.fromPartition(DESKTOP_PARTITION, { cache: true });
  await partition.clearStorageData({
    storages: ["cookies", "localstorage", "indexdb", "filesystem", "cachestorage"],
  });
  // The preload surface does not exist today; a reload on a cleared partition is signed out.
  window.reload();
}

function template(window: BrowserWindow): Electron.MenuItemConstructorOptions[] {
  const canRestartStack = !OVERRIDE_URL;
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
          label: "Restart the local stack",
          enabled: canRestartStack,
          click: () => {
            void restartStack(window);
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

// Headless or broken-GPU machines (CI runners, some remote-desktop sessions) cannot create a
// GPU-accelerated window; BOMA_DESKTOP_DISABLE_GPU=1 forces software rendering. Off otherwise:
// real machines keep hardware acceleration.
if (process.env.BOMA_DESKTOP_DISABLE_GPU === "1") {
  app.disableHardwareAcceleration();
}

app.whenReady().then(() => {
  checkpoint("whenReady");
  const window = makeWindow();
  checkpoint("window created");
  Menu.setApplicationMenu(Menu.buildFromTemplate(template(window)));
  checkpoint("menu set");
  if (SIGNED_UPDATE) {
    enableSignedUpdateFeed();
  }
  void boot(window);
  checkpoint("boot dispatched");

  app.on("activate", () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      Menu.setApplicationMenu(Menu.buildFromTemplate(template(makeWindow())));
    }
  });
});

// The window never outlives the data: stop the stack on every way out.
let quitting = false;
app.on("before-quit", (event) => {
  if (quitting) {
    return;
  }
  quitting = true;
  event.preventDefault();
  void stopStackOnce().finally(() => app.quit());
});

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") {
    app.quit();
  }
});
