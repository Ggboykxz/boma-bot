# v0.4.0 architecture proposal — `crates/api/src/correlation.rs` (request/trace correlation, REQ-054)

Worker #133 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no code changed).

Status of claims: all "current state" statements are CONFIRMED by direct reads of `correlation.rs`,
`router.rs`, `automation.rs`, `Cargo.toml` (workspace + api crate), `spec/01-requirements.md` REQ-054
and `spec/05-known-defects.md` in this worktree. Forward-looking items are proposals (GUESS until
implemented and tested).

## Current structure (CONFIRMED)

- `Correlation` is three plain `String`s — `request_id`, `trace_id`, `span_id` (`correlation.rs:25-34`).
  Nothing prevents an invalid value from being constructed; `parse_traceparent` returns an untyped
  `(String, String)` tuple and `mint_trace` returns another (`65-70`).
- The caller's `x-request-id` is accepted verbatim if it is non-empty, <=64 bytes and ASCII-graphic
  (`44-48`, `72-74`), then echoed in the response (`205-209`). Any client can pick a colliding or
  arbitrary reflected id; this is a header/log-forging surface, not a trustworthy identifier.
- `parse_traceparent` (`76-103`) refuses the whole header on anything unexpected: version != `"00"`,
  wrong field count, non-hex, zero ids. W3C requires *tolerating* higher versions, so a valid future
  header is treated as absent. `flags` is length-checked but its bits are never interpreted, and
  `tracestate` is ignored. Refusing outright is a deliberate, documented policy ("a wrongly-parsed
  header makes worse triage than a fresh id") and should be kept for genuinely malformed input.
- `mint_trace` masks a UUID with `0x0000_0000_0000_ffff_ffff_ffff_ffff_ffff` (80 low bits) then
  formats only 16 hex digits (`65-69`): the mask is misleading and a 1-in-2^64 all-zero span id is
  never re-validated.
- The three ids are repeated by hand in both the span and the completion event (`177-179`, `200-202`):
  any future log statement can silently omit them. There is no extension/extractor for downstream
  code and no propagation into outbound calls (webhooks, external agents) — REQ-054's "request and
  run correlation" is only satisfied inside this one event.
- The route label comes from a hand-maintained prefix table `template()` (`105-158`) that duplicates
  the router's dynamic shapes. New dynamic routes fall through to the raw path, so a future token in
  a path could reach a log line — the same class of leak as D-02. Meanwhile the workspace already
  enables axum's `matched-path` feature (`Cargo.toml:29`) and nothing uses it (`grep`: no `MatchedPath`).

## Guiding principle for v0.4.0

Make the correlation value typed and valid-by-construction, derive the route label from routing rather
than a parallel table, and stamp the ids once so every log and every outbound call inherits them.
Keep the "mint fresh on doubt" policy; never widen it at the cost of trusting attacker input.

## Upgrades (5 concrete items)

1. **Typed, sealed ids.** Replace the three `String`s with `RequestId`, `TraceId`, `SpanId` newtypes
   (validated constructors, `Display`/`AsRef<str>`), and make `parse_traceparent` return a typed
   `TraceContext { trace_id, span_id, flags }` using `thiserror` for parse errors. Invalid states become
   unrepresentable; `mint_trace` builds valid newtypes directly.
   *Risk:* touches every `correlation.*_id` access in `log_http` and tests and can change Debug/serde
   shape used by golden fixtures. *Mitigation:* keep `Display` byte-identical, add accessors mirroring
   old field paths, run golden + contract suites.

2. **Spec-complete parser with property/fuzz tests.** Tolerate versions above `00` with W3C's
   version-dependent field-count rule, interpret `flags` minimally, canonicalise case, and keep
   wholesale refusal for genuinely malformed/zero headers. Add `proptest` + fuzz coverage for every
   accepted family (AGENTS.md: property/fuzz for every external-format parser), preserving REQ-054
   test names (`tools/trace_check.py` counts declarations).
   *Risk:* tolerating higher versions changes which ids are emitted (wire-visible). *Mitigation:*
   still mint fresh on any unrecognised shape; record any externally visible change in
   `docs/PARITY.md` before shipping.

3. **Derive the route template from `MatchedPath`.** Extract axum's `MatchedPath` in `log_http` as the
   primary label and keep `template()` only as a fallback for unmatched/static paths. This removes the
   duplicated router table and closes the token-leak regression gap for future routes.
   *Risk:* the correlation layer is added last in `router.rs:152-155` (outermost) and may not see the
   extension set by routing; nested routers under `/api/v1/automation` can also hide it. *Mitigation:*
   verify at runtime, move the layer inside the nested routers if needed, and add a differential test
   over the full route set asserting no raw dynamic segment ever reaches a log.

4. **Stamp once, propagate outward.** Stop repeating three `%` fields in two places; rely on the
   `info_span!` plus a JSON subscriber layer/filter that injects ids into every event, and expose
   `Correlation` via `Extension` so handlers and outbound HTTP clients can attach `traceparent` /
   `x-request-id` (REQ-054 request *and* run correlation).
   *Risk:* subscriber-wide changes affect all targets and could leak ids into unrelated logs;
   outbound header propagation is externally visible. *Mitigation:* scope to the `boma.http` target,
   test that only ids (never tokens or secrets) are propagated, and log any wire change as a DEVIATION.

5. **Harden the caller-supplied request id.** Adopt a strict grammar (UUID or `^[A-Za-z0-9._:-]{1,64}$`);
   on a conforming value keep the echo, otherwise mint a fresh id and retain the supplied value in a
   separate, clearly labelled `client_request_id` field for triage.
   *Risk:* rejecting previously accepted ids changes the echoed header; clients may depend on exact
   echo. *Mitigation:* conforming ids (the common case) stay unchanged, document the change in
   `docs/PARITY.md`, and add property tests covering control chars, unicode and overlong input.
   Also fix the misleading 80-bit mask in `mint_trace` and re-validate the minted span id.

## Cross-cutting risks / sequencing

- Sequence 1 → 2/3 → 4 → 5: typed ids are the prerequisite for a typed parser and for safe
  propagation; `MatchedPath` is independent and can land first as a small, high-value commit.
- Never replace the "mint on doubt" rule with permissive parsing: a wrongly trusted id is worse than
  a fresh one (module contract, lines 3-7).
- Keep invite/share/bot/signing tokens out of every log (REQ-042, D-02); templating must be proven,
  not assumed.
- D-04 (trusted proxies / forwarded headers) is a REQ-054-adjacent defect but lives outside this
  file; do not absorb it here.
- Verification gate per step: `cargo fmt --check`, `clippy -D warnings`, unit + `tests/` integration,
  golden and OpenAPI contract suites.

## Out of scope for this module

Full OpenTelemetry exporter wiring, CSP violation reporting, and the named run-timing events
(`run.execution_started`, `run.first_response`, `run.finished`) live elsewhere; this proposal only
restructures the request/trace id value, its parsing, and the HTTP correlation boundary.
