# Brief 0040 — contact-us-in through the observability lens

Proposal only; no product code was changed. **CONFIRMED** = verified by a read here; **GUESS** = a
recommendation for the owner. Upstream was read as data at `/tmp/opencode/openbot-parity` (HEAD `559d5ac`,
2026-11-19). No upstream code, asset, wording, identifier or brand is reproduced; no Pro component is used.
Product strings stay Boma Bot.

## 1. The upstream capability in my own words (data only)

CONFIRMED (upstream `ContactPage.tsx`, `routes/v1/contact.ts`, `server/contact-message.ts`, `lib/analytics.ts`,
`server/email-delivery.ts`, `server/account-analytics.ts`, `ANALYTICS.md`, `PRIVACY.md`): a public `/contact`
page posts name, optional company, email, a fixed-list topic and a capped message to a same-origin endpoint
(`Origin` + `Sec-Fetch-Site` checked). The server validates (single-line name/company ≤ 120 chars, message ≤
2000, email normalized, unknown topic refused), rate-limits with SHA-256-hashed keys (3 per 15 min per IP, 15
per 15 min globally), and hands the message to the team inbox over SMTP (a hosted email service as fallback).

Observability is deliberately content-free: the page reports one event carrying only an allowlisted topic tag
and a `succeeded|failed|rate_limited` result (production builds only); the account service reports a mailbox-quota event (fixed kind tag; `accepted|rate_limited|unknown|failed` result; accepted+unknown count against a
500/hour quota). Message, name and email are never sent as analytics. A 500 returns an `auth/<16 hex>` reference
and logs only the error's type tag. Inside the product, "Contact us" / "Contact sales" is a click-through
tracked as one event tagged with the surface (add-server, license or members); the product keeps no record of
the handoff.

Worth naming: every telemetry field is a closed enum (nothing free-text), and the user-quotable reference is the
only correlation that survives. The gap: client event, HTTP request and SMTP send share no id, and the account
service's own logs are `console.*` with random references — no trace correlation, no counter for the contact
path itself.

## 2. Why observability is the deciding lens

CONFIRMED: Boma Bot has no contact surface (`spec/08-ui-ux.md` §8.1 — anything not listed is not a screen; no
contact route in the tree). The wave-3 lanes treat it as a new, owner-approved surface anchored to REQ-058. If
it ships, each stage — submit, validate, rate-limit, deliver, answer — must be reconstructible from our own
structured logs, request ids and counters, or we have rebuilt the upstream gap with better fonts. REQ-054
already ships the substrate; this lane attaches the contact path to it rather than inventing a parallel
telemetry pipe. The audit row for the fact is brief 0028's lane (REQ-058); this proposal owns only logs,
counters and correlation.

## 3. Grounding in our own contract

- **REQ-054 (anchor).** CONFIRMED — `spec/01-requirements.md:441` (MUST): structured logs carry request and
  run correlation; run timing uses named fields; OpenTelemetry export and CSP violation reporting are available.
  `docs/TRACEABILITY.md` REQ-054 row counts only declaration-name tests (`tools/trace_check.py` skips `docs/`;
  prose earns nothing).
- **Correlation.** CONFIRMED — `crates/api/src/correlation.rs`: one JSON log per event; `request_id` /
  `trace_id` / `span_id` (strict `traceparent`, malformed = absent), echoed `x-request-id`; the log records the
  **route template, never the raw path** (tokens never reach a line); `log_http` emits the `boma.http` line and
  `boma_http_requests_total{route,status_class}`.
- **Named-field events & counters.** CONFIRMED — `crates/app/src/worker.rs` (`announce_run_started` /
  `announce_run_finished` under target `boma.runs`) and `crates/app/src/metrics.rs` (hand-rolled, counters only,
  sorted/escaped labels, poisoned lock drops the increment, `/metrics` behind `BOMA_METRICS`, 404 otherwise).
  Model the contact events on exactly this shape.
- **A public, unauthenticated precedent.** CONFIRMED — `crates/api/src/public.rs::csp_report` records host-
  only violations through a typed audit row plus `boma_csp_violations_total{directive}`, D-01 rate-limited,
  body-capped; a public contact endpoint copies this shape, not upstream's analytics.
- **Redaction.** CONFIRMED — `crates/domain/src/audit.rs` (`redact_audit_value`, `SECRET_TOKENS`) is a
  backstop, not permission to log inputs; D-02's fate (a dev mail reader leaking tokens) is why.

## 4. Proposed shape — a contact path that narrates itself, content-free

GUESS (design; owner approves before code):

- One new target `boma.support`. One event per terminal outcome of the submit path:
  `support.contact_submitted` with bounded fields `surface` (`web|in_app`), `topic` (a closed enum, unknown
  values dropped, never free text), `result` (`succeeded|failed|rate_limited|invalid`) and the correlation
  triple the request already carries. One delivery-leg event `support.contact_delivered` with `channel`
  (`smtp|fallback`) and `result` (`accepted|rate_limited|unknown|failed`).
- The reference we show a user on failure **is the `request_id`** (same echo, same shape). "Email us quoting
  ref …" then resolves to the single `boma.http` line and the single `boma.support` line — upstream's
  `auth/<hex>` idea, upgraded to our correlation contract, zero PII required.
- If the in-app entry ships: `support.contact_opened` with `entry_point` (`add_server|license|members`, our
  enums, not any upstream wire values), mirroring upstream's in-product click point.
- Counters: `boma_support_contacts_total{result}` and `boma_support_contact_deliveries_total{channel,result}`.
  Labels are closed sets — no ids, names, emails, messages, URLs or free text, so a scrape cannot leak.
- **No secret leakage, by construction:** only enums plus the correlation ids our run logs already carry;
  route templates never raw paths; rate-limit keys SHA-256 as the CSP path does; topic allowlist enforced at the
  boundary. The audit row (or lack of one) stays 0028's decision. OTLP stays deferred (DEC-18); nothing here
  depends on it.

## 5. One REQ-named test

Rust unit test in `crates/app/src/usecases.rs` (capturing-tracing + testkit pattern):

```
fn req_054_a_contact_submission_logs_one_correlated_line_and_counts_without_content() { … }
```

Assertions: a submission ending `rate_limited` emits exactly one `support.contact_submitted` line whose fields
include `request_id`/`trace_id`/`span_id` and a bounded `result`;
`boma_support_contacts_total{result="rate_limited"}` advances by one; the rendered JSON log and the `/metrics`
body contain neither the sender's address nor the message text nor any `SECRET_TOKENS`-shaped value; an off-list
`topic` is dropped, never logged. The `REQ-054` name makes `tools/trace_check.py` count it; the `REQ-054` row in
`docs/TRACEABILITY.md` is extended only after this test exists.

## 6. First slice (test-first, smallest complete loop)

1. Add the emit helper (modelled on `announce_run_started`) and the two counters in `metrics.rs` style; no
   transport, UI or RPC surface ships in this slice.
2. The single `REQ-054` test above; confirm it fails first, then green.
3. Note the proposal and the upstream files consulted in `docs/STATE.md`; no new REQ id (the property
   attaches to REQ-054). The support surface itself still needs the owner's STATE/PARITY/ADR approvals tracked
   by the wave-3 lanes.
4. Gates unchanged: `cargo fmt --check`, `clippy -D warnings`, tests, `tools/trace_check.py`, Playwright, axe
   (zero critical/serious, light/dark, 360 px).

**Out of scope:** the contact surface/RPC itself (lanes 0021–0030), the audit row (0028, REQ-058), outbound mail
content, any upstream analytics export, OTLP forwarding, free-text log fields, any Pro component.

## 7. Evidence

CONFIRMED — upstream HEAD `559d5ac`: `ContactPage.tsx` (client validation, same-origin POST, allowlisted
submission event, production-only), `contact-message.ts` (limits, topic label mapping, SHA-256 rate-limit keys,
429), `lib/analytics.ts` (runtime-validated allowlist), `email-delivery.ts` + `account-analytics.ts` (mailbox-quota event, fixed kind/result tags, no address/name/content), `authErrorResponse` (`auth/<hex>` reference),
`isSameOriginReportRequest`, `source-ip.ts`, `ANALYTICS.md`/`PRIVACY.md`. Ours: `spec/01-requirements.md`
REQ-054; `correlation.rs`; `public.rs` (`csp_report`); `metrics.rs`, `worker.rs`, `usecases.rs`
(`record_csp_violations`); `domain/src/audit.rs`; `tools/trace_check.py`; `docs/adr/024-operate.md` (REQ-054
§10–12, DEC-18); `docs/TRACEABILITY.md` REQ-054 row; wave-3 siblings `0021–0030`. GUESS — event/counter names,
`reference = request_id`, the `surface`/`entry_point` enum sets.
