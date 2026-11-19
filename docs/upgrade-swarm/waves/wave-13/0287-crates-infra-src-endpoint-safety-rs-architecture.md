# Worker #287 — `crates/infra/src/endpoint_safety.rs` — Architecture (v0.4.0)

Module: `crates/infra/src/endpoint_safety.rs` (22 lines, read this session on `before-m5`).
Lens: architecture / structure. Spec anchor: REQ-064 (Web Push, MUST),
`spec/01-requirements.md`:512-522; local port `ports.rs`:4542-4557; caller `rpc.rs`:4222-4255.
Status: current shape CONFIRMED by reading the files below. Upgrades and risks are
PROPOSAL/GUESS (design inference, not implemented). English per repo disk convention.

## 1. Current shape (CONFIRMED)

- `InfraEndpointSafety` is a zero-sized `#[derive(Default, Clone, Copy)]` adapter; its only job is
  to implement `ports::EndpointSafetyChecker` (`check(&str) -> Result<(), AppError>`).
- `check()` delegates wholesale to `crate::net_safety::check_outbound_url(url, false)`, maps
  `Ok(_) -> ()` (discarding `CheckedOutbound` and its `resolved` IPs), and flattens **every**
  `NetSafetyError` variant into one `AppError::Rule(DomainError::InvalidName("the endpoint address
  is refused"))`.
- `net_safety.rs`:104-155 holds the real policy (HTTPS, no credentials, port 443, no metadata /
  private / loopback / link-local, per-call DNS resolution). `allow_private = false` is hardcoded in
  the one-line adapter call, so this path can never relax the rules.
- Wiring: `bin/src/commands.rs`:265 installs `Some(Arc::new(InfraEndpointSafety))` into
  `services.endpoint_safety`, but that field is `Option<...>` (`service.rs`:294) and the RPC calls it
  only under `if let Some(checker)` (`rpc.rs`:4245) — an absent adapter is fail-open.
- The API layer re-parses and pre-checks scheme + credentials itself (`rpc.rs`:4231-4244) before the
  adapter runs, then maps any refusal to a fixed client string (`rpc.rs`:4249-4254).
- Test double: `ScriptedEndpointSafety` (`testkit/src/build.rs`:10191-10215) accepts a URL unless it
  contains a refused fragment. The adapter itself has **no unit tests**; SSRF rules are tested only
  in `net_safety.rs`.

## 2. Architecture issues (CONFIRMED by inspection)

1. **Split-brain validation.** Scheme and credential rules live in both `rpc.rs` and `net_safety.rs`;
   port/private-range rules only in `net_safety`. Two sources of truth drift. `check_outbound_url`'s
   port 126 branch (`port != 443 && port != 80`) is dead under `allow_private=false` because
   line 129 already forces `port == 443 && scheme == https` — confusing but currently fail-closed.
2. **Optional port = fail-open.** `None` silently disables the write-time SSRF boundary; nothing
   makes the checker mandatory for deployment configurations that omit the bin wiring.
3. **Reason, audit trail and observability lost.** The typed `NetSafetyError` (5 reasons) and the
   `CheckedOutbound.resolved` list (kept "for the audit trail (D-05)", `net_safety.rs`:24) are both
   discarded at this boundary. No `tracing` event, no metric: SSRF refusals are invisible.
4. **Semantically wrong error.** `InvalidName` ("naming") is reused for "endpoint refused"; the
   trait doc promises "an error naming the rule that refused the URL" (`ports.rs`:4548) but the
   adapter names none. RPC collapses it again, so callers cannot distinguish reasons.
5. **Check-time / use-time gap.** The check resolves DNS at subscribe time only; Web Push delivery
   in `push.rs` resolves again later (detached tasks, wave-10 note R6). DNS rebinding between the
   two is not prevented. This adapter is a write-time guard, not a delivery-time guard — the module
   doc ("behind `push`'s SSRF boundary") overstates it.

## 3. Concrete upgrades for v0.4.0 (with risks)

**U1 — One source of truth for the webhook SSRF policy (do first, mostly refactor).**
Expose a pure syntax validator from `net_safety.rs` (scheme / credentials / port / metadata host)
and have both `rpc.rs` and `check_outbound_url` call it; delete the duplicated checks in `rpc.rs`
and the dead port-126 branch. Pass the policy as a typed value instead of a buried `false` literal.
Risks: (a) `rpc.rs` currently returns distinct client strings ("not a URL", "must be an https URL",
"carries no credentials") — preserve them byte-for-byte or record a DEVIATION; (b) prove no
URL previously accepted becomes refused (under `allow_private=false`, port 80 is already refused);
(c) keep metadata hosts refused even with any allow-list.

**U5 — Adapter-level tests and an honest contract (cheap, testable).**
Add unit tests with REQ-064 in the names asserting that every refusal reason maps to an error and
never to `Ok`, and document on the struct that this is write-time only. Inject the inner check via a
`pub(crate)` seam so tests need no real DNS. Risks: (a) seam slightly widens the module API — keep
it crate-private; (b) do not add real network calls to unit tests; (c) `trace_check.py` counts
declaration lines, so put REQ-064 in the `fn` names.

**U2 — Fail-closed wiring.** Make `endpoint_safety` a required port with a default-deny checker, or
have the RPC treat `None` as "refuse subscription", so an omitted adapter cannot turn off the
boundary. Risks: (a) `service.rs`:370 and `testkit/src/build.rs`:349 currently default to `None` —
they must supply a safe default; (b) test doubles must be updated; (c) touches service struct and
bin wiring; (d) no wire change if the client-visible refusal string stays fixed.

**U3 — Preserve the reason, the audit trail and one security event.** Introduce a dedicated
`AppError`/`DomainError` variant (e.g. `EndpointRefused(NetSafetyReason)`) instead of `InvalidName`;
keep the `Resolved` set in one `tracing::warn!` + counter metric (reason label), never logging the
endpoint URL. Risks: (a) AppError shape change — RPC flattens it, so no client wire change (GUESS:
check no other caller matches the variant); (b) metric name/labels must follow `metrics.rs` (GUESS);
(c) never log URLs, webhook URLs or key material.

**U4 — Close the check/use gap (largest, phase last).** Either add a `resolve` port returning
`CheckedOutbound` and have the Web Push transport connect to the pinned IP (preserving SNI/Host),
or re-run the same check at delivery time and refuse if resolution changed. At minimum document the
residual rebinding risk. Risks: (a) pinning is a port change plus reqwest custom resolution;
(b) re-checking adds a DNS lookup per delivery and may transiently fail legitimate pushes — delivery
must stay silent and never fail a run (REQ-064); (c) keep the 5 s timeout and no-redirect rule;
(d) no new externally visible behavior.

## 4. Ordering, invariants, non-goals

Order: **U1 → U5 → U2 → U3 → U4** (cheap, testable, wire-neutral first; U4 is the only structural
port change). Invariants to preserve: HTTPS-only, no credentials, no metadata / private / loopback,
port 443, write-time check still at subscribe, and client-visible error strings unchanged.
Non-goals: no private allow-list for push, no delivery wire/API change, no feature outside the spec,
no ADR or gate edit without the owner.

## 5. Evidence

CONFIRMED — read this session: `endpoint_safety.rs`:1-22; `net_safety.rs`:1-269; `ports.rs`:4542-4557;
`rpc.rs`:4222-4255; `service.rs`:294,370,497; `bin/src/commands.rs`:265; `testkit/src/build.rs`:349,
10191-10215; `lib.rs`:25,48; sibling wave-13 and wave-10 proposals; spec REQ-064.
GUESS — whether goldens assert the exact refusal strings; the `metrics.rs` naming convention; whether
another caller matches the `InvalidName` variant; whether IP pinning can move cleanly behind a
string/bytes-only transport port.
