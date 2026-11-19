# 0204 — `crates/bin/src/telemetry.rs` — reliability proposal (v0.4.0)

Worker #204 · module `crates/bin/src/telemetry.rs` · lens: reliability · READ-ONLY review of
`rebuild-workspace/.worktrees/before-m5` at v0.3.0-era source. Proposals only; no code changed.
Claims: **CONFIRMED** = read in this tree this session; **GUESS** = inference needing a test.
Scope: this 50-line file plus its call sites (`cli.rs:93-97`, `automation.rs:131-141`),
`config.rs:62-78,307-316,346-349`, `correlation.rs:105-211`, `commands.rs:425-457`, and spec
`04-nonfunctional.md:96-109`. Anchors: REQ-053/054, AGENTS.md "observability", "secrets never logged",
"graceful shutdown that drains in-flight requests".

## What is already right (keep; do not re-propose)

- Invalid filters fail loudly with the variable name (`telemetry.rs:16-18`, REQ-054 test at 38-49):
  no silent downgrade to `info` (CONFIRMED).
- `BOMA_LOG_FORMAT=json` emits one JSON object per line with request/trace/span ids; `correlation.rs`
  logs the route *template*, never the raw path, so invite/share tokens and installation ids do not
  reach logs today (CONFIRMED, `correlation.rs:105-158`).
- `Config` redacts the database URL in `Debug`/`Display` (`config.rs:370-382,786-795`) and the
  provider key is `secrecy::SecretString` (CONFIRMED).

## Failure modes

### F1. Exporter failure — no exporter exists, and none is designed to fail safe (medium, forward-looking)
Spec 4.6 requires **optional OTLP tracing** where absent configuration means "off", not "degraded"
(CONFIRMED). This module installs only a `fmt` subscriber; there is no OTLP layer, no dependency,
and no config knob (CONFIRMED: `Cargo.toml:19-34`, `TelemetrySettings` has only `filter`/`format`).
Any v0.4.0 exporter added naively creates three new failure modes: (a) an unreachable collector
blocking or panicking on the request path; (b) an unbounded retry queue growing without limit;
(c) exporter logs re-entering tracing and producing a feedback loop. There is currently no retry,
timeout, circuit breaker or span-drop counter to inherit (CONFIRMED absence). GUESS: a collector
outage is the most likely production incident once OTLP is enabled.

### F2. Secret leakage in spans — redaction lives at call sites, not in the log path (high)
JSON mode sets `.with_current_span(true).with_span_list(true)` (`telemetry.rs:23-24`), so **every**
log line serialises the full span ancestry and all span fields. Redaction is enforced only by
discipline at individual call sites (`correlation.rs` template, `config.rs` custom `Debug`); a single
`info_span!("...", token = %raw)` anywhere puts the value into every descendant line. Spec 4.6 says
redaction/pseudonymisation belongs *in the log path*, not to call sites (CONFIRMED) — this module
has no such layer. Additional confirmed exposure: `Config` derives `Debug` and three bearer fields
are plain `String` (SCIM/automation/billing webhook — see worker 0197), so any `{config:?}` reaches
logs unredacted; `check_config` uses `%config` (Display, safe) today (CONFIRMED `commands.rs:427`).
GUESS: an `info!(?config)` added later is the realistic leak vector. The invalid-filter error echoes
the raw filter string (`telemetry.rs:18`); low risk but worth noting.

### F3. Shutdown flush — no handle, no drain, last events lost (medium)
`init()` returns `anyhow::Result<()>` and installs a *global* subscriber via `try_init()`, keeping no
handle (`telemetry.rs:16-32`). `commands.rs::shutdown_signal()` logs "shutting down" and returns
(`commands.rs:456`); the process then exits with no explicit flush. A `fmt` writer to stdout is
line-buffered so today's loss is small, but an OTLP `BatchSpanProcessor` buffers spans in memory —
without `shutdown()`/`force_flush()` the final events are silently dropped on every deploy (GUESS
until an exporter exists, then CONFIRMED). A global dispatcher also cannot be uninstalled, so a
per-call guard must own the *provider*, not the subscriber. `try_init()` is not idempotent: a second
valid `init()` (tests, `automation.rs`) fails "subscriber already installed" (CONFIRMED shape).

## Concrete hardening (v0.4.0)

1. **P1 — return a flush guard.** Change `init()` to `anyhow::Result<TelemetryGuard>` where the guard
   owns the OTLP provider handle and exposes `async fn shutdown(self)` plus a `Drop` fallback that
   calls `force_flush()` with a bounded timeout (e.g. 5 s) and logs a dropped-span count. Hold the
   guard in `cli.rs::run()` and call `shutdown()` after `shutdown_signal()` resolves so the
   "shutting down" line is emitted first. Keep the global fmt dispatcher for `tracing` macros.
   Risk R1: signature touches `cli.rs:95` and `automation.rs:141`; `Drop` cannot be async — do not
   block the Tokio runtime (use an explicit async path; `block_in_place` only as a last resort).
2. **P2 — optional, fail-open OTLP layer.** Add config `BOMA_OTLP_ENDPOINT` (absent ⇒ layer not
   installed, not "degraded"), `opentelemetry` + `opentelemetry_sdk` + `opentelemetry-otlp` via
   `cargo add` (docs.rs checked; record name/version/license in `docs/DEPENDENCIES.md`). Use a
   bounded `BatchSpanProcessor` queue, a timeout on every export, drop-on-full with a counter, and a
   circuit breaker that logs once at WARN then goes quiet. Filter out the exporter's own target to
   prevent re-entrant spans. Risk R2: new deps and an operator-visible knob → document in
   `.env.example`; an unreachable collector must never change request outcomes (add a test).
3. **P3 — redaction layer in the log path.** Implement a `tracing_subscriber::Layer<S>` that scrubs
   known secret field names (`token`, `secret`, `api_key`, `authorization`, `password`, `*_secret`)
   and masks token-like values before the fmt and OTLP layers. Add a REQ-054 test that emits a seeded
   secret as a span field and asserts absence from captured JSON and from the OTLP span. Risk R3:
   heuristic masking mangles diagnostics; keep an explicit field-name deny-list, not a broad regex,
   and keep the correlation route templates as belt-and-braces.
4. **P4 — cap span-list verbosity.** Make `.with_span_list(true)` opt-in (default `false`) or gate it
   behind `BOMA_LOG_SPAN_LIST`; it amplifies both F2 leakage and per-line cost. Risk R4: changes the
   JSON log shape → DEVIATION candidate for downstream consumers; record in `docs/PARITY.md` and
   wait for owner approval before shipping.
5. **P5 — idempotent init for reuse/tests.** Cache the guard behind a `OnceLock`/`Mutex` so a second
   `init()` returns a clone/handle instead of failing, and let tests use `tracing::subscriber::with_default`
   for per-test filters. Risk R5: a process-wide guard means test-local filters cannot be swapped; add
   REQ-054 tests for redaction and flush that use a per-test subscriber, not the global one.

## Risks / trade-offs

- R1 (P1): `Drop`-based flush is best-effort only; the explicit `async shutdown()` must be the primary
  path and be called from every exit (serve, worker, automation), or spans are still lost.
- R2 (P2): exporter failure must be fail-open — a collector outage must not fail requests, not even
  slow them; backpressure drops spans and counts them rather than blocking.
- R3 (P3): redaction cannot be the *only* defence; keep secrets out of span fields at source too.
- R4 (P4): log-shape changes are externally visible to log consumers (DEVIATION gate).
- R5 (P5): test isolation vs. global subscriber — prefer `with_default` over weakening assertions.
- Cross-cutting: preserve every `req_0xx_...` test name and every `BOMA_*` name (traceability/compat).

## Explicit non-goals

- No rename of `BOMA_LOG_FILTER`/`BOMA_LOG_FORMAT` and no removal of the fail-fast filter parse.
- No changes to `correlation.rs` templates or the `Config` redaction work already assigned to 0197.
- No synchronous/blocking network call on the async runtime; no unbounded queues.

## Suggested rollout

Wave A: P1 + P5 (signature/guard, no wire change) with `cargo fmt --check`, `clippy -D warnings`,
REQ-054 tests after each step. Wave B: P3 redaction layer (REQ-054 test for the seeded secret).
Wave C: P2 optional OTLP with fail-open tests. Wave D: P4 shape change only after `docs/PARITY.md`
entry and owner approval. All line refs are against `.worktrees/before-m5`; nothing was executed here.
