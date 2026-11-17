//! The embedded local-first stack (M35): everything the app needs, in the package.
//!
//! One responsibility: bring `postgres` + `boma migrate` + `boma serve` + `boma worker` up
//! safely, supervise them, and tear them down in order. The desktop is a window onto this
//! stack — no Docker, no system Postgres, no account, no setup at install time.
//!
//! Everything lives under `<userData>/boma/`: `pgdata/`, `master-key`, `port`, `artifacts/`,
//! `serve.pid`, `worker.pid`, `postgres.log`, `boma.log`. Deleting it resets the deployment.

import {
  createServer,
  type Server,
} from "node:net";
import {
  createWriteStream,
} from "node:fs";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
  unlinkSync,
  readdirSync,
  appendFileSync,
} from "node:fs";
import { spawn, type ChildProcess } from "node:child_process";
import { EventEmitter } from "node:events";
import path from "node:path";
import { randomBytes } from "node:crypto";

/** Steps of the bring-up, for the failure dialog. */
export type StackStep =
  | "master-key"
  | "initdb"
  | "postgres"
  | "migrate"
  | "serve"
  | "worker"
  | "healthy";

/** A failure of one named step, with the tail of its log lines when available. */
export class StackError extends Error {
  constructor(
    readonly step: StackStep,
    message: string,
    readonly log?: string,
  ) {
    super(message);
    this.name = "StackError";
  }
}

/** Where everything lives. */
export interface StackPaths {
  /** `userData/boma` — the whole deployment state. */
  root: string;
  pgData: string;
  masterKey: string;
  artifacts: string;
  portFile: string;
  servePid: string;
  workerPid: string;
  postgresLog: string;
  serverLog: string;
}

/** The paths for this installation. */
export function stackPaths(userData: string): StackPaths {
  const root = path.join(userData, "boma");
  return {
    root,
    pgData: path.join(root, "pgdata"),
    masterKey: path.join(root, "master-key"),
    artifacts: path.join(root, "artifacts"),
    portFile: path.join(root, "port"),
    servePid: path.join(root, "serve.pid"),
    workerPid: path.join(root, "worker.pid"),
    postgresLog: path.join(root, "postgres.log"),
    serverLog: path.join(root, "boma.log"),
  };
}

/** Where the packer put the ingredients; overridable for rehearsals via `BOMA_STACK_RESOURCES`. */
export function resourcesRoot(): string {
  return process.env.BOMA_STACK_RESOURCES ?? process.resourcesPath;
}

function isFreePortOnLoopback(port: number): Promise<boolean> {
  return new Promise((resolve) => {
    const server: Server = createServer();
    server.once("error", () => resolve(false));
    server.once("listening", () => {
      server.close(() => resolve(true));
    });
    server.listen(port, "127.0.0.1");
  });
}

function pickFreePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const server: Server = createServer();
    server.once("error", reject);
    server.once("listening", () => {
      const address = server.address();
      server.close(() => {
        if (typeof address === "object" && address !== null) {
          resolve(address.port);
        } else {
          reject(new Error("the port picker returned nothing"));
        }
      });
    });
    server.listen(0, "127.0.0.1");
  });
}

function readPersistedPort(portFile: string): number | undefined {
  try {
    const port = Number(readFileSync(portFile, "utf8").trim());
    return Number.isInteger(port) && port > 0 && port < 65_536 ? port : undefined;
  } catch {
    return undefined;
  }
}

function persistPort(paths: StackPaths, port: number): void {
  writeFileSync(paths.portFile, `${port}\n`, { mode: 0o600 });
}

/** The last lines of a log file, for the failure dialog. */
function logTail(file: string, lines = 12): string | undefined {
  try {
    const all = readFileSync(file, "utf8").split("\n");
    const slice = all.slice(Math.max(0, all.length - lines));
    return slice.join("\n").trim() || undefined;
  } catch {
    return undefined;
  }
}

/**
 * A failure dialog is not a log: persist the bring-up failure where support (and CI) can read
 * it. Best-effort — logging must never mask the original error.
 */
function persistFailure(paths: StackPaths, step: string, detail: string | undefined): void {
  if (!detail) return;
  try {
    appendFileSync(paths.serverLog, `\nfatal: ${step}: ${detail}\n`, "utf8");
  } catch {
    // Ignore: the dialog still carries the message.
  }
}

function pidOf(pidFile: string): number | undefined {
  try {
    const pid = Number(readFileSync(pidFile, "utf8").trim());
    return Number.isInteger(pid) && pid > 0 ? pid : undefined;
  } catch {
    return undefined;
  }
}

/** Kill a pid if it exists; resolve once it is gone or the timeout elapsed. */
async function terminatePid(pid: number | undefined, graceMs: number): Promise<void> {
  if (!pid) return;
  try {
    process.kill(pid, 0);
  } catch {
    return;
  }
  try {
    process.kill(pid, "SIGTERM");
  } catch {
    return;
  }
  const deadline = Date.now() + graceMs;
  while (Date.now() < deadline) {
    try {
      process.kill(pid, 0);
      await sleep(100);
    } catch {
      return;
    }
  }
  try {
    process.kill(pid, "SIGKILL");
  } catch {
    // Already gone.
  }
}

/** Reap orphans left by an unclean exit of the previous run. */
export async function reapOrphans(paths: StackPaths): Promise<void> {
  await terminatePid(pidOf(paths.workerPid), 3_000);
  await terminatePid(pidOf(paths.servePid), 3_000);
  for (const pidFile of [paths.workerPid, paths.servePid]) {
    try {
      unlinkSync(pidFile);
    } catch {
      // Absent is fine.
    }
  }
}

/** Spawns a command, captures its combined output tail, and resolves with its exit code. */
async function run(
  file: string,
  args: readonly string[],
  options: { cwd?: string; env?: NodeJS.ProcessEnv } = {},
): Promise<{ code: number | null; output: string }> {
  return new Promise((resolve) => {
    const child: ChildProcess = spawn(file, args as string[], {
      cwd: options.cwd,
      env: options.env ? { ...process.env, ...options.env } : process.env,
      stdio: ["ignore", "pipe", "pipe"],
    });
    let output = "";
    const append = (chunk: Buffer | string) => {
      output += chunk.toString();
      const split = output.split("\n");
      if (split.length > 120) {
        output = split.slice(-120).join("\n");
      }
    };
    child.stdout?.on("data", append);
    child.stderr?.on("data", append);
    child.on("error", (error) => resolve({ code: -1, output: `${output}\n${error.message}` }));
    child.on("exit", (code) => resolve({ code, output }));
  });
}

const isWindows = process.platform === "win32";
const binaryName = (name: string) => (isWindows ? `${name}.exe` : name);
export const postgresBin = (root: string, name: string) =>
  path.join(root, "postgres", "bin", binaryName(name));
export const bomaBin = (root: string) => path.join(root, "boma", binaryName("boma"));

/**
 * Probe a deployment base origin; `/health` is appended here (and only here). Callers pass the
 * base (`http://127.0.0.1:<port>`), never a path that already ends in `/health` — a double
 * suffix probes `/health/health` forever, which is how this nearly shipped.
 */
export async function isDeploymentReachable(baseUrl: string): Promise<boolean> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 2_000);
  try {
    const response = await fetch(`${baseUrl}/health`, { signal: controller.signal });
    return response.ok;
  } catch {
    return false;
  } finally {
    clearTimeout(timer);
  }
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function openServerLog(file: string) {
  return createWriteStream(file, { flags: "a", encoding: "utf8" });
}

/** A running stack; `fatal` carries the reason when the stack dies under the app. */
export class StackHandle extends EventEmitter {
  /** The origin the window should load. */
  readonly url: string;

  constructor(readonly httpPort: number) {
    super();
    this.url = `http://127.0.0.1:${httpPort}`;
  }
}

/** Bring up the whole stack. `stopStack` tears it down in the reverse order. */
export async function startStack(userData: string): Promise<StackHandle> {
  const paths = stackPaths(userData);
  mkdirSync(paths.root, { recursive: true });
  mkdirSync(paths.artifacts, { recursive: true });
  await reapOrphans(paths);

  if (!existsSync(paths.masterKey)) {
    writeFileSync(paths.masterKey, randomBytes(32).toString("hex"), { mode: 0o600 });
  }
  const masterKey = readFileSync(paths.masterKey, "utf8").trim();

  // The cluster. (PG_VERSION is the only honest marker of a complete cluster.)
  if (!existsSync(path.join(paths.pgData, "PG_VERSION"))) {
    if (existsSync(paths.pgData) && readdirSync(paths.pgData).length > 0) {
      throw new StackError(
        "initdb",
        `the data directory ${paths.pgData} exists but is not a cluster`,
        "A crashed or interrupted first boot leaves a partial directory. Delete it (or the whole boma/ folder) and relaunch to reset the deployment.",
      );
    }
    const initdb = await run(postgresBin(resourcesRoot(), "initdb"), [
      "-D",
      paths.pgData,
      "-U",
      "boma",
      "--auth=trust",
      "--no-locale",
      "-E",
      "UTF8",
    ]);
    if (initdb.code !== 0) {
      persistFailure(paths, "initdb", initdb.output.split("\n").slice(-12).join("\n"));
      throw new StackError(
        "initdb",
        "the embedded PostgreSQL could not initialise its data directory",
        initdb.output.split("\n").slice(-12).join("\n"),
      );
    }
  }

  const pgPort = await pickFreePort();
  let httpPort = readPersistedPort(paths.portFile);
  if (httpPort !== undefined && !(await isFreePortOnLoopback(httpPort))) {
    httpPort = undefined;
  }
  httpPort ??= await pickFreePort();

  // Postgres up; pg_ctl waits for the server itself.
  const pg = await run(postgresBin(resourcesRoot(), "pg_ctl"), [
    "-D",
    paths.pgData,
    "-l",
    paths.postgresLog,
    "-o",
    `-p ${pgPort}`,
    "-w",
    "start",
  ]);
  if (pg.code !== 0) {
    persistFailure(paths, "postgres", logTail(paths.postgresLog) ?? pg.output.split("\n").slice(-12).join("\n"));
    throw new StackError(
      "postgres",
      "the embedded PostgreSQL did not start",
      logTail(paths.postgresLog) ?? pg.output.split("\n").slice(-12).join("\n"),
    );
  }

  const env = {
    BOMA_LOCAL_FIRST: "true",
    BOMA_DATABASE_URL: `postgres://boma@127.0.0.1:${pgPort}/postgres`,
    BOMA_BIND_ADDR: `127.0.0.1:${httpPort}`,
    BOMA_WEB_ROOT: path.join(resourcesRoot(), "web"),
    BOMA_MASTER_KEY: masterKey,
    BOMA_ARTIFACT_DIR: paths.artifacts,
    BOMA_LOG_FORMAT: "pretty",
    SANDBOX_PROVIDER: "none",
    BOMA_JOBS: "postgres",
    BOMA_REALTIME: "postgres",
  };

  const migrate = await run(bomaBin(resourcesRoot()), ["migrate"], { env });
  if (migrate.code !== 0) {
    await stopStackAt(pgPort, paths, httpPort);
    // A non-zero exit with an empty output means the binary never started (a missing DLL on
    // Windows, for example). Say so explicitly instead of showing a blank dialog.
    const detail = migrate.output.trim().length > 0
      ? migrate.output.split("\n").slice(-20).join("\n")
      : `boma migrate exited with code ${migrate.code} without writing anything — the binary may not start on this machine (a missing DLL is common on Windows).\nBinary: ${bomaBin(resourcesRoot())}\nDatabase: ${env.BOMA_DATABASE_URL}`;
    persistFailure(paths, "migrate", detail);
    throw new StackError("migrate", "the migrations did not apply", detail);
  }

  const serverLog = openServerLog(paths.serverLog);
  const serve = spawn(bomaBin(resourcesRoot()), ["serve"], {
    env: { ...process.env, ...env },
    stdio: ["ignore", "pipe", "pipe"],
    detached: false,
  });
  writeFileSync(paths.servePid, String(serve.pid ?? ""), { mode: 0o600 });
  serve.stdout?.pipe(serverLog, { end: false });
  serve.stderr?.pipe(serverLog, { end: false });

  const worker = spawn(bomaBin(resourcesRoot()), ["worker"], {
    env: { ...process.env, ...env },
    stdio: ["ignore", "pipe", "pipe"],
    detached: false,
  });
  writeFileSync(paths.workerPid, String(worker.pid ?? ""), { mode: 0o600 });
  worker.stderr?.pipe(serverLog, { end: false });

  const handle = new StackHandle(httpPort);
  const onWorkerExit = (code: number | null) => {
    workerCrashes += 1;
    if (workerCrashes === 1) {
      const retry = spawn(bomaBin(resourcesRoot()), ["worker"], {
        env: { ...process.env, ...env },
        stdio: ["ignore", "pipe", "pipe"],
        detached: false,
      });
      writeFileSync(paths.workerPid, String(retry.pid ?? ""), { mode: 0o600 });
      retry.stderr?.pipe(serverLog, { end: false });
      retry.on("exit", onWorkerExit);
    } else {
      handle.emit("fatal", `the background worker exited twice (last code ${code}); see ${paths.serverLog}`);
    }
  };
  let workerCrashes = 0;
  serve.on("exit", (code: number | null) => {
    handle.emit("fatal", `the server exited (code ${code}); the app cannot work against it`);
  });
  worker.on("exit", onWorkerExit);

  const healthy = await waitForHealth(`http://127.0.0.1:${httpPort}`, 90, 1_000);
  if (!healthy) {
    persistFailure(paths, "healthy", logTail(paths.serverLog) ?? `no server log lines available (${paths.serverLog})`);
    await stopStackAt(pgPort, paths, httpPort);
    throw new StackError(
      "healthy",
      "the server did not become healthy",
      logTail(paths.serverLog) ?? `no server log lines available (${paths.serverLog})`,
    );
  }
  persistPort(paths, httpPort);
  return handle;
}

async function stopStackAt(pgPort: number, paths: StackPaths, _httpPort: number): Promise<void> {
  await terminatePid(pidOf(paths.workerPid), 3_000);
  await terminatePid(pidOf(paths.servePid), 3_000);
  if (existsSync(paths.pgData)) {
    await run(postgresBin(resourcesRoot(), "pg_ctl"), ["-D", paths.pgData, "-m", "fast", "-w", "stop"]);
  }
  for (const pidFile of [paths.workerPid, paths.servePid]) {
    try {
      unlinkSync(pidFile);
    } catch {
      // Already gone.
    }
  }
}

/** Stop the stack; the window never outlives the data. */
export async function stopStack(userData: string, httpPort = 0): Promise<void> {
  const paths = stackPaths(userData);
  await stopStackAt(0, paths, httpPort);
}

async function waitForHealth(url: string, attempts: number, delayMs: number): Promise<boolean> {
  for (let i = 0; i < attempts; i += 1) {
    if (await isDeploymentReachable(url)) {
      return true;
    }
    await sleep(delayMs);
  }
  return false;
}
