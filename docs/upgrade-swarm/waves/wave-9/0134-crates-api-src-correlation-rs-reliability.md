# v0.4.0 reliability proposal — `crates/api/src/correlation.rs` (correlation / per-request logging)

Worker #134 · LENS=reliability · target v0.4.0 · READ-ONLY analysis (no code changed).

Status of claims: every "current state" statement below is CONFIRMED by a direct read of
`correlation.rs` (all 302 lines) in this worktree. The `in_scope` finding is CONFIRMED by reading
`tracing::Span::in_scope` semantics against the call at line 181. Forward-looking items are
proposals (GUESS until implemented and tested). No tests were run for this proposal.

## Current structure (CONFIRMED)

- `Correlation` holds `request_id`, `trace_id`, `span_id` (27-34). `from_headers` (40-54) parses
  `traceparent` (falling back to `mint_trace`) and honors `x-request-id` if "reasonable" (72-74),
  else mints a UUID v4. Absent/malformed headers never fail the request — by design (36-38).
- `mint_trace` (65-70) uses two `Uuid::new_v4()` calls; the span id is the low 64 bits masked and
  hex-formatted. Both ids are random; there is no parent/child relationship (fine for a root span).
- `parse_traceparent` (77-103) requires version `00`, exact lengths, hex digits, non-zero ids, and
  exactly four segments. It accepts *any* 2-hex flags value. Uppercase is accepted and lowercased.
- `log_http` (164-211) creates an `info_span!` with all three ids (173-180), runs the handler via
  `span.in_scope(|| next.run(request)).await` (181), increments a counter keyed by `route` and
  `status_class` (190-193), emits a completion line (194-204), and echoes `x-request-id` (205-209).
- `template` (106-158) maps known dynamic routes to templates but returns the **raw path verbatim**
  for anything unmatched (157); that raw value is also used as the metrics label (192).

## Guiding principle for v0.4.0

Make the three ids trustworthy and *actually attached* to every log emitted during a request, make
id generation fail soft, and stop treating client-controlled headers as authoritative. Keep
REQ-054's "honor well-formed inbound ids / echo `x-request-id`" behavior unless a change is filed
as a DEVIATION in `docs/PARITY.md` (AGENTS.md).

## Failure modes

1. **Propagation gap (highest severity).** Line 181 calls the *synchronous* `Span::in_scope` with a
   closure that only *constructs* the response future. `in_scope` exits the span before `.await`
   polls that future, so the `http.request` span is **not active while the handler runs**. Any
   `tracing` event from handlers/services lacks `request_id`/`trace_id`/`span_id`. Only the
   completion line (194-204) is correlated, and only because it repeats the fields manually. The
   stated goal of "a span ... through the log tree" (1-11) is therefore not met for inner logs.
2. **ID generation can panic.** `Uuid::new_v4()` (48, 66, 67) unwraps OS-RNG failure internally;
   an RNG error panics the task, dropping the request and contradicting "a failure never escapes as
   anything but a response" (162-163). No fallback path exists.
3. **Header spoofing / untrusted ids.** `is_reasonable_request_id` (72-74) accepts any 1-64 ASCII
   graphic string, so a client controls a log field and the echoed response header: quote/backslash
   forging in non-JSON log formats, short-token/PII stuffing, and reuse of one id across many
   requests (colliding correlation keys, defeating triage). `traceparent` accepts reserved flag
   bits (96-98), and `headers.get` silently takes the first of duplicate headers (42, 45).
4. **Unbounded metric cardinality / raw-value leak.** `template`'s fallback returns the raw path
   (157) and that string becomes a metric label (192). A client can mint unlimited distinct paths
   (`/whatever/<random>`), growing the metrics registry without bound and writing raw tokens/ids
   into telemetry — despite the module's own "never the raw path" promise (9-11).

## Upgrades (concrete hardening, with risks)

1. **Attach the span across the await.** Replace line 181 with
   `next.run(request).instrument(span).await` (`use tracing::Instrument;`), or enter the span inside
   an async block. Add a test that captures a `tracing` subscriber and asserts an inner event carries
   all three ids.
   *Risk:* `Instrument` changes enter/exit timing and can surface fields on more events, altering
   golden log captures. *Mitigation:* gate on a subscriber-capture unit test before/after and review
   any log-golden diff.

2. **Fail-soft id generation.** Use `Uuid::try_v4()` and, on RNG failure, fall back to a
   process-unique, monotonic value (atomic counter + process start instant) for `trace_id`/`span_id`
   and `request_id`; never panic on the request path.
   *Risk:* a counter fallback can collide after restart or across replicas. *Mitigation:* prefix
   the fallback with a per-process random boot id generated at startup (fail startup, not requests),
   and cover with a test that injects RNG failure.

3. **Treat inbound ids as untrusted input.** Keep honoring them for REQ-054 compatibility, but
   normalize `x-request-id` to `[A-Za-z0-9._-]`, cap at 64 chars, and log it under a distinct
   `client_request_id` field while the server-minted id remains the correlation key. Reject
   duplicate `traceparent`/`x-request-id` headers and restrict flags to `00`/`01`. Consider always
   minting a server id and echoing that instead.
   *Risk:* changing the echoed header value or rejecting previously accepted headers is
   wire-visible → DEVIATION in `docs/PARITY.md` and operator approval required. *Mitigation:* land
   normalization and duplicate rejection first behind tests; propose the "always mint" switch
   separately.

4. **Bound metric cardinality.** Make `template`'s unmatched fallback a constant (e.g.
   `"/unmatched"` or `"/{path}"`) so the metric label set is finite, and extend the explicit template
   list to cover every dynamic route rather than relying on the verbatim fallback.
   *Risk:* static paths lose their exact label in metrics. *Mitigation:* keep the full path only in
   a debug field for known-safe routes, add a test asserting `template("/random/abc")` is constant,
   and audit route list against the router to avoid silent gaps.

5. **De-duplicate and test the boundary.** Route both `x_request_id()` (58-61) and the inline echo
   (205-209) through one helper. Add property/fuzz tests for `parse_traceparent` and
   `is_reasonable_request_id` (per AGENTS.md "property or fuzz tests for every parser"), plus a
   propagation test (item 1) and the REQ-054 ids in all test names (`tools/trace_check.py` counts
   declarations).

## Cross-cutting risks / sequencing

- Order 1 → 2 → 4 → 3/5: fixing span attachment first yields the largest observability win with no
  wire change; spoofing hardening (3) carries the only likely externally visible change.
- Any change to echoed `x-request-id` or to which inbound headers are accepted is a DEVIATION —
  record in `docs/PARITY.md` and wait for approval before shipping (AGENTS.md).
- Correlation ids are created here but never injected into outbound HTTP calls; distributed traces
  break at the service boundary. That belongs to the HTTP client layer, not this file — noted as a
  propagation gap for a separate worker, not changed here.
- Verification gate for each step: `cargo fmt --check`, `clippy -D warnings`, unit + `tests/`
  integration, golden/contract suites, and a subscriber-capture propagation test.

## Out of scope for this module

Sampling policy, downstream `traceparent` injection, log-subscriber/format configuration, and
metrics-backend retention live elsewhere; this proposal only hardens id generation, propagation,
and header trust at the HTTP correlation boundary.
