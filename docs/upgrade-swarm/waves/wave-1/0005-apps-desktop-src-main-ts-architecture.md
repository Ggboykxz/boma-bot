# 0005 — apps/desktop/src/main.ts · v0.4.0 architecture proposal

- Brief: id 5, module `apps/desktop/src/main.ts`, lens `architecture` (`docs/upgrade-swarm/briefs-upgrade.jsonl`).
- Status: **PROPOSAL only.** No code, spec, menu wording or release edited. Markers: **CONFIRMED** = read this session; **GUESS** = needs owner decision.
- Anchors: `apps/desktop/src/main.ts` (read at `before-m5`), `apps/desktop/src/stack.ts` (read), `docs/STATE.md` (2026-11-17 Windows boot-gate entry, v0.3.0-rc3/rc5 notes), M35 design `docs/superpowers/specs/2026-11-16-m35-self-contained-desktop-design.md` (distribution-only; no wire/web/server/spec change).

## 1. What the file is today (CONFIRMED)

~330 lines, one module, six jobs: (a) env at module scope (`BOMA_DESKTOP_URL` override, `BOMA_DESKTOP_SIGNED`, `BOMA_DESKTOP_DISABLE_GPU`); (b) window factory + `guardNavigation` (origin pin, external `openExternal`); (c) diagnostics (`checkpoint`→`boot.log`, `logHint`, `resetHint`); (d) boot state machine (override probe → embedded `startStack` → one user-driven retry) and `fatalAndQuit`; (e) native menu (`File`/`Stack`/`View`) and `signOut` (partition storage clear); (f) app lifecycle (`whenReady`, `activate`, `before-quit`, `window-all-closed`).

M35 intent holds: thin shell, no preload/IPC/Node-in-renderer, `persist:boma` partition, embedded stack = default, `BOMA_DESKTOP_URL` = self-hosted window, update feed inert until signed. `stack.ts` owns supervise/teardown and is the right boundary — do not merge it into window code.

## 2. Structural defects (architecture, CONFIRMED)

1. **Triplicated launch sequence.** `boot()`, the retry inside `boot()`, and `restartStack()` each re-implement `startStack → guardNavigation → handle.on('fatal') → liveStack = handle → loadURL`. They already drift (retry path has no `checkpoint()`; `restartStack` skips error typing). The `/health/health` 404 bug (STATE.md, fixed `c450ea1`) was exactly this class of duplicated probe.
2. **`stopStackOnce` is not idempotent in use.** `restartStack()` sets `stackStopped = false` after `stopStackOnce()`, voiding the documented contract; a quit between restart and re-flag could double-stop. `fatalAndQuit` calls `stopStack` directly, then `before-quit` calls `stopStackOnce` → `stopStack` again (harmless today, layering smell).
3. **Duplicate health probe.** `isReachable()` in main.ts re-implements `isDeploymentReachable()` in stack.ts, with the same URL-suffix contract — the exact regression shape that gated Windows boot.
4. **`activate` creates a naked window.** macOS `activate` with zero windows calls `makeWindow()` but never `boot()`s it: no `guardNavigation`, no `loadURL`, menu rebound to an inert window. Also double `session.fromPartition` in `makeWindow`/`signOut`.
5. **No single-instance lock.** A second launch reaps the first's `serve`/`worker` via pid files (`reapOrphans`) and races the persisted HTTP port. Two shells can silently kill each other's stack.
6. **Override path is second-class.** `BOMA_DESKTOP_URL` gets no `checkpoint()` lines, an http(s)-agnostic `openExternal`, and a Quit-only failure dialog (no retry), unlike the embedded path's richer UX.

## 3. What to keep / what to split (v0.4.0)

**Keep:** the M35 thin-shell posture and its invariants (partition ownership, `contextIsolation/sandbox`, no IPC, signed-update default-off, `boot.log` support trail, single-window, stack-not-outliving-data teardown order in stack.ts). **Split** main.ts into `main.ts` (composition root, ~80 lines: env, GPU flag, lifecycle wiring), `window.ts` (makeWindow/guardNavigation/session), `menu.ts` (template + rebuild), `boot.ts` (one launch state machine used by boot, retry, restart, activate), `diag.ts` (checkpoint/logHint/resetHint). stack.ts unchanged in role; optionally export `isDeploymentReachable` for reuse instead of the local probe.

## 4. Upgrade proposals (3–5) with risks

**P1 — Split + single launch state machine.** One `launchOrigin(window): Promise<'loaded'|'fatal'>`; boot/retry/restart/activate all call it. Delete local `isReachable`; reuse stack.ts probe. *Risk:* regressing the just-stabilized retry path; mitigate by re-running `tools/desktop-stack-smoke.sh` and both `desktop-boot` CI legs (Linux + Windows) and diffing `boot.log` checkpoint order.
**P2 — Single-instance lock.** `app.requestSingleInstanceLock()`; second instance focuses the existing window and exits without touching pid files; `reapOrphans` stays first-launch-only. *Risk:* stale lock after hard kill — keep pid-liveness (`kill(pid,0)`) as the real check, lock only as a hint. *GUESS:* owner wants focus-vs-quit behavior.
**P3 — Fix window lifecycle symmetry.** `activate` re-enters `launchOrigin`; `window-all-closed`/`before-quit` share one `shutdown()`; `stopStackOnce` truly idempotent (generation counter, no flag reset). *Risk:* changing quit timing could orphan PG children; keep SIGTERM→3 s→SIGKILL order and `pg_ctl -m fast` teardown untouched.
**P4 — Menu overhaul, roles-first.** Add macOS `Edit` role submenu (copy/paste/select-all), `About Boma Bot`, local-only "Open data folder" / "Copy diagnostics" (support affordance, no network); keep `Stack > Restart the local stack` disabled in override mode; devtools role only when a dev env var is set; nicer `Restart` vs `Stack > Restart` naming. *Risk:* new support surface — keep everything local-file, no IPC, no diagnostics upload (M35 distribution-only). *GUESS:* menu wording needs owner sign-off.
**P5 — Override-path parity.** Give the override path checkpoints, the same Try again/Quit dialog on probe failure, and an https-preference warning for plain-http remote origins (never silently block — self-hosters on LAN http exist). *Risk:* altering owner-documented self-hosted UX; keep behavior additive, document in README/UPGRADING.

## 5. Suggested sequencing & gates

P1 → P3 (same PR, one boot path) → P2; P4/P5 independent. Each slice CI-green (typecheck, lint, `desktop` + `desktop-boot` jobs); no spec/REQ ids touched; M35 non-goals (macOS packs, signing) stay out. Success = boot.log identical-or-better, Windows gate green, zero new user-visible strings unapproved.
