# Worker #298 — `crates/infra/src/mcp_stdio.rs` — LENS: reliability — v0.4.0 proposal

- **Status:** PROPOSAL (read-only review; no source changed). CONFIRMED = verified by reading this file in this worktree; GUESS = needs a test/oracle run against a real stdio server.
- **Scope:** one-RPC-per-spawn JSON-RPC 2.0 stdio transport (`REQ-033`): spawn, `initialize` handshake, single call, read, teardown.

## Summary

The "fresh process per call, kill-on-drop" design is sound for containment, but the teardown and framing paths rely on implicit assumptions that turn ordinary server misbehaviour into hangs or silent data loss. Four concrete reliability gaps: an unbounded `wait()`, a misapplied byte cap, deadlock-prone write-before-read ordering with no process-group isolation, and error typing that collapses every failure into `Unavailable`. None is covered by a test in this module (CONFIRMED: file has no `#[cfg(test)]`).

## Failure modes

### Process death (P)

- **P1 (HIGH, CONFIRMED):** `let _ = child.wait().await` (line 120) has **no timeout**. A child that answers the call but ignores EOF on stdin and refuses to exit (event loops, watchers, servers that wait on a signal) hangs the caller forever, defeating `REQUEST_TIMEOUT` which only guards I/O. This is the worst bug in the module.
- **P2 (MED, CONFIRMED):** the exit status is discarded (`let _ = ...wait()`, line 120), and a child that dies mid-response is reported only as generic `Unavailable` (line 147/151). A crash after writing a partial or invalid reply is indistinguishable from a timeout; no signal/exit code is logged, so operators cannot tell a segfault from a protocol refusal.
- **P3 (MED, CONFIRMED):** on the early-return paths (`?` at lines 94/95/114/116), teardown depends solely on `kill_on_drop(true)` (line 73). Tokio sends SIGKILL and reaps the direct child, but no `wait()` is awaiting it; this is usually fine, yet it is never asserted, so kill/reap behaviour is version-dependent (GUESS: verify on the pinned Tokio).

### Framing errors (F)

- **F1 (HIGH, CONFIRMED):** `BufReader::new(stdout).take(MAX_LINE_BYTES)` (line 81) caps the **total bytes read over the whole session**, not one line as the constant's comment claims (line 27). The initialize reply plus the call reply together consume the 1 MiB budget, so a large tools/list response is truncated after a small handshake — `read_line` then sees `read == 0` (line 149) and returns `Unavailable`. The bound is also applied twice (per line, per session) with contradictory intent.
- **F2 (MED, CONFIRMED):** malformed JSON lines (157–159) and lines with other ids (160–163) are skipped with `continue`. A server that emits non-JSON logs on stdout is tolerated, but a genuinely corrupt/oversized response is silently swallowed until the 10 s timeout — a framing error masquerades as unavailability and burns the whole deadline.
- **F3 (MED, CONFIRMED):** `stderr(Stdio::null())` (line 72) discards the child's only diagnostic channel. Combined with P2, a failed spawn handshake is undebuggable in production.
- **F4 (LOW, CONFIRMED):** the `initialize` result is explicitly discarded (`let _ = initialized;`, line 115); protocol version and capabilities are never validated, so an incompatible server is driven as if it had agreed to `2024-11-05`.
- **F5 (LOW, CONFIRMED):** ids are fixed at 1 and 2 and the JSON-RPC `error.code` is ignored (only `message`, lines 164–171) — acceptable for one call/process, but it blocks any future pipelining and loses error classification.

### Deadlocks (D)

- **D1 (HIGH, CONFIRMED):** the `initialized` notification and the `tools/call` request are written back-to-back (98–114) before reading the call reply (116). If the child writes enough unsolicited stdout (logs, progress, a large init reply) to fill the ~64 KiB pipe while it is still reading stdin, the child blocks on stdout and the parent blocks in `write_all` — a classic pipe deadlock, broken only by `REQUEST_TIMEOUT` (line 131). Reading the init reply first (done at 95) does not protect the second write.
- **D2 (MED, CONFIRMED):** `write_all` under timeout can partially write; on timeout the stream is left mid-frame, and recovery is only via kill-on-drop. No explicit drain/abort, so correctness depends on teardown (P3).
- **D3 (LOW, GUESS):** a single 10 s constant (line 31) bounds handshake, writes and reads alike; a legitimate slow tool call is killed mid-flight. No per-phase or configurable budget.

### Orphan children (O)

- **O1 (HIGH, CONFIRMED):** no process group (`setsid`/`setpgid`) is created. A command that is a wrapper or spawns its own server leaves grandchildren alive after the direct child is SIGKILLed. Those grandchildren inherit the piped stdout (line 71), so `read_line` never observes EOF (`read == 0`, line 149) and the caller waits the full timeout instead of detecting death — slow leak plus zombie/orphan accumulation.
- **O2 (MED, CONFIRMED):** every RPC spawns an unbounded process (64–78); under concurrent load there is no semaphore, so a flood of calls can exhaust PIDs/memory (a self-inflicted fork bomb). The design's containment property has no admission control.

## Concrete hardening (prioritized)

- **P0 — bound teardown:** wrap `child.wait().await` in `tokio::time::timeout` in a dedicated teardown helper; on timeout call `start_kill()` then a bounded second `wait()`, and log exit status/signal on every path (P1/P2). Introduce a typed `StdioError` variant (crash / timeout / EOF / malformed / rejected) instead of blanket `Unavailable` so callers and retries can discriminate.
- **P0 — fix framing:** replace session-wide `.take(MAX_LINE_BYTES)` with a bounded read loop that accumulates one line and errors once it exceeds 1 MiB *per line*; on malformed JSON with a matching id, fail fast rather than `continue` (F1/F2). Capture stderr into a bounded buffer (e.g. last 8 KiB) and include a snippet in errors (F3).
- **P1 — process isolation:** spawn each child in its own process group on Unix (`command_ext`/`pre_exec` via a tiny unsafe shim behind an ADR, or a `setsid` launcher) and kill the whole group on teardown (O1). Add a `tokio::sync::Semaphore` sized by config to cap concurrent children (O2). Both are externally visible behaviour changes → log as DEVIATION candidates in `docs/PARITY.md` if they alter timing/errors.
- **P1 — remove deadlock window:** after writing the init request, fully drain the init reply before writing the notification/request; drain stdout concurrently with stdin writes (e.g. a `select!`/reader task) so unsolicited output cannot backpressure the pipe (D1). Validate the init `protocolVersion`/capabilities instead of discarding them (F4).
- **P2 — observability/policy:** distinguish per-phase budgets (connect/handshake/call/teardown) behind typed config, defaulting to today's 10 s (D3). Log child name, pid, exit reason and byte counts with `tracing` (P2/F3). Consider bounded retry **only** for idempotent `tools/list`, never `tools/call` (P3).

## Risks / trade-offs

- Process-group kill via `pre_exec` needs `unsafe_code` exception (ADR required by AGENTS.md) or a `setsid` wrapper dependency; both add portability cost (Unix-only, no Windows parity).
- Bounded `wait()` after a successful reply may SIGKILL a child that would have exited cleanly milliseconds later — a timing DEVIATION; choose a generous grace period and measure.
- Strict per-line limits and fail-fast malformed handling can reject servers that currently "work" by emitting junk on stdout; mitigated by keeping non-JSON *lines without a matching id* as ignorable while rejecting malformed *id-matching* replies.
- Semaphore admission adds backpressure and new failure under saturation; expose as config, document in `docs/STATE.md`.
- Capturing stderr risks leaking server internals to logs; cap and truncate, never include unbounded output.
