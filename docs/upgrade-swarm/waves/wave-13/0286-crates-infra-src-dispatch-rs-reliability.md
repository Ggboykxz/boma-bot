# 0286 — `crates/infra/src/dispatch.rs` — reliability proposal (v0.4.0)

Worker #286 · lens: reliability · READ-ONLY review of `rebuild-workspace/.worktrees/before-m5`.
Proposals only; no code changed. **CONFIRMED** = read in this tree this session; **GUESS** = needs a
test. Scope: this 63-line dispatcher, its two arms (`mcp_http.rs`, `mcp_stdio.rs`), the guard they
share (`net_safety.rs`), call sites (`commands.rs:141-144`, `usecases.rs:860-923, 7268-7282`), retry
classification (`error.rs:43-66`, `worker.rs:436-463`) and spec `04-nonfunctional.md:61-92`, D-05.
Anchors: REQ-021/024/033.

## What is already right (keep)

Redirects are refused and every URL passes `check_outbound_url` before any byte leaves
(`mcp_http.rs:36-40,57-68`); metadata hosts are refused even with `allow_private` (`net_safety.rs:113-115,140-146`).
stdio fails closed (empty allow-list, `env_clear`, `kill_on_drop`; `mcp_stdio.rs:61-78`) and checks response
`id` (`mcp_stdio.rs:160-163`); tool names are sanitised twice (`mcp_http.rs:174-184`, `usecases.rs:891-898`).
All outbound clients in the crate have a timeout. None of this is a reason to add retries here blindly.

## Failure modes

### F1. SSRF and transport confusion (CONFIRMED)
`match server.transport.as_str() { "stdio" => ..., _ => http }` (`:45-48,58-61`) is a catch-all: any
value except the exact lowercase `stdio` becomes an outbound HTTP call. A mangled value (`"StdIO"`,
`"stdio "`, `"http2"`) silently moves the execution boundary from a gated spawn to the network;
`url` vs `command` coherence is never checked and `McpServerRecord.transport` is a free `String`
(`ports.rs:2364`). The guard's `CheckedOutbound.resolved` is discarded (`mcp_http.rs:58-68`): reqwest
re-resolves at connect, so a rebinding answer can reach a private IP after a public verdict passed.
The D-05 audit records a verdict, not the addresses. GUESS: unlikely against honest servers, but it
is the exact threat D-05 names.

### F2. Retry storms and error misclassification (CONFIRMED shape)
No concurrency limit, breaker or backoff lives in the dispatcher. `list_tools` is two sequential POSTs
(`mcp_http.rs:194-213`); N concurrent runs hitting one flapping server produce up to 2N unthrottled
30 s requests. A routine run failing with `Unavailable` is auto-retried with backoff (`worker.rs:436-463`,
REQ-024), amplifying a dead server. Transient `429`/`5xx` and JSON-RPC errors are mapped to
`AppError::Rule` (`mcp_http.rs:106-124`), which `is_retryable()` says is NOT retryable (`error.rs:49-51`):
the cases that need backoff never get it, while timeouts get run-level retries without a limiter.
`tools/call` has side effects but the JSON-RPC `id` is hardcoded to 1 (`mcp_http.rs:78`), so no server
can deduplicate a retried call.

### F3. Timeouts are per-phase, not per-call (CONFIRMED)
`timeout(30s)` (`mcp_http.rs:37`) has no `connect_timeout`: a TCP-accepting, silent server holds a task
30 s. `text/event-stream` is accepted (`mcp_http.rs:75,111-115`) but `first_sse_message` parses a fully
buffered body (`bytes()`, line 100): a trickling stream is read to the cap even after the answer
arrived. stdio's `REQUEST_TIMEOUT = 10s` restarts for every line (`mcp_stdio.rs:142-148`), so a child
emitting a harmless line every 9 s loops forever; three round trips get three separate budgets and no
total deadline. The dispatcher imposes no deadline, so any future arm inherits only its author's care.

### F4. Signature / authenticity verification (CONFIRMED absence)
Nothing proves a result came from the registered server. The HTTP path checks neither `jsonrpc` nor
response `id` (`mcp_http.rs:111-129`, contrast `mcp_stdio.rs:160`), pins no TLS credential, and ignores
the addresses the guard validated (F1). Tool descriptions and `inputSchema` are copied verbatim
(`mcp_http.rs:164-173`) into the model's tool list: a substituted response is a prompt-injection
channel. The stdio arm allow-lists a *name*: `program_allowed` matches basenames (`domain/mcp.rs:260-270`)
and the child inherits `PATH` (`mcp_stdio.rs:68`), so a writable PATH entry named `python3` is executed
as trusted; no hash, signature or canonical path is checked at spawn. Adjacent: the plugin fetcher
checks a pinned SHA but not a content signature (`plugin.rs:143-150`) — out of scope, noted for parity.

## Concrete hardening (prioritized, with risks)

**P0 — Canonical transport + coherent record (F1).** Parse `enum TransportKind { Http, Stdio }` once at
registration; refuse unknown values at dispatch instead of defaulting to HTTP; assert `url.is_some()`
for HTTP and `command.is_some()` for stdio. *R1:* legacy/garbage rows now fail where they silently used
HTTP — externally visible, record in `docs/PARITY.md` and await approval; keep `http`/`stdio` accepted.

**P0 — Carry the verdict to connect (F1).** Use the already-returned `CheckedOutbound`: connect to a
validated address via reqwest `resolve_to_addrs` with SNI/Host from the hostname, re-check the address
at connect, and log the resolved set in the D-05 audit. *R2:* pinning an address can break DNS failover;
re-validate on every retry and fail closed if a new address is private.

**P1 — Typed errors + one breaker (F2).** Map network/timeout/`429`/`5xx` to `AppError::Unavailable`
(honour `Retry-After`), keep SSRF/config/`4xx` as `Rule`. Add a per-server `Semaphore` (~4) and a
circuit breaker with jittered half-open probes in the dispatcher, shared by both arms. *R3:* a breaker
can hide recovery and stack with the REQ-024 run retry — choose ONE retry owner and document it.

**P1 — One end-to-end deadline (F3).** Wrap each port method in a single `tokio::time::timeout`
(~25 s HTTP, 12 s stdio) returning a typed timeout error; add a 5 s `connect_timeout` and bound the
whole stdio `rpc()`, not each line. For SSE, consume `bytes_stream()` and stop at the first complete
`data:` event under an idle timeout. *R4:* slow legitimate tools get cut off — DEVIATION candidate,
record in `docs/PARITY.md` if observable results change.

**P1 — Authenticity checks (F4).** Require `jsonrpc == "2.0"` and a per-call unique `id` echoed back
(never reuse 1); add an optional per-server pinned SPKI hash. For stdio, resolve the allow-listed
program to a canonical absolute path at startup from a trusted PATH, record and verify its sha256
before spawn, drop inherited `PATH`, set an explicit `current_dir`. *R5:* pinning breaks rotating
CDNs — opt-in only; self-updating runtimes cannot be hashed, so document the trusted-path/hash list
as a deployment duty rather than claim a signature scheme that does not exist.

**P2 — No blind retry of side-effecting calls (F2/F4).** Retry `tools/call` in a new run only with a
server-supplied idempotency key or a recorded outcome for that unique id; otherwise fail permanently
with a clear message. *R6:* reduces automatic recovery for stateful servers — make it per-server
config and keep tool results inside one run.

**P2 — Dispatcher tests (all).** Inject stub arm transports in `#[cfg(test)]`/`tests/mcp_dispatch.rs`:
unknown transport refused; `http`/`stdio` route correctly; `stdio`+url and `http`+command refused;
deadline against a stalled arm; breaker opens then half-opens; mismatched response `id` rejected;
`5xx` is `Unavailable` while an SSRF refusal is `Rule`. Put `req_033_...` in test names.

## Open questions (route to docs/STATE.md)

- Refuse unknown `transport` strings, or keep the HTTP fallback for legacy rows? (DEVIATION gate.)
- Who owns retries: the dispatcher breaker or the REQ-024 run-level retry? Stacking is unsafe.
- Per-server config knobs for SPKI pinning and stdio hashing, or deployment-wide?
- Are shortened timeouts an externally visible change needing a parity entry?

## Status

All findings CONFIRMED by source reading except the noted GUESS; no runtime verification, no code
changed (read-only worker). Line refs are against `.worktrees/before-m5`.
