# v0.4.0 reliability proposal — `crates/app/src/acp_provider.rs` (local ACP provider)

Worker #166 · LENS=reliability · target v0.4.0 · READ-ONLY analysis (no code changed).

Status of claims: every "current state" statement below is CONFIRMED by a direct read of
`acp_provider.rs` (all 337 lines), `acp.rs` (all 324 lines), `error.rs` (1-67), the call sites in
`worker.rs:594-630` and `usecases.rs:9609-9640`, the probe in `rpc.rs:4000-4050`, and the
REQ-061 tests in `testkit/src/build.rs:10670-10882`. Forward-looking items are proposals (GUESS
until implemented and tested). No tests were run for this proposal. Companion doc #164 covers the
`acp.rs` wire client; this doc stays on the provider/selection layer.

## Current structure (CONFIRMED)

- `AcpProvider` is stateless per turn: `complete()` makes a fresh workspace, calls `turn_inner`,
  then applies `cleanup_policy` (`acp_provider.rs:331-336`). One subprocess per turn.
- `new()` resolves `BOMA_ACP_WORKSPACES` once at construction (75-80); the worker builds one
  provider per run with `workspace_root: None` (`worker.rs:606-614`), so the default
  `temp_dir()/boma-acp-<pid>` root is used.
- Provider selection reads the `acp_agents` attachment first (`worker.rs:598-604`), picks the
  compiled-in `ExecutorSpec` by `id`, and **silently falls back to the internal model** if the
  read errors or the id is unknown (616-628). That fallback masks an unavailable executor.
- `turn_inner` passes `HANDSHAKE_TIMEOUT` (10 s) as the session I/O timeout (282); the same value
  becomes the per-line read timeout in `acp.rs:157,256`. `prompt_timeout` (600 s default) only
  wraps the whole prompt future (`acp_provider.rs:296`).
- All failures — bad cwd, spawn, handshake, `session/new`, prompt, empty answer — collapse to
  `AppError::Unavailable` (275, 283, 293-307), the only retryable variant (`error.rs:49-51`).

## Failure modes

1. **Executor probe failure.** `bots.detectedExecutors` runs a best-effort 2 s `--version`
   probe (`rpc.rs:4011-4026`) and reports ids only from it (4003-4009). But `attach_acp_agent`
   (`usecases.rs:9609-9640`) and `bots.connectAcp` (`rpc.rs:4056-4071`) never probe at all —
   they validate the id against the compiled registry only. An operator can attach to a compiled-in
   executor that is not installed; the failure is deferred to the first run. A flaky/slow `--version`
   probe (>2 s) reports the executor as absent and hides a working binary.
2. **PATH drift.** Detection/attachment happen at connect time; the run happens later, possibly in
   a service process whose PATH differs from the operator's shell. If the binary is moved, upgraded
   to a different name, or was only on the shell PATH, the attachment stays valid in the store but
   `AcpSession::spawn` fails every turn (`acp.rs:64-72`). The bot keeps being selected
   (`worker.rs:602-604`), so every run fails retryably instead of failing fast or falling back.
3. **Connect / handshake errors.** `initialize()` returns a `Value` that `turn_inner` discards
   (`acp_provider.rs:293`). Despite the rustdoc in `acp.rs:56-57` claiming a non-v1 answer is
   rejected, no protocol-version check exists — a wrong-version executor is driven as if valid.
   Every connect stage flattens to the same `Unavailable`, so the run reason ("a dependency is
   unavailable") cannot tell spawn from handshake from `session/new`; the test at
   `build.rs:10819` only checks the reason is non-empty and not internal. Because `Unavailable`
   is retryable, a deterministic connect fault (bad command, rejected protocol) is retried with
   backoff like a transient outage.
4. **Per-line timeout reused for the turn.** The 10 s handshake deadline is also the per-line
   prompt read deadline (`acp.rs:256`). An executor that pauses >10 s between message chunks
   (normal "thinking" time) aborts the turn long before the 600 s budget — a false failure.
5. **Permission-card stall.** `wait_for_answer` swallows card-read errors silently (`acp_provider.rs:191`),
   so an answered card whose read transiently fails keeps the subprocess parked up to the 24 h
   default timeout (61). Terminal-run polling (181-190) mitigates cancellation but a forgotten
   card still pins a child process for a day.
6. **Workspace residue / concurrency.** Failed workspaces are kept by default (319-325) with no
   TTL or cap → unbounded disk growth on repeated failures. `fresh_workspace` keys on
   thread-id + nanos (98-103); a clock regression or collision could reuse a dir. No semaphore
   bounds concurrent ACP subprocesses, so a run burst can exhaust process limits.

## Concrete hardening (with risks)

- **Re-probe at turn time before spawn.** Add a cheap `command --version` check (bounded, e.g.
  2 s) in `turn_inner` before `AcpSession::spawn`, and return a distinct, non-retryable
  `AppError` variant (e.g. `ExecutorUnavailable`) so a drifted binary fails the run legibly
  instead of being retried. *Risk:* --version is not an ACP capability check; a binary that
  answers --version but not ACP still fails later. Keep it a pre-filter, not a guarantee.
- **Validate the handshake.** Parse `initialize()`'s result for `protocolVersion == 1` and fail
  fast before `session/new`; give the connect stages stage-labelled errors (spawn / initialize /
  session_new) mapped to a stable reason. *Risk:* stricter validation may reject an executor that
  currently "works" by relaxing the version — a behaviour change to record in `docs/PARITY.md`.
- **Separate I/O timeouts.** Give `AcpSession` a short handshake timeout and a longer per-line
  read timeout (e.g. prompt_timeout or a dedicated inter-chunk idle timeout), keeping the outer
  total budget. *Risk:* too-generous idle deadline lets a stalled executor hold resources longer;
  pair with the cancellation/EOF checks proposed in doc #164.
- **Fail fast on connect instead of silent fallback.** In `worker.rs:614-628`, only fall back to
  the internal provider for an unreadable attachment; for a known-but-missing executor, surface a
  failed run (REQ-061 second criterion) rather than answering with the wrong model. *Risk:* changes
  observed bot behaviour; needs operator sign-off as a DEVIATION.
- **Make permission reads observable and bounded.** Log card-read errors, tolerate a bounded
  streak, and lower/expose the 24 h default; keep terminal-run cancellation. *Risk:* lowering the
  timeout can cancel a legitimate long approval — make it configurable, not a hard constant.
- **Bound workspaces and subprocesses.** Add a cap + TTL sweep for retained failure workspaces and
  a semaphore around concurrent ACP turns. *Risk:* sweeping needs care not to delete a workspace
  still in use; add a grace period and count-based, not time-only, eviction.
- **Make workspace naming collision-proof** with a process-owned atomic counter or `create_new`,
  falling back on collision. *Risk:* minor; `create_new` needs a retry loop.

## Tests to add (traceability: REQ-061 in the name)

- `req_061_detected_executor_is_reprobed_before_spawn` — stub exits 0 for --version then
  disappears; assert the run fails legibly and is not retried as transient.
- `req_061_wrong_protocol_version_fails_before_session_new` — stub answers v2; assert no
  `session/new` is sent and the run reason names the handshake.
- `req_061_long_inter_chunk_pause_does_not_abort_turn` — stub sleeps >10 s between chunks;
  assert the answer is collected within the total budget.
- `req_061_known_but_missing_executor_fails_instead_of_internal_fallback` — attachment to a
  compiled id with no binary; assert Failed run, not an internal-model answer.
- `req_061_failure_workspaces_are_bounded` — many failing runs; assert retained dirs <= cap.

All claims above are CONFIRMED from source reads; every proposed change is GUESS until implemented
and its tests run.
