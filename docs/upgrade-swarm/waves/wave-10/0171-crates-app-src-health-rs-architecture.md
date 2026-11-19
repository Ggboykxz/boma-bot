# v0.4.0 Architecture Proposal — `crates/app/src/health.rs` (worker #171, lens: architecture)

## Evidence and limits
CONFIRMED (read from the file): `ReadinessCheck` is an `async_trait` port with `name() -> &'static str`
and `check() -> Result<(), &'static str>`; `Readiness` owns `Vec<Arc<dyn ReadinessCheck>>`; `report`
loops **sequentially**, wrapping each probe in its own `tokio::time::timeout` and collecting failing
names; `ReadinessReport` exposes `is_ready()` and `failing() -> &[&'static str]`; `Capabilities` has
11 public fields (four free-form "enum" strings plus a standalone `degraded: bool`) and a `Default`;
four unit tests cover pass / down / timeout / secret-free defaults.
GUESS (not checked, read-only scope): callers in `api` and the binary, the wire schema in
`spec/openapi.yaml`, and whether `/health` vs the AGENTS-mandated `/healthz` is the public contract.

## What is already right (keep)
- The port lives in `app`, correct layer; the endpoint is unauthenticated and the error is a static
  reason, so a driver error cannot leak a connection string (REQ-054).
- Every outbound probe has a timeout (REQ-053); liveness/readiness are deliberately separated in docs.

## Concrete upgrades (change → risk)

1. **Bounded concurrent probing with a single shared deadline; stable failure order.**
   Change: add `report_with_deadline(instant)` using `FuturesUnordered`/`buffer_unordered` with a
   concurrency cap and one overall budget; collect outcomes by index, then keep `failing()` in
   declaration order. Keep `report(timeout)` as a thin wrapper so tests/callers do not change.
   Risk: parallel probes create a startup thundering herd on the DB/blobstore; a cap plus a shared
   deadline is required. Completion order differs from declaration order — must re-sort or the
   existing `assert_eq!(failing(), ["database"])` semantics break silently.

2. **Typed, redaction-safe probe outcome with latency for observability.**
   Change: introduce `ProbeOutcome { name, result: ProbeResult, elapsed }` where `ProbeResult` is
   `Ok | Failed(&'static str) | TimedOut`; emit per-probe duration via `tracing` and let the API/metrics
   layer aggregate. Keep `check() -> Result<(), &'static str>` — the redaction boundary stays in the
   port. Add `report_detailed()` rather than changing `failing()`'s signature.
   Risk: any `Debug`/`Display` that carries the raw driver error must stay out of logs or REQ-054 is
   violated; more types in `app` must still have rustdoc and pass pedantic clippy.

3. **Make `Capabilities` states unrepresentable, without changing the wire format.**
   Change: replace the four string enums (`runtime`, `sandbox`, `jobs`, `realtime`) with `serde`
   enums whose `rename` values are exactly the current strings, and derive `degraded` from the actual
   subsystem state instead of storing an independent bool that can contradict it.
   Risk: serde renames must match the spec byte-for-byte; a mismatch is a DEVIATION requiring
   `docs/PARITY.md` and
   approval, and will fail the OpenAPI contract test. GUESS: confirm the schema before touching it.

4. **Encode "at least one probe" in the type.**
   Change: drop `Default` for `Readiness` (or gate it behind `#[cfg(test)]`) and add a validated
   constructor returning `Result<Readiness, EmptyProbeSet>`; make `is_empty()` impossible on a built
   value so the "binary refuses to start" invariant is enforced by the type, not a runtime check.
   Risk: `Default`/`Clone` may be used by config defaults or startup code (unverified); removing them
   could break callers — grep first. Keep `len()` for existing tests.

5. **First-class liveness type and endpoint-name reconciliation.**
   Change: add a `Liveness` type that is pure/in-memory and flips false during graceful shutdown, so
   `/health` answers from memory as documented; document probes as a port implemented in `infra`.
   Reconcile `/health` + `/ready` (file docs) against `/healthz` + `/readyz` (AGENTS.md) and the spec
   before changing any path.
   Risk: adding a shutdown flag needs wiring to the shutdown signal and new tests; renaming public
   paths is a DEVIATION — do not do it silently.

## Cross-cutting risks / acceptance
- No public field, path, or error string may change without a `docs/PARITY.md` DEVIATION entry.
- Preserve the four existing test names and add `req_053_*` / `req_054_*` names for new behavior.
- Keep `app` IO-free: implementations of `ReadinessCheck` stay in `infra`; only orchestration lives here.
- Verify with `cargo fmt --check`, `clippy -D warnings`, and the unit tests before proposing any merge.
