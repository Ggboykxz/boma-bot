# 0177 — `crates/app/src/metrics.rs` — architecture (v0.4.0)

- Worker: 177 · Lens: architecture · Target: v0.4.0 · Read-only review; no code edited.
- Module: 121-line hand-rolled Prometheus text exposition in the `app` crate (use cases layer).
- Spec anchors: REQ-054 (observability, MUST), `docs/QUALITY_BAR.md` (observability, fuzz/property
  tests on every external-format parser, rustdoc on public library items), ADR-024 (metrics flag
  decision), `AGENTS.md` (no externally visible change without a recorded DEVIATION).
- Read here: `metrics.rs`, `service.rs` (`Services.metrics`, `with_metrics`), `worker.rs`,
  `usecases.rs`, `crates/api/src/{health,correlation}.rs`, `crates/api/tests/http_surface.rs`,
  `crates/bin/src/{config,commands}.rs`, `spec/01-requirements.md`, `docs/QUALITY_BAR.md`,
  `docs/STATE.md`.
- CONFIRMED = read in this session; GUESS = inferred.

## Structure today (CONFIRMED)

`Metrics { inner: Mutex<BTreeMap<String, u64>> }`. The map key is the **whole exposition line**
(`name{label="value",…}`, built by `key_of`), the value is the `u64` counter. Public API: `new`,
`increment(name, labels)`, `add(name, labels, n)`, `render()`. Labels are sorted by name so
`{a,b}` and `{b,a}` collide; `escape` handles `\`, `"`, `\n`; a poisoned lock drops increments
by design so telemetry never fails a request. Live series (all call sites read):
`boma_runs_started_total{trigger}`, `boma_runs_finished_total{status}` (3 call sites),
`boma_runs_first_response_total`, `boma_run_first_response_ms_total` (sum),
`boma_http_requests_total{route,status_class}`, `boma_csp_violations_total`. Output is
`text/plain; version=0.0.4`, gated by `BOMA_METRICS` (404 otherwise). Tests: two unit tests
(aggregation/sorted labels, stable sorted render) plus HTTP-level REQ-054 flag/content-type tests.

The shape is small and correct for its traffic; the debt is structural, not behavioural.

## Architecture gaps (CONFIRMED unless marked)

- **No metadata.** The 0.0.4 body has no `# HELP`/`# TYPE` lines, so a scraper cannot distinguish a
  counter from an untyped series; the contract claiming `version=0.0.4` is only partly honoured.
- **Stringly-typed surface.** Names and label names are bare literals at 12+ call sites; the
  same name is repeated three times. A typo silently creates a new series; no compile-time check.
- **Storage coupled to wire format.** The map key is a pre-baked line, so escaping or format
  changes require re-keying; there is no per-family label schema, no cardinality bound, and no
  way to enumerate families for metadata emission.
- **Missing coverage on the encoder.** `escape`/`key_of` are an external-format encoder with only
  two happy-path tests — no property/fuzz test (quality bar), no poisoned-lock or CR/unicode case.
- **Naming semantics.** `boma_run_first_response_ms_total` uses the `_total` counter suffix for a
  millisecond sum; with `# TYPE counter` a scraper's `rate()` would mean "ms/s", not events/s
  (GUESS — confirm the intended PromQL before emitting TYPE). Kept today for parity.

## Concrete upgrades for v0.4.0 (each with a risk)

1. **A typed metric catalog.** Define one `Metric` descriptor per family (name, HELP text, TYPE,
   ordered allowed label names) as `pub const` items in `metrics.rs` with rustdoc; make every
   `increment`/`add` call site in `worker.rs`, `usecases.rs` and `api/correlation.rs` reference a
   catalog constant instead of a literal. Add a test that no call site uses a raw `&str` name.
   *Risk:* touches three crates; emitted names, labels and values must stay byte-identical or the
   REQ-054 HTTP tests and any scraper diff become a DEVIATION needing owner sign-off.

2. **Emit `# HELP`/`# TYPE` lines.** Derive them from the catalog and prefix each family in
   `render()`, producing valid 0.0.4. Keep lines sorted and stable.
   *Risk:* changes the `/metrics` body shape. Tests use `contains(...)` and a `text/plain` prefix
   so they should survive (GUESS — run them), but update any golden/scraper fixture explicitly and
   record the shape change; decide the TYPE for the `_ms_total` sum (counter vs untyped) before
   shipping, since a wrong TYPE invites misleading `rate()` queries.

3. **Split the data model from rendering.** Store a structured key (`family: &'static str` plus
   `labels: Vec<(String, String)>`) in the map; make `key_of`/`render` the only code that formats a
   line. This enables metadata, label validation and future counting without re-keying.
   *Risk:* a wider refactor of the hot module; must preserve both REQ-054 invariants exactly —
   label-order canonicalisation and deterministic sorted output. Add a property test asserting
   `render()` is always sorted and every line re-parses, so the refactor cannot regress silently.

4. **Harden the encoder and bound cardinality.** Validate label names and non-empty metric names,
   document/escape `\r`, and add a `MAX_SERIES` cap that drops new series past the limit while
   counting them in an internal `boma_metrics_dropped_total`. Add proptest/fuzz coverage for
   `escape`+`render` (encoder round-trip, no injected newlines) and a poisoned-lock test.
   *Risk:* a cap changes behaviour under a label-value bug or probe — bounds keep the endpoint
   safe but hide series; the cap value and drop-logging policy need a STATE note, and the internal
   counter must carry no PII (route templates/status classes only, never ids or addresses).

5. **Take the lock off the render path.** `render()` currently builds the entire body while
   holding the `Mutex`, blocking every HTTP increment for the scrape duration; increments also
   serialise on one lock on every request. Snapshot-then-format (clone under lock, format outside)
   is the minimal fix; sharded atomics are the full fix.
   *Risk:* snapshotting allocates under the lock and must not lose an increment between clone and
   render; atomics per dynamic label set are complex. For a low-traffic local-first product this
   may be over-engineering — treat as conditional on a measured contention benchmark, and keep
   the "telemetry never fails a request" invariant and exact REQ-054 counts (no lost increments).

## Explicit non-goals

No registry crate dependency (the file's rationale stands), no gauges/histograms or OTLP exporter
in this slice, no rename of the `_ms_total` sum without an owner-approved DEVIATION, no new metric
outside REQ-054, and no change to the `BOMA_METRICS` flag/404 policy. Every upgrade keeps the two
existing REQ-054 unit tests and the HTTP flag/content-type tests green; wire-visible shape changes
(2, 4) go through `docs/PARITY.md`/STATE, not silently.
