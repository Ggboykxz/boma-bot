# Upgrade proposal — `crates/app/src/acp_provider.rs` (architecture lens)

Target: v0.4.0 · Module: `crates/app/src/acp_provider.rs` · REQ-061 (local CLI agents / ACP executors)
Status: PROPOSAL (read-only review; nothing implemented here). Claims below are CONFIRMED by
reading the current tree; forward-looking statements are marked GUESS.

## What the code does today (CONFIRMED)

- `acp_provider.rs` holds a purely static registry: `executors() -> &'static [ExecutorSpec]`,
  a `const fn` with one row (`opencode`, command `opencode`, args `["acp"]`). `ExecutorSpec` is
  `{ id, command, args }`, all `&'static` strings, `Copy + Clone`. Identity, spawn command and
  ACP subcommand are fused in one value; no version, environment, capability or protocol field.
- `AcpProvider` is stateless per turn: it resolves a spec at construction, spawns an `AcpSession`,
  renders prompt context and bridges `session/request_permission` to approval cards.
- Three call sites independently consume the registry:
  `api/src/rpc.rs` (`bots.detectedExecutors`, `bots.connectAcp`),
  `app/src/usecases.rs` (`attach_acp_agent` validates an id), and
  `app/src/worker.rs` (`provider_for` re-resolves the stored attachment each run).
- Detection is **not** in this module. `rpc.rs` owns `detected_executor_ids(probe)` and
  `executor_on_path(command)` (`--version`, 2 s timeout); it probes sequentially, collects a
  parallel `Vec<bool>`, then zips it back through `detected_executor_ids(|_| answers.next()...)`.
  The reply is `DetectedExecutorsReply { executors: Vec<String> }` — ids only, no status/reason.

## Architecture problem for v0.4.0

The registry is a const data table in `app`, but the *meaning* of "detected executor" is split
across three crates with three lookup patterns and one order-coupled zip. REQ-061's rationale
names Codex, Claude, OpenCode, Cursor and Cline, yet the matrix is one row; every new row must be
wired in three places and proven with a wire test. There is no single place that can answer
"which known executors are usable on this machine, and why not" — which is exactly what the UI
detection surface (`bots.detectedExecutors`) and attachment validation need.

## Concrete upgrades (architecture lens)

**U1 — Own detection in the registry module, not in `rpc.rs`.**
Add `DetectedExecutor { spec: ExecutorSpec, available: bool, reason: Option<&'static str> }`
and `detect_executors(probe)` in `acp_provider.rs`; make `detected_executor_ids` a thin adapter
that calls it. All three call sites then resolve ids through one function.
*Risk:* the RPC reply shape is a public interface (`Vec<String>` ids); enriching it is a
DEVIATION needing `docs/PARITY.md` + approval. Keep the wire type unchanged in v0.4.0 and expose
the richer type only inside Rust. Also note process probing is IO: keep `executors()` pure in
`app`, put the actual `--version` spawn in `infra` behind a port so `app` stays IO-free per the
layering rules.

**U2 — Break the order-coupled parallel-vector zip.**
Replace the `answers: Vec<bool>` + `detected_executor_ids(|_| answers.next()...)` in
`bots_detected_executors` with a keyed map `id -> bool`, so a spec added to the const table can
never be misassociated with another spec's probe result.
*Risk:* touching this path must keep `req_061_detected_executors_reports_only_known_ids` green
and preserve the invariant that only compiled-in ids can ever be reported. A regression here
silently attributes availability to the wrong executor — add a test that reorders/duplicates ids.

**U3 — Enrich `ExecutorSpec` with spawn/capability metadata via a `const fn` builder.**
Add optional per-row fields: `display_name`, `acp_args` (generalise `args`), `env` (static
key/value slice), and `protocol`/`capabilities` (e.g. supports permission bridge, supports
`session/new`). This makes adding a row a single declarative change and gives the UI display
names instead of raw ids.
*Risk:* more `&'static` fields complicate the `const fn` and the `Copy` bound every caller relies
on; each new row needs a wire-tested stub (`crates/testkit/fixtures/stub-acp-agent.py`) and the
real CLI args are unproven (STATE records `opencode acp` as a GUESS) — an unverified row would
fail runs legibly on real machines, so mark each GUESS until exercised.

**U4 — Single resolver for `attach` / `worker` / `rpc`.**
Introduce `resolve_executor(id) -> Result<ExecutorSpec, UnknownExecutor>` used by
`attach_acp_agent`, `provider_for` and connect/disconnect, replacing three hand-rolled
`executors().iter().find(...)` lookups. Give the error a typed variant carrying the id so
callers can emit a legible reason and list known ids.
*Risk:* `AppError::Rule(InvalidName)` is today's attach error (CONFIRMED); changing it is an
externally visible error-shape change. Keep the existing mapped error at the API boundary and
add the richer variant only internally; watch for a `app -> api` dependency cycle (resolver
must stay in `app`).

**U5 — Bound and cache detection; make the probe meaningful.**
Cache `--version` results with a short TTL and cap concurrency, and consider probing the actual
ACP subcommand rather than a bare `--version` (a version check does not prove `session/new`
works). This is the "detected executors" freshness story for the UI.
*Risk:* caching stale availability misleads the operator (an uninstalled/broken CLI shows as
detected); more subprocesses widen the PATH-hijack surface — keep `kill_on_drop`, the 2 s
timeout, no shell, and never treat probe stdout/stderr as trusted data. GUESS: exact TTL needs a
decision and likely an ADR before implementation.

## Sequencing / verification

U2 and U4 are low-risk refactors with no wire change — land first, keeping the three REQ-061
tests green. U1 then U3 move the detection structure into this module. U5 last, behind an ADR.
Every step: `cargo fmt --check`, clippy `-D warnings`, `cargo test --workspace --locked`, and
`tools/trace_check.py`; any reply-shape change goes through `docs/PARITY.md` + owner approval.
