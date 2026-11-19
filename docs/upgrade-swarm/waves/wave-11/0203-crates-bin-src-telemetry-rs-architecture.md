# Worker 0203 — `crates/bin/src/telemetry.rs` — architecture proposal (v0.4.0)

- Lens: architecture. Requirement: REQ-054 (Observability) / spec 4.6.
- Status: PROPOSAL only. Read-only except this file. No code changed.

## What I inspected (CONFIRMED)
- Full `crates/bin/src/telemetry.rs` (50 lines): one `init(&TelemetrySettings) -> anyhow::Result<()>`
  builds an `EnvFilter`, then a `tracing_subscriber::fmt()` builder with two branches
  (`Json` adds `.with_current_span(true).with_span_list(true)`; `Pretty` is plain), installed via
  `try_init()`. One test, `req_054_an_invalid_filter_is_an_error_and_not_a_silent_fallback`.
- `crates/bin/src/config.rs:62-78,346-348` — `TelemetrySettings` has only `filter: String` and
  `format: LogFormat`; sourced from `BOMA_LOG_FILTER` (default `info`) and `BOMA_LOG_FORMAT`.
- `crates/bin/src/cli.rs:93-97` and `crates/bin/src/automation.rs:131-141` — two places assemble
  settings and call `telemetry::init`; the automation path reads env lazily with `std::env::var`,
  not the typed `Config` (CONFIRMED).
- `crates/bin/Cargo.toml:19-34` — deps include `tracing`, `tracing-subscriber`, `anyhow`; **no**
  `opentelemetry` / `opentelemetry-otlp` crate (CONFIRMED).
- `.env.example:20-21` documents only `BOMA_LOG_FORMAT` and `BOMA_LOG_FILTER`; no OTLP vars.
- `spec/01-requirements.md:441-447` requires "OpenTelemetry export ... available";
  `spec/04-nonfunctional.md:96-109` requires optional OTLP, absent config = "off" not "degraded",
  request/trace/span from `traceparent`, and redaction inside the log path (CONFIRMED).
- `crates/api/src/correlation.rs:29-52,66-102` — correlation is a plain struct with manually
  generated `Uuid`-based `trace_id`/`span_id`, injected as log *fields* at `correlation.rs:177-202`;
  it is not backed by a `tracing` span context, so telemetry.rs sees none of it (CONFIRMED).
- `crates/api/src/router.rs:92-93`, `public.rs:738-830` — CSP reporting exists at `POST /api/csp-report`;
  `health.rs:130-148` and `router.rs:34` expose `/metrics`. The gap is at the binary/subscriber edge.

## Current architecture (CONFIRMED)
- `init()` is a single monolithic function: filter parsing, layer selection, and global installation
  are fused, so the subscriber topology cannot be built or inspected in a unit test.
- `try_init()` installs a **global** subscriber; a process that calls `init()` twice fails by design
  (documented). Which command paths might call both `cli.rs` and `automation.rs` in one process is
  unverified (GUESS: none today, but the invariant is unenforced).
- No OTel layer/exporter, no service name, no sampling, no shutdown/export flush — REQ-054's
  "OpenTelemetry export available" clause is unimplemented at this layer (CONFIRMED).
- No redaction layer here; spec 4.6 puts redaction/pseudonymisation in the log path, not call sites.
- Env config is split: typed for `cli.rs`, ad-hoc for `automation.rs` — two sources of truth.

## Gaps against the architecture bar
1. One fused `init()`; no composable layer builder, no injectable surface (testability).
2. OTel/OTLP absent despite a MUST requirement; no typed settings, no graceful exporter shutdown.
3. Two config surfaces assembled independently; no single fail-fast validation point.
4. Correlation data (trace/span ids, `traceparent`) not connected to subscriber spans — duplicate,
   isolated authorities rather than one trace context (partly GUESS on intended end state).
5. No in-path redaction; adding tests that call `init()` will collide on global subscriber state.

## Upgrades (v0.4.0)

### U1 — Split construction from installation
Extract `pub(crate) fn build_subscriber(settings) -> Result<ImplSubscriber>` (pure w.r.t. global
state) leaving `init()` as `try_init` only; return a `TelemetryGuard` that owns export shutdown.
- Why: topology becomes testable without touching the global subscriber; prerequisite for U2/U5.
- Risk: layered `tracing_subscriber` generic types get verbose; keep the existing JSON shape
  (`current_span` + `span_list`) byte-identical or log output is a DEVIATION.

### U2 — Optional OTLP trace layer (REQ-054)
Add typed `OtelSettings` (`enabled`, `endpoint`, `service_name`, `sample_ratio`) to `TelemetrySettings`;
when absent/disabled install nothing (spec: off, not degraded). Attach an `opentelemetry-otlp`
OTLP-**HTTP** span exporter (reuse `reqwest`+rustls, avoid tonic/gRPC) via a `tracing-opentelemetry`
layer; flush on shutdown through the U1 guard. `cargo add` with docs.rs API verification; record
name/version/license in `docs/DEPENDENCIES.md`; ADR for HTTP-vs-gRPC and sampler choice.
- Why: satisfies the MUST export clause without adding a service when unconfigured.
- Risk: new dependency weight; batch export needs a Tokio runtime + drain at exit or spans are lost;
  endpoint/auth are secrets—keep them out of logs; enabling export changes observed behaviour
  (operator opt-in, not a silent default).

### U3 — One typed config surface
Move all telemetry env reading into `Config::from_env`; delete the ad-hoc `std::env::var` block in
`automation.rs:134-139` and reuse one `TelemetrySettings` builder from both call sites.
- Why: single source of truth, fail-fast, no duplicated defaults.
- Risk: byte-identical env names/defaults must be preserved (DEVIATION guard); confirm no process
  calls `init()` twice once the paths are unified.

### U4 — Unify correlation with the trace context
Bridge `crates/api/src/correlation.rs` into real `tracing` spans (extract `traceparent` into the
OTel context) so the ids exported by U2 and the ids logged at `correlation.rs:177` are one authority.
- Why: spec 4.6 requires trace ids parsed from `traceparent` and carried through the queue.
- Risk: cross-crate change (api + bin); current `Uuid`-based ids and OTel 128-bit ids must be
  reconciled without changing the public `x-request-id` contract (DEVIATION); GUESS on effort.

### U5 — In-path redaction + testable init
Add a redaction layer at the subscriber (spec 4.6) and a `#[cfg(test)]` constructor that builds
without `try_init`, so multiple REQ-054-named tests can assert filter/format/redaction.
- Why: privacy rule today is left to call sites; one global test already blocks expansion.
- Risk: redaction changes rendered log values—make it default-on for secrets only and record any
  output-shape change in `docs/PARITY.md`.

## Suggested order (small steps, test-first)
U1 → U3 (parity-locked refactors) → U2 (opt-in export) → U5 → U4 (cross-crate). Each step:
`cargo fmt --check`, `clippy -D warnings`, REQ-054-named tests.

## Verification for v0.4.0 (REQ-named)
- `fn req_054_invalid_filter_is_an_error_not_a_silent_fallback()` (exists).
- `fn req_054_no_otel_layer_is_installed_when_unconfigured()` (off ≠ degraded).
- `fn req_054_optional_otlp_settings_validate_endpoint_and_sample_ratio()`.
- `fn req_054_subscriber_output_shape_is_unchanged_for_json_and_pretty()`.
- `fn req_054_secret_values_are_redacted_in_the_log_path()`.
- Contract check: `.env.example` names every `BOMA_OTEL_*` var the binary reads.

## Provenance
CONFIRMED = read from named files this session. GUESS = design judgment / parity or effort uncertainty.
