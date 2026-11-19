# Brief 0020 — always-allow-on through the observability lens

Proposal only; no product code changed. Upstream material was read as data — no upstream code,
asset, wording, identifier or brand is reproduced, and no Pro component is used.
**CONFIRMED** = verified by a read here; **GUESS** = a recommendation for the owner.

## 1. The upstream capability in my own words

CONFIRMED (data only: upstream `CHANGELOG.md`, `docs/architecture/{agents,computer-use,structure}.md`,
`ANALYTICS.md`): an agent normally pauses before it changes files, runs a command, calls a tool or
publishes, and a person answers a pending card. At the most permissive setting those effects proceed
without a card. The interesting middle is a one-shot answer on the card — while it waits, a person can
turn that single "yes" into a standing rule. Upstream keeps that rule client-side, keyed to a tool and
an account, so it survives a restart but is not pushed to the server; a configuration change downgrades
it to a warning instead of silently persisting, and sign-in, forms and screen-takeover prompts are never
absorbed.

Observability of that decision is split: upstream records when a host asks for human input and when the
reply IPC finishes, so operators can see both the request for a decision and its resolution. Its privacy
rules are strict — no message content, commands, credentials, tokens, invitations, raw errors, local
identifiers or user-authored names, only allowlisted numeric/string fields. GUESS: the intent is to
measure approval friction without turning the telemetry stream into a second copy of private data.

## 2. Why observability is the deciding lens here

CONFIRMED: our approval gate is real but per-decision (REQ-007 card, REQ-017 exactly-once answer), and
there is no standing grant today — `docs/STATE.md` schedules M16 scoped auto-approvals, and a new REQ
id, ADR and PARITY DEVIATION need owner approval before transport changes ship. A grant is a permanent
widening of what an agent may do unattended. If it is granted, revoked or exercised without a trace,
the only record that authority changed lives in a client preference. The observability lens refuses a
silent bypass: every widening, every exercise and every downgrade must be reconstructible from our own
structured logs, request correlation and counters, without copying upstream's event names or vendor
export.

## 3. Grounding in our own contract

- **REQ-054 (anchor).** CONFIRMED — `spec/01-requirements.md:441`: structured logs carry request and run
  correlation, run timing uses named fields, CSP reporting is available (MUST). It is already the
  requirement behind our HTTP and run telemetry, so a grant's observability attaches to it rather than
  inventing a new surface.
- **Correlation & no-secret logging.** CONFIRMED — `crates/api/src/correlation.rs`: one JSON log per event
  under `BOMA_LOG_FORMAT=json`, each request carries `request_id`, `trace_id`, `span_id`, a validated W3C
  `traceparent` and an echoed `x-request-id`; the log records the **route template, never the raw path**,
  so invite tokens, share tokens and webhook/installation ids never reach a line. `boma_http_requests_total`
  is incremented with `{route,status_class}` labels. `crates/bin/src/telemetry.rs` makes an invalid filter
  an error, never a silent fallback.
- **Named-field events.** CONFIRMED — `crates/app/src/worker.rs::announce_run_started/announce_run_finished`
  and `crates/app/src/usecases.rs` emit `event="run.execution_started"|"run.finished"` under target
  `boma.runs` with `run_id`, `space_id`, `bot_id`, `trigger`, `status`, `duration_ms`, plus the
  `boma_runs_started_total{trigger}` counter. This is the exact shape a grant event should mirror.
- **Counters.** CONFIRMED — `crates/app/src/metrics.rs`: counters only (no gauges/histograms), labels
  sorted and escaped, exposition gated behind a flag, and a poisoned lock drops an increment rather than
  failing a request.
- **Redaction & typed audit.** CONFIRMED — `crates/domain/src/audit.rs`: `AuditKind` has stable `as_str()`
  wire values, `redact_audit_value` blanks secret-shaped values and `SECRET_TOKENS`/`REDACTED` are the
  backstop; REQ-045 `audit.export` is redacted JSONL. Adding a grant audit kind belongs to the separate
  audit lane (brief 0008); this slice only needs the log/counter emission to be secret-safe.
- **Gate discipline.** CONFIRMED — `tools/trace_check.py` counts a REQ id only when it appears in a test
  *declaration* name and skips `docs/`; prose earns nothing. Tests must carry `REQ-054` in their names.

## 4. Proposed shape — an observable grant

GUESS (design, to be approved before code):

- At the point that would apply an always-allow grant, emit one named-field event under a new target
  `boma.approvals`: `event="approval.grant_exercised"` with bounded fields `effect_class` (from a domain
  effect enum, never a free-text tool string), `decision`, `scope`, plus the same `request_id`,
  `trace_id`, `span_id`, `run_id`, `space_id`, `bot_id` fields the run/HTTP logs already use. Emit one
  event each for `approval.grant_created`, `approval.grant_revoked` and `approval.grant_stale` — a
  downgrade is a warning-level event and a counter tick, never a silent apply.
- Add counters in `metrics.rs` style: `boma_approval_grants_total{decision}` and
  `boma_approval_bypasses_total{effect_class}`. Labels are closed sets; no ids, names, arguments,
  prompts or free-form text, so a scrape cannot become a leak.
- A helper mirroring `announce_run_started` centralises the event + counter so every call site emits the
  same fields. `x-request-id` from the correlation context ties a grant decision back to the HTTP
  request that caused it, matching REQ-054's request-and-run correlation.
- **No secret leakage, by construction:** log only bounded enum values and the ids our run logs already
  carry; never tool arguments, token values, invite/webhook ids, raw paths, credentials or raw errors.
  The route template and `redact_audit_value` remain backstops, not permission to log inputs.

## 5. One REQ-named test

New Rust test in `crates/app/src/usecases.rs` (capturing-tracing + testkit pattern):

```
fn req_054_a_standing_grant_emits_one_correlated_event_and_one_counter_without_arguments() { … }
```

Assertions: exercising one grant emits exactly one `approval.grant_exercised` event whose fields include
`request_id`/`trace_id`/`span_id` and a bounded `effect_class`, and advances
`boma_approval_bypasses_total` by one; the rendered JSON log and `/metrics` body contain neither the
tool's argument string nor any `SECRET_TOKENS`-shaped value; a stale grant emits a warning event and a
`boma_approval_grants_total{decision="stale"}` tick. The `REQ-054` name makes `tools/trace_check.py` count it.

## 6. First slice (test-first, smallest complete loop)

1. Add the emit helper + the four named events and two counters at the grant decision point, modelled on
   `announce_run_started`; no transport or UI change.
2. Add the single `REQ-054` test above; confirm it fails first, then make it green.
3. Note the proposal and upstream docs consulted in `docs/STATE.md`; no new REQ id is needed because the
   observability property attaches to REQ-054 (the grant feature itself still needs the M16 REQ/ADR/PARITY
   approvals before any transport ships).
4. Gates unchanged: `cargo fmt --check`, `clippy -D warnings`, tests, `tools/trace_check.py`, Playwright,
   axe (zero critical/serious, light/dark).

**Out of scope:** the grant/consent storage and audit kind (other lanes), any upstream analytics event or
vendor export, client-side telemetry, free-text log fields, and any Pro component.

## 7. Evidence

CONFIRMED — `spec/01-requirements.md` REQ-054; `crates/api/src/correlation.rs` (`Correlation`, `template`,
`log_http`); `crates/bin/src/telemetry.rs`; `crates/app/src/metrics.rs`; `crates/app/src/worker.rs`
(`announce_run_started/finished`); `crates/domain/src/audit.rs` (`AuditKind`, `redact_audit_value`,
`SECRET_TOKENS`); `tools/trace_check.py`; `docs/STATE.md` (M16, owner rule). Upstream data only —
`CHANGELOG.md`, `docs/architecture/{agents,computer-use,structure}.md`, `ANALYTICS.md`. GUESS — event and
counter names, field set, and the warning on staleness.
