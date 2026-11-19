# Wave 1 #006 — apps/desktop/src/main.ts — Reliability (v0.4.0)

Scope: read-only review of `apps/desktop/src/main.ts` (before-m5, 349 lines) + `stack.ts` for context. v0.4.0 proposal. No code edits here.

## Failure modes observed / credible

1. **GPU / Session-0 hang (CI runners, Windows services, RDP, headless Linux).** Software rendering is opt-in only (`BOMA_DESKTOP_DISABLE_GPU=1`, lines 311-313). On a broken-GPU runner the window is created but never paints, or `whenReady()` never settles; either way there is no timeout and no diagnostic — the app sits with no window and no trace. `app.whenReady().then(...)` (315) has no `.catch` and no watchdog. Session-0 (Windows service session) has no GPU at all; the current code does not detect it.
2. **Silent quit paths.** `window.loadURL(...).catch(() => app.quit())` (128, 143, 171) exits with no dialog, no `boot.log` line, no support breadcrumb. `enableSignedUpdateFeed` also swallows check failures silently (303) — acceptable, but `autoUpdater` emits `error` as an EventEmitter event and **no `error` listener is registered**, so an updater failure is an unhandled EventEmitter throw = process crash.
3. **Icon warning on every launch.** The comment at 60-62 documents the missing-`build/`-asset warning rather than fixing it. Noise on stderr on some platforms; pollutes support logs and can mask real errors.
4. **before-quit stack stop can wedge or double-stop.** (335-343) `event.preventDefault()` + `stopStackOnce().finally(() => app.quit())`. `stopStack` can block for seconds (pg_ctl `-w stop`, `terminatePid` busy-polls up to 3s per pid, SIGKILL fallback) with **no overall deadline**; if it rejects, `.finally` still quits but the rejection is unhandled and nothing is written to `boot.log`. Two other stop paths bypass the single-flight flag entirely: `fatalAndQuit` (110) calls `stopStack` directly; `restartStack` (217-231) flips `stackStopped` back to `false` mid-flight, so a quit landing during a restart can stop a half-started stack or race pid files.
5. **boot.log is write-only and unbounded.** `checkpoint()` (89-97) appends forever (no rotation, no size cap), carries no env markers (platform, GPU mode, CI, Session-0), and is not written on `uncaughtException` / `unhandledRejection` / GPU-process crash — exactly the records a hung runner needs.
6. **macOS `activate` builds a blank window.** (327-331) `makeWindow()` is created with no `loadURL` and no `boot()` — the user gets an empty frame.
7. **Restart races and double-clicks.** `restartStack` has no re-entrancy guard; two menu invocations spawn two stacks competing for the port file. `handle.on("fatal")` during a pending quit opens a *new* error dialog while `before-quit` is mid-teardown.
8. **loadURL failure after successful stack start** leaves a running stack + dead window; only `app.quit()` remains, and quit then relies on before-quit (fine) but logs nothing about which URL failed.

## Proposals (exact changes)

**P1. Auto GPU fallback with a pre-ready watchdog.** Replace the bare env check with tri-state `BOMA_DESKTOP_GPU=auto|off|on` (default `auto`): `off` ⇒ `disableHardwareAcceleration()`; `auto` ⇒ disable when `process.env.CI === "true"`, Windows `SESSIONNAME === "Services"`, Linux with no `DISPLAY`/`WAYLAND_DISPLAY`, or `BOMA_DESKTOP_DISABLE_GPU=1`. `checkpoint(\`gpu mode: ${mode} (${reason})\`)`. Add a module-scope watchdog started before `whenReady()`: if `whenReady` has not resolved in 20s, write `boot.log` ("whenReady timeout") and `app.exit(1)` with a distinct code (dialog unavailable pre-ready). Add post-ready watchdog (120s) covering `boot()` completion; on expiry, `checkpoint` + `fatalAndQuit(window, "boot timed out")`. *Risk:* software GL on mis-detected RDP sessions — keep `BOMA_DESKTOP_GPU=on` escape hatch; watchdog `app.exit` may orphan the stack — acceptable, `reapOrphans` handles it next launch (confirm it runs before any spawn, it does, stack.ts:280).

**P2. Fix the updater crash path.** `autoUpdater.on("error", (e) => { checkpoint(\`updater error: ${e.message}\`); })` — never throws. Keep the existing silent-check behavior. *Risk:* none beyond one listener.

**P3. Bound and unify shutdown.** Wrap `stopStackOnce()`'s body in `Promise.race([stopStack(...), sleep(10_000)])`; on timeout or rejection, `checkpoint("stack stop: timeout/error ...")` and proceed. Route `fatalAndQuit` and `restartStack` through the same single-flight helper (drop direct `stopStack` calls). Set a `quitting` guard checked at the top of `restartStack` and of the `fatal` handler (if quitting, log only). Add `process.on("uncaughtException"|"unhandledRejection")` → `checkpoint` + `fatalAndQuit` best-effort. *Risk:* forced quit with a wedged postgres can leave stale `postmaster.pid` — already handled next boot by `pg_ctl`/`reapOrphans`; pid-file deletion stays in `stopStackAt`.

**P4. Structured, rotated boot.log.** In `checkpoint()`: if `boot.log` > 256 KB, rename to `boot.log.1` (overwriting) before append. Add a boot header line on `whenReady` with `platform`, `arch`, `gpu mode`, `CI`, `SESSIONNAME`, versions — support reads one file. Add checkpoints on `render-process-gone`, `child-process-gone` (GPU), `unresponsive`/`responsive` (renderer) with reason/exitCode. *Risk:* one extra `statSync` per checkpoint; `.1` rename loses older history — acceptable.

**P5. Icon: fix instead of warn.** Resolve `build/icon.png` (Linux/Windows ico) via `process.resourcesPath` with `existsSync`; set `icon` on `BrowserWindow` only when present; add a `package.json` `postinstall`/CI check that the file exists in the packaged layout; delete the stale apology comment. *Risk:* taskbar/dock appearance may shift where the icon now actually applies — that is the intent; macOS ignores the field.

**P6. Correct the fatal/silent paths.** Replace every `loadURL(...).catch(() => app.quit())` with `.catch(() => fatalAndQuit(window, \`could not load ${url}\`))` (checkpoints included). macOS `activate`: if zero windows, create window **and call `boot(window)`** (or reload existing) — never a blank frame. Guard `restartStack` with a `restartInFlight` boolean and disable the menu item while set. *Risk:* double-boot on activate if boot already ran — gate on `BrowserWindow.getAllWindows().length === 0` (already present) plus a `booted` flag.

**P7. Honest failure detail on retry.** In the retry branch (164-180), `retryError as StackError` may lack `step`; render `String(retryError)` when `step` is absent instead of printing `undefined`. *Risk:* none.

## Verification plan

- Runner rehearsal: `CI=true xvfb-run boma-desktop` — boot reaches healthy or exits with code 1 and a "whenReady timeout"/"gpu mode: off" line in `boot.log` within 25s/120s.
- Kill `boma serve` mid-run → `fatal` dialog, single `boot.log` entry, `before-quit` exits ≤10s.
- `boot.log` size cap test: pre-fill 300 KB log, launch, observe rotation.
- `npm run pack` artifact check: icon present, no stderr icon warning on Linux/Windows.

## Risks / non-goals

- No changes to sandboxing, navigation guard, partition, updater signing model — out of lens scope, already sound.
- Watchdog timers must be `unref`'d where they must not hold the event loop open pre-ready.
- Windows Session-0 cannot show dialogs; the pre-ready path must not call `dialog` before `whenReady`.
- All changes are main-process only; renderer sandbox unchanged.
