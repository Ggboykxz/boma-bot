# 0194 — crates/bin/src/commands.rs — reliability proposal (v0.4.0)

Scope: long-running and one-shot jobs (`serve`, `worker`, `rekey`, `migrate`, `check_config`) and
helpers (`build_app`, `secrets_vault`, `vapid_config`, `shutdown_signal`, `worker_owner`).
Evidence: source reading of `commands.rs`, `cli.rs`, `main.rs`, `infra/src/stores/secrets.rs`,
`infra/src/tenant.rs`, `infra/src/secret_box.rs`. CONFIRMED = traced in code; GUESS = needs a
runtime test. Read-only worker: no code changed.

## Failure-mode inventory

1. Exit codes (CONFIRMED). Every command returns `anyhow::Result<()>`; `main` returns it, so every
   failure surfaces as exit 1. No distinct code for config error, DB unreachable, migration
   failure, or partial/degraded state. Orchestrators cannot branch on "retry" vs "swap key now" vs
   "give up". `rekey`'s documented contract ("swap `BOMA_MASTER_KEY` only after exit 0") rests
   entirely on this single undifferentiated code.
2. `rekey` partial side effects (CONFIRMED mitigated). `infra::rekey_space_keys` runs inside
   `tenant::with_tenant`, i.e. one transaction that rolls back on any decode/DB error
   (`tenant.rs:116-142`), so exit != 0 leaves zero rows changed and keeping the old key is safe.
   Residual risks: (a) whole-table `SELECT ... FOR UPDATE` holds row locks for the entire rewrap —
   availability hit on large deployments (GUESS on scale); (b) no read-back verification that every
   row unwraps under the new key before returning Ok; (c) `new_key: String` and both `SecretBox`es
   are not zeroized; (d) no advisory lock against a concurrent `rekey`/`migrate`.
3. `build_app` side effects + repeated vault (CONFIRMED). `secrets_vault()` is called 5x
   (`commands.rs:190,193,195,201,210`), each rescanning all env vars and building a fresh
   `SecretBox`. With no `BOMA_MASTER_KEY` each call yields a *different* ephemeral key
   (`secret_box.rs:56-60,74-79`), so an ephemeral deployment splits its secrets across 5 unrelated
   keys. `FileArtifactContentStore::new` creates a directory (`commands.rs:162-165`) that survives
   a later `?` failure (e.g. vapid/vault construction).
4. `serve` startup ordering (CONFIRMED). `computer.provision()` (side-effecting, `commands.rs:46-52`)
   runs before `build_app` and before `TcpListener::bind` (`82-84`). A later failure (bad config,
   port in use) leaves a provisioned sandbox leaked, and a port conflict is discovered only after
   expensive provisioning. `axum::serve` has graceful shutdown but no drain timeout (`91-97`), so a
   stuck in-flight request can block shutdown indefinitely.
5. `worker` error handling (CONFIRMED). `dispatch_routines`, `sweep_idle_computers`, and
   `worker.step()` all use `?` (`341-358`). A single transient `Unavailable` (DB blip) terminates
   the process; with `connect_lazy` this becomes a crash loop on cold start. No backoff/jitter
   (fixed 250 ms poll, `338-361`) → thundering herd with multiple workers. The three sub-tasks are
   sequential: a failing idle sweep starves `worker.step()` even though queue progress is
   independent. No shutdown signal (unlike `serve`), so SIGTERM can kill a claimed run mid-step and
   leave a stale lease until timeout (GUESS — depends on `Worker::step` internals).
6. `shutdown_signal` (CONFIRMED). On Unix, a failure to register SIGTERM is logged and then the
   future resolves, which `tokio::select!` treats as a shutdown request (`439-453`) — the server
   exits immediately on a handler-registration error. Only SIGTERM/ctrl-c, no SIGHUP.
7. Test coverage (CONFIRMED). `commands.rs` has no `#[cfg(test)]` module; none of these failure
   paths are exercised.

## Concrete hardening (prioritized)

P0 — Typed error taxonomy + exit codes. Add `enum CommandError { Config, Dependency, Migration,
Rekey, Partial }` (thiserror) at the binary edge; map in `cli::run`/`main` via
`std::process::ExitCode`: config=2, dependency/DB=3, migration=4, rekey=5, partial/degraded=6,
success=0 (keep clap parse=2). Internally keep `anyhow::Result` but downcast at the edge. Risk:
externally visible CLI change → DEVIATION; record in docs/PARITY.md and await approval before
shipping; default unknown errors to 1 so legacy callers are not broken.

P0 — Rekey read-back + zeroize + lock. After `rekey_space_keys` returns Ok, run a verification
scan under the new key (unwrap every row; fail if any row does not unwrap) before logging success.
Take a session-scoped `pg_advisory_lock` with a timeout around rekey and migrate to serialize
operators. Zeroize `new_key` and key material after use. Risk: verification doubles a full scan
(latency); a badly scoped advisory lock can block other migrations — use a distinct, short-lived
key and always release on drop.

P1 — Single vault. Build `secrets_vault()` once in `serve`/`worker`, share via `Arc<SecretBox>`,
pass into `build_app`; remove the 5 internal calls and the repeated warning spam. Risk: `build_app`
signature change (internal only).

P1 — Bind-first / provision cleanup. In `serve`, bind the listener before `computer.provision()`
so port conflicts fail fast; wrap provisioning in a guard that releases the sandbox on any later
error. Risk: brief window where the port is bound but not serving — either accept connection
refused/503 health checks during startup, or serve a 503 until `readiness` is green.

P1 — Worker resilience. Classify `AppError::Unavailable` as transient: bounded retry with
exponential backoff + jitter (e.g. 3 attempts) before giving up; keep permanent errors fail-fast.
Decouple the three sub-tasks so a failed sweep logs-and-continues to `worker.step()`. Add a
shutdown signal (`tokio::select!` on `shutdown_signal()`) and a bounded in-flight drain. Risk:
retries can mask a genuinely broken deployment (mitigate with a capped error-rate log); backoff
must use `tokio::time::sleep` (already async, no blocking call on the runtime).

P2 — Shutdown robustness. Do not resolve `terminate` on handler-registration failure: log and fall
back to `pending()`. Add a drain deadline (e.g. 10 s) to `axum::serve`. Risk: the deadline can cut a
long request — record as DEVIATION if it changes externally visible behavior.

P2 — Tests. Add `#[cfg(test)]` in `commands.rs` (or `tests/`): exit-code mapping table; rekey
rollback on a corrupted row (all-or-nothing); rekey read-back verification failure; worker
transient-error retry/backoff and permanent-error propagation; `shutdown_signal` on SIGTERM;
single-vault identity; `serve` bind-before-provision ordering. Use testcontainers for DB paths.
Put REQ ids in test names where a REQ applies (e.g. REQ-046 for rekey).

## Open questions (route to docs/STATE.md)
- Are distinct exit codes acceptable, or must legacy callers keep seeing 1 for all failures?
  (DEVIATION gate — needs owner approval.)
- Should the worker retry transient errors in-process, or is orchestrator crash-loop-with-backoff
  the intended policy?
- Is a shutdown drain deadline acceptable, or must in-flight runs always finish?

Status: all findings CONFIRMED by source reading except the noted GUESS items; no runtime
verification performed (read-only worker).
