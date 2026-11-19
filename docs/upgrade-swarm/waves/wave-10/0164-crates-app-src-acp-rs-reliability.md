# v0.4.0 reliability proposal — `crates/app/src/acp.rs` (ACP stdio client)

Worker #164 · LENS=reliability · target v0.4.0 · READ-ONLY analysis (no code changed).

Status of claims: every "current state" statement is CONFIRMED by a direct read of `acp.rs`
(all 324 lines), `error.rs` (1-67), and the call site in `acp_provider.rs` (266-310) in this
worktree. Forward-looking items are proposals (GUESS until implemented and tested). No tests
were run for this proposal.

## Current structure (CONFIRMED)

- `AcpSession` owns one `Child` with piped stdin/stdout/stderr and `kill_on_drop(true)` (58-84).
  `spawn`'s rustdoc says it "completes the `initialize` handshake" (51) — it does not; the caller
  invokes `initialize()` separately (`acp_provider.rs:293`).
- Every failure — spawn, write, flush, EOF, oversized line, UTF-8, JSON parse, RPC `error`,
  timeout, missing `sessionId` — collapses to `AppError::Unavailable` (~15 sites). That is the
  *only* retryable variant (`error.rs:49-51`), so deterministic protocol faults are retried.
- `request` (146-179) and `session_prompt` (235-307) apply `timeout` per **line read** only. The
  caller adds one outer turn deadline (`acp_provider.rs:296`); the session itself has none.
- The caller spawns one process per turn and tears it down unconditionally (278-303): no
  respawn, no exit-code inspection, no stderr retained for diagnosis.

## Failure modes

1. **Session-per-run / no supervision.** A turn is `spawn → initialize → session/new → prompt →
   close → shutdown` inside one function (`acp_provider.rs:278-303`). If the executor dies at any
   point, there is no `try_wait`, no exit code, no restart path: the whole run fails and every
   later call on the session returns `Unavailable`. `session_close` (310-314) and `shutdown`
   (317-320) still run on a dead/hung child, each adding up to `HANDSHAKE_TIMEOUT` (10 s) plus a
   2 s wait to the failure path. Worst-case teardown exceeds the configured prompt deadline.
2. **Subprocess death and deadlock.** EOF is indistinguishable from any other fault (124-125).
   A write to a dead pipe can *succeed* into the kernel buffer, then the read hangs for a full
   timeout (600 s default, `DEFAULT_PROMPT_TIMEOUT`, 324). Worse, stderr is piped at spawn (69)
   but drained only if the caller opts in (93-102): a chatty executor fills the ~64 KiB pipe and
   **blocks on its own log writes**, hanging the turn with no error. `read_line_bounded` consumes
   partial bytes before a newline arrives (136-139); cancelling it (the outer timeout drops the
   future) discards those bytes and desynchronizes the JSON-RPC stream — a cancellation-safety
   bug. Writes (107-111) and `decide()` (268-271) have no timeout at all, violating the
   "timeout on every outbound call" rule in AGENTS.md; a hung permission decider blocks forever.
3. **Protocol drift.** `initialize` hardcodes `protocolVersion: 1` (190) but never checks the
   reply, despite the rustdoc promise (55-57, 184-185) — a v2 executor is silently accepted.
   Inbound messages are never checked for `jsonrpc: "2.0"` (164); ids are accepted only as `u64`
   (166, 266), so string ids (legal JSON-RPC) orphan the response and spin until timeout.
   `session/request_permission` with a malformed id falls back to `unwrap_or(0)` (266) and the
   agent waits for an answer that never matches. Missing `stopReason` silently defaults to
   `"end_turn"` (298-299); unknown methods/notifications are dropped in the `_` arm (294-303),
   hiding version drift. Because all of this is `Unavailable`, the worker retries a permanently
   broken handshake as if it were a transient outage.

## Concrete hardening (with risks)

1. **Classify errors before mapping.** Introduce an internal `AcpError` (`Spawn`, `Io`, `Timeout`,
   `Eof { exit_code, stderr_tail }`, `Protocol(&'static str)`, `Permission`) and map to `AppError`
   at the public boundary: keep `Unavailable` only for `Spawn`/`Io`/`Timeout`/`Eof`; route
   `Protocol` to a new non-retryable variant (or document the mapping choice).
   *Risk:* a new `AppError` variant touches the api error envelope → record any wire change as a
   DEVIATION in `docs/PARITY.md`. *Mitigation:* if adding a variant is blocked for v0.4.0, at
   minimum log the classification and cap retries at the worker layer.
2. **Supervise the child.** Drain stderr automatically inside `spawn` (return the handle or keep
   it), retaining a bounded ring of the last N log lines; on every read failure call
   `child.try_wait()` and log pid + exit code + stderr tail. Give `shutdown` an escalate-to-kill
   path after the 2 s grace.
   *Risk:* auto-spawning the drain task changes `drain_stderr` ownership/callers; a dropped handle
   detaches but must not lose the stderr tail. *Mitigation:* keep the API, make stdout-independent,
   and test a script that writes >1 MiB to stderr mid-turn.
3. **Make reads cancellation-safe and bounded.** Buffer bytes in a `pending: Vec<u8>` field and
   `consume()` only once a full line is present, so dropping a timed-out future cannot corrupt the
   stream; keep `MAX_LINE_BYTES` as the hard cap and emit a distinct `Protocol` error when hit.
   *Risk:* extra state must handle multiple lines per chunk; bugs there could drop a valid line.
   *Mitigation:* unit-test interleaved/multi-line chunks and timeout-mid-line explicitly.
4. **Enforce deadlines at every edge.** Wrap `send`/`flush`, `decide()`, and `session_close` in
   `timeout`; add a total-turn budget inside `session_prompt` (not only in the caller) with a
   separate idle-line timeout, and kill the child when the budget expires.
   *Risk:* a tight idle timeout can cut large but legitimate chunks. *Mitigation:* configure
   separately from `DEFAULT_PROMPT_TIMEOUT`, and verify against the real executor's cadence.
5. **Fail fast on protocol drift.** Validate the `initialize` reply's `protocolVersion == 1`,
   require `jsonrpc: "2.0"`, correlate responses by full `Value` id (accept string ids), count and
   warn on unknown methods, reject malformed permission ids instead of `unwrap_or(0)`, and require
   `stopReason` instead of defaulting.
   *Risk:* stricter parsing can reject executors that currently "work" — a run-outcome change →
   DEVIATION and operator approval. *Mitigation:* differential-test against each supported
   executor version first; land the harmless checks (id correlation, permission id) before the
   strict `version`/`jsonrpc` rejection.
6. **Do not auto-replay prompts.** Keep one process per turn, but surface "executor exited (code
   X)" with its stderr tail so the worker's existing retry can decide. Automatic respawn + replay
   of `initialize`/`session/new`/prompt is *not* recommended: prompts are not idempotent and a
   replay can duplicate side effects.
   *Risk:* if a supervised/respawning session is later required, idempotency must be proven first.
   *Mitigation:* treat respawn as an opt-in flag with an ADR, not a v0.4.0 default.

## Cross-cutting risks / sequencing

- Order 1 → 3 → 2 → 4 → 5 → 6: classification first makes every later failure diagnosable;
  cancellation-safety before supervision avoids masking stream corruption as a child death.
- Items 1 and 5 are the only ones with possible externally visible effects; both need
  `docs/PARITY.md` entries and approval before shipping (AGENTS.md).
- Verification gate per step: `cargo fmt --check`, `clippy -D warnings`, unit + `tests/`
  integration, and new fake-executor tests (early exit, stderr flood, oversized line, string id,
  wrong protocol version, hung decider) with `REQ-061` in every test name (`tools/trace_check.py`
  counts declarations).

## Out of scope for this module

Executor selection/config, worker retry policy, sandbox/workspace cleanup, and the permission-card
UI live elsewhere. This proposal only hardens process lifecycle, error classification, deadlines,
and protocol tolerance at the ACP stdio boundary.
