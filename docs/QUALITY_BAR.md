# Quality bar ("better than the original")
Parity

100% of MUST requirements implemented and tested; at least 90% of SHOULD; every COULD either done or explicitly deferred in docs/STATE.md.
Every REQ id named by at least one test declaration (tools/trace_check.py, comments do not count, no ORPHAN id left unexplained).
All golden examples of spec/golden/ pass; none is silently skipped.
All UI golden examples of spec/golden/ui/ pass (seeded states, roles and states).
The generated OpenAPI document matches spec/openapi.yaml, or every difference is a recorded and approved DEVIATION.
If the original is available, differential API tests compare both implementations on the same inputs, and docs/UI-PARITY.md covers every screen and state; intentional differences are listed, unintentional ones are fixed.
Defects

Every item of spec/05-known-defects.md fixed, or explicitly deferred with a reason.
Gates are honest

A gate (CI job, lint, deny.toml, coverage threshold, golden runner, contract test, trace_check, axe threshold, UI golden replay, production-host guard) may be improved or replaced, but never silently: an ADR or a STATE.md entry explains the change, and the replacement is at least as strict.
No ignored or skipped test without an entry in docs/STATE.md.
Rust code quality

cargo fmt --check clean; cargo clippy --all-targets --all-features -- -D warnings clean.
unsafe_code forbidden (any exception has an ADR); zero unwrap, expect, panic, todo or unreachable-by-accident in non-test code.
Typed errors, newtypes for ids and money, no floating point for money.
cargo deny check clean (advisories, licenses, bans, sources); cargo machete reports no unused dependency; Cargo.lock committed; toolchain pinned.
Coverage (cargo llvm-cov) at least 80% of lines overall and 90% on the domain crate; property tests on critical invariants; fuzz or property tests on every external-format parser.
No dead code, no TODO without an entry in docs/STATE.md.
Security

All external input validated at the boundary; outputs encoded; parameterized queries only.
Authentication and authorization on every non-public endpoint; deny by default.
No secrets in code or git history; secrets from environment, wrapped and never logged. No BoardUI license key in any file.
Secure defaults: secure cookies, CORS allow-list, rate limiting, request body limits, security headers, safe error messages (no internals to clients).
Passwords hashed with Argon2id if applicable; rustls rather than OpenSSL.
Reliability and performance

Performance budget defined in /plan and measured in /audit with the same commands and machine; no regression between measurements; benchmarks for hot paths.
Timeouts on every outbound call, retries with backoff, idempotency wherever the spec involves external calls or payments.
No blocking in async code, no lock across .await, bounded queues.
Graceful shutdown; /healthz and /readyz; restart-safe.
Developer experience

One command each to install, run, test, lint, build (Makefile or justfile).
CI running fmt, clippy, tests, cargo deny, coverage, machete, secret scan and trace_check on every push, plus the UI suite and axe when there is a UI.
README with quickstart, architecture overview (mermaid), configuration table and troubleshooting. ADRs for major decisions. rustdoc on public library items.
Observability

Structured JSON logs with request ids, metrics endpoint, clear error codes, no secrets or personal data in logs.
UX and accessibility (only if the spec has a UI)

Parity of behavior: every screen, state and interaction of spec/08-ui-ux.md exists; no feature invented beyond the spec.
Design direction: either faithful visual parity (licensed assets, labelled references) or an original design; the choice is an ADR. References live in ../re-workspace/notes/ui/ and docs/ui-parity/.
WCAG 2.2 AA: zero critical or serious axe finding; keyboard-only path for every primary flow; visible focus; logical focus order; no information by color alone; 4.5:1 text contrast; 44 px targets where the spec has touch.
States: loading, empty, partial, error, permission-denied and offline are designed, implemented and tested, not left to chance.
Responsive: usable at 360 px and at the plan's widest breakpoint; no horizontal scroll at 360 px.
Light and dark, prefers-reduced-motion, forced-colors at least not broken.
i18n/RTL: no hardcoded user-visible string; dir=rtl tested when the spec requires it.
Performance: LCP, INP and CLS within the plan's budget on a throttled profile, JS bundle within budget, no layout shift caused by late data.
Design system discipline: BoardUI tokens and rules, one theme, no ad-hoc colors or spacing, local component edits documented in docs/UI.md.
Delivery

Semantic versioning, CHANGELOG, migration guide from the original's data and export formats (spec/02-data-model.md), reproducible build (--locked), checksums, LICENSE chosen by me.
/ui-parity run against the original when it is available, and a license check of every asset, font and vendored component before publishing.
