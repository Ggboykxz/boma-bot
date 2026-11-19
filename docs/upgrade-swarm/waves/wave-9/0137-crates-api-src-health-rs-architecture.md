# v0.4.0 architecture proposal — `crates/api/src/health.rs` (probe structure)

Worker #137 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no code changed).

Status of claims: "current state" statements are CONFIRMED by direct reads of `health.rs`,
`state.rs`, `router.rs`, `openapi.rs` in `crates/api`, `crates/app/src/health.rs`, `config.rs`
and `spec/01-requirements.md` (REQ-053/054) in this worktree. Forward-looking items are proposals
(GUESS until implemented, tested and measured).

## Current structure (CONFIRMED)

- One module hosts five unrelated handlers: liveness (`:99`), readiness (`:114`), metrics (`:135`),
  worker health (`:164`) and auth discovery (`:186`, REQ-040). Only the first three are probes;
  `metrics` is an operator surface gated by `BOMA_METRICS` (`:136-138`), and `auth_capabilities`
  belongs to the auth surface. All are wired flat in `router.rs:29-35`.
- DTOs `Liveness` (`:24`), `ReadinessBody` (`:31`), `InternalHealth` (`:82`).
  `ReadinessBody::from_capabilities` (`:61-75`) hand-copies 11 `Capabilities` fields; the only
  test (`req_053_...`, `:197-238`) checks documented keys are *present*, not that no field is
  silently dropped or renamed on the wire.
- `liveness` is a memory-only constant, always `200` (`:99-101`). `readiness` runs
  `state.readiness().report(timeout)`, maps ready -> `200` body, else logs failing names at `warn`
  and returns a generic `503` via `ApiError::unavailable()` (`:114-128`). No failure detail reaches
  the unauthenticated wire (REQ-054): this split is the load-bearing security property.
- Probes are sequential, each wrapped in `tokio::time::timeout(readiness_timeout)`
  (`crates/app/src/health.rs:56-76`). `config.rs:114-119` only enforces
  `readiness_timeout <= request_timeout`; N hanging checks can sum to N x timeout, past the router's
  `TimeoutLayer`, surfacing `408` instead of the intended structured `503`.
- `ReadinessReport` (`crates/app/src/health.rs:81-97`) carries only failing names (`&'static str`):
  no per-check status, latency or reason. `internal_health` (`:164-170`) is a stub — `ok: true`,
  `last_completed_at: None` — with an explicit M1-heartbeat TODO (`:165-166`).
- Wire-shape inconsistency: `InternalHealth` uses `#[serde(rename_all = "camelCase")]` (`:81`)
  (`lastCompletedAt`), while `Liveness`/`ReadinessBody` keep snake_case fields. Both are frozen by
  `spec/golden/health.json` and `ready.json` and by the OpenAPI contract test.
- Header policy is implicit: probes rely on the router's global `Cache-Control: no-store`; only
  `metrics` sets its own (`:143-146`). No test pins `no-store` on a probe response.
- `/healthz` and `/readyz` alias the same handlers and are deliberately absent from the generated
  document (`router.rs:20-22`, `openapi.rs:72-80`).

## Guiding principle for v0.4.0

Probes must stay unauthenticated, secret-free, cheap and deterministic. Shrink and type the probe
layer; keep every public path, alias, status code, header and body byte-for-byte. Any externally
visible delta is a DEVIATION to record in `docs/PARITY.md` before shipping (AGENTS.md).

## Upgrades (5 concrete items)

1. **Decompose the module: `health.rs` = probes only.** Move `metrics` into `metrics.rs` and
   `auth_capabilities` into the auth surface (`authcap.rs`/`authroutes.rs`); leave liveness,
   readiness and `internal_health` here as the single "ops probes" unit.
   *Risk:* relocating handlers changes utoipa path registration and can silently alter the OpenAPI
   document, the "ops"/"auth" tag grouping, or route order. *Mitigation:* keep the
   `#[utoipa::path]` annotations verbatim, re-register them in `openapi.rs`, and gate on the
   contract test against `spec/openapi.yaml` plus the health/ready goldens; no alias or path moves.

2. **Bound the whole readiness pass, not just each check.** Wrap `Readiness::report` in an outer
   deadline in the handler (or run checks concurrently with one total budget in
   `crates/app/src/health.rs`), so N slow probes cannot exceed `request_timeout` and leak a `408`.
   *Risk:* concurrency adds pool contention under simultaneous probes and reorders the per-check
   `warn` logs; mapping the outer timeout to `503` changes a wire-visible status where `408` appeared
   before (DEVIATION). *Mitigation:* keep per-check timeouts and static reasons, add an outer
   timeout that returns the same `ApiError::unavailable()`, test with a hanging probe set asserting
   response <= `request_timeout` and a structured `503`, and record the status change in PARITY.

3. **Introduce a typed internal readiness detail with a strict wire boundary.** Extend
   `ReadinessReport` to hold per-check `(name, status, duration_ms, static_reason)` for structured
   logging, while the public `ReadinessBody` and the `503` body stay exactly as today (names only
   in logs, nothing sensitive on the wire).
   *Risk:* richer values invite leaking connection targets or timing into the unauthenticated reply,
   violating REQ-054. *Mitigation:* keep the detail type non-`Serialize` and confined to `tracing`;
   add a test that both `200` and `503` serialised bodies contain no URL/credential and match the
   golden shape; keep `&'static str` static reasons only.

4. **Fix the probe response contract: one projection, explicit headers.** Replace the manual
   `from_capabilities` copy with a single source of truth (e.g. `Capabilities -> CapabilityView ->
   ReadinessBody`) and pin `Cache-Control: no-store` on each probe handler rather than relying on
   global middleware. Document and test the camelCase/snake_case split instead of letting it drift.
   *Risk:* handler-level headers can double up with the router's `if_not_present` layer; a "cleanup"
   of field casing would break both goldens and the OpenAPI contract (DEVIATION). *Mitigation:*
   use `if_not_present` semantics, assert exactly one `no-store` per probe in an integration test,
   add a test that every `Capabilities` field is projected exactly once, and keep casing unchanged.

5. **Make `internal_health` truthful from a worker heartbeat port.** Replace the stub `ok: true` /
   `None` with a value projected from a `WorkerHealth` port (last completed timestamp + a configured
   stale threshold), so the supervisor probes real progress instead of a constant.
   *Risk:* this changes externally visible body semantics (DEVIATION) and couples the probe to
   worker scheduling; a too-tight threshold causes spurious supervisor restarts. *Mitigation:* keep
   JSON field names and shape, source the threshold from validated config, add REQ-053 tests for
   fresh vs stale vs no-job-yet, record the deviation and await approval before shipping.

## Cross-cutting risks / sequencing

- Sequence 1 -> 4 -> 3 -> 2 -> 5: establish the probe-only module and pin the wire contract/tests
  first (the safety net), then add typed detail, then the deadline, then the heartbeat last (it is
  the only genuine behavior change).
- Nothing here should add a dependency; items 1-4 are structural/test-only. Preserve all aliases,
  paths, status codes, headers and golden bodies; log every externally visible delta in PARITY.
- Gate each step with `cargo fmt --check`, `clippy -D warnings`, unit + `tests/` integration, the
  golden replays and the OpenAPI contract test; keep REQ ids in test names (`tools/trace_check.py`
  counts declarations, not comments).

## Out of scope for this module

The internals of `Capabilities` construction (`crates/app/src/health.rs`), the router's layer order
(`router.rs`, worker #149) and the metrics renderer (`crates/app`) are other modules; this proposal
only covers how the health/ready probes are structured, typed and exposed at the HTTP boundary.
