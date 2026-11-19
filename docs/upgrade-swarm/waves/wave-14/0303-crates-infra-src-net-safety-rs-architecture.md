# Worker #303 — `crates/infra/src/net_safety.rs` — architecture (v0.4.0)

- Module: `crates/infra/src/net_safety.rs` (269 lines, REQ-033 / D-05); lens: architecture / structure;
  mode: READ-ONLY proposal, no code changed, no tests run.
- CONFIRMED by file read this session: the module itself, its four call sites
  (`mcp_http.rs:58`, `messaging_http.rs:95`, `connectors.rs:43,107`, `endpoint_safety.rs:13`),
  `lib.rs:31`, ADR-021, `spec/04-nonfunctional.md §4.5` and `spec/05-known-defects.md` D-05.
  Upgrades/risks are GUESS (design inference). No wire/API/data-format change is proposed by default;
  anything externally visible is flagged as a DEVIATION needing `docs/PARITY.md` + owner approval.

## 1. Current structure (CONFIRMED)

- One public module, no types/state beyond four items: `CheckedOutbound { host, port, resolved }`,
  `NetSafetyError` (5 refusal variants), `is_private_address(IpAddr) -> bool`, and the async
  `check_outbound_url(&str, allow_private: bool) -> Result<CheckedOutbound, NetSafetyError>`.
- Private `METADATA_HOSTS: &[&str]` (4 literals). Rule order in `check_outbound_url`: parse → scheme
  (L110) → metadata host string (L113) → credentials (L116) → host (L119) → port (L123–131) →
  `tokio::net::lookup_host` (L132) → empty set (L137) → resolved metadata (L140) → private ranges
  (L147) → `Ok`. Fail-closed, one function — matches the module's "one place" claim (L4–5).
- All four callers **discard** the returned `CheckedOutbound`, including `resolved` (kept "for the
  audit trail (D-05)", L24). No caller consumes the audit data here; ADR-021 puts auditing elsewhere.
- `allow_private: bool` releases three rules at once: scheme (HTTP), port (non-443) and private
  ranges. Metadata endpoints stay blocked in both modes.

## 2. Architecture findings (CONFIRMED unless marked)

1. **Coarse mode flag.** One boolean conflates three independent relaxations (least-privilege / D-05).
2. **Overlapping, partially dead rule checks.** Under `allow_private=false` L110 already forces HTTPS
   and L129 forces `port==443 && scheme==https`, so L126's `port != 443 && port != 80` branch is
   unreachable; under `allow_private=true` neither L126 nor L129 fires. Two conditions guard the same
   rule — hard to audit for completeness, and a drift hazard (cf. sibling #287 U1).
3. **Two metadata checks over one const with different representations.** L113 compares `host_str()`
   as a string, L140 compares `IpAddr::to_string()`. GUESS: the bracketed IPv6 literal in
   `METADATA_HOSTS` (`fd00:ec2::254`) cannot match L113's `host_str()`, so that entry only works via
   the resolved-IP path — a representation mismatch inside a single policy.
4. **Partial, hard-coded address classification.** `is_private_address` (L61–84) checks v4 octets and
   `Ipv6Addr::segments()[0]`; modern special-purpose ranges (NAT64 `64:ff9b::/96`, site-local
   `fec0::/10`, Teredo, multicast) are unclassified. GUESS/needs decision: these are non-public, but
   the classifier is a maintenance liability with no whole-space property test.
5. **I/O coupled to policy.** The guard resolves DNS inline with no injected resolver or timeout;
   the validated IPs are returned but never pinned by the transport, leaving the documented
   DNS-rebinding window (siblings #280 C5, #287 U4). Tests needing a "private" answer need real DNS.

## 3. Concrete upgrades for v0.4.0 (with risks)

**U1 — Replace the boolean with a typed policy (structure, do first).**
Introduce `OutboundPolicy { allow_http, allowed_ports, allow_private_ranges }` with
`OutboundPolicy::public()` (current `false`) and a narrowly-scoped per-space exception builder; keep
`check_outbound_url(url, bool)` as a thin shim, then migrate the four callers and tests.
Risks: (a) Rust-crate API change only — internal, so not a DEVIATION, but all four callers + 8
`req_033_ssrf_*` tests must move together; (b) the shim must be byte-identical to today's behavior so
no URL flips from accepted to refused; (c) do not let a "narrow" exception accidentally grant HTTP +
all ports + all ranges as the bool does today (that is the D-05 shape we are removing).

**U2 — Linearize the rules into one auditable pipeline (correctness, mostly refactor).**
Express each rule as a pure predicate over a parsed URL + resolved addresses, evaluated in the fixed
order above; delete the dead/overlapping port branch (L126) and unify the two metadata lookups onto
one canonical form (parse the const entries once into `IpAddr`/host, compare typed values).
Risks: (a) error *variant* returned per URL must stay identical — lock all 5 reasons with tests before
editing (siblings #287 U1 and #299 depend on this); (b) GUESS about `host_str()` bracket behavior must
be confirmed by a test, not assumed; (c) metadata hosts must remain un-releasable by any policy.

**U3 — Make the pure policy I/O-free and inject the resolver (testability + rebinding seam).**
Split `validate(&ParsedUrl, &[IpAddr], &OutboundPolicy) -> Result<CheckedOutbound, NetSafetyReason>`
(pure) from a `Guard { resolver: impl Resolve }` that does `lookup_host` under a bounded timeout.
Callers can then unit-test refusals with a scripted resolver (no real DNS), and the `resolved` set
becomes the contract a transport can pin against (coordinate with #280/#287 rather than duplicating
their fix here). Risks: (a) `Resolve` trait + struct widen the API — keep it crate-private where
possible; (b) adding a resolver timeout is a behavior change under slow DNS (GUESS: choose a generous
default that cannot refuse today's accepted calls); (c) true IP pinning needs reqwest custom
resolution and is a separate port change — leave it out of this module's scope.

**U4 — Harden address classification with a property test (security, cheap).**
Add a `proptest` over arbitrary v4/v6 bytes asserting: public addresses are never blocked, private/
loopback/link-local/ULA are always blocked (REQ-033 in the name). Decide explicitly on the
special-purpose ranges in finding 4; if broadened, document it as a refusal-only tightening (GUESS:
non-public addresses are not legitimate targets, so parity risk is low, but confirm goldens).
Risks: (a) widening the blocked set can refuse a previously accepted address — check goldens and
record any change; (b) NAT64/64:ff9b can be a legitimate public destination — needs an owner decision
before blocking; (c) keep `is_private_address`'s current boolean signature as the tested entry point.

**U5 — Make the audit promise honest (observability, GUESS scope).**
Either have the guard own an audit sink so D-05's "emit an event each use" is enforced at the single
choke point, or drop the `resolved`/audit-trail wording and document that auditing is the callers'
job. Risks: (a) ADR-021 says callers already write `mcp.private_call` — adding guard-side events may
**duplicate** them; confirm the existing sink before wiring anything; (b) never log the raw URL or
resolved IPs beyond the existing audit schema (query secrets/PII); (c) no new client-visible behavior.

## 4. Ordering, invariants, non-goals

Order: **U2 → U1 → U3 → U4 → U5** (wire-neutral refactors first; U3/U5 are the only structural
changes). Invariants to preserve: HTTPS-only by default, no credentials, no metadata/private/loopback,
port 443 by default, per-call resolution, metadata never releasable, all 5 refusal reasons and their
strings stable, `req_033_ssrf_*` names in the trace gate. Non-goals: no new allow-list surface, no
change to mcp/messaging/push wire behavior, no new dependency without `cargo add` + docs.rs +
`docs/DEPENDENCIES.md`, no ADR or gate edit without the owner.

## 5. Evidence & tests to add

- CONFIRMED — read this session: `net_safety.rs:1-269`; callers `mcp_http.rs:58-68`,
  `messaging_http.rs:95-99`, `connectors.rs:43,107`, `endpoint_safety.rs:13-20`; `lib.rs:31`;
  ADR-021; `spec/04-nonfunctional.md:88-90`; `spec/05-known-defects.md:20`; sibling wave-13/wave-10
  proposals.
- GUESS — `url::Url::host_str()` bracket behavior for IPv6; whether goldens pin exact refusal strings;
  whether the app layer already emits a per-use audit event (U5); whether a resolver timeout is safe.
- Suggested names: `req_033_the_dead_port_branch_is_gone_and_reasons_are_stable` (U1/U2),
  `req_033_a_narrow_policy_does_not_grant_all_relaxations` (U1),
  `req_033_metadata_is_refused_in_every_policy` (U2),
  `req_033_classification_holds_for_arbitrary_addresses` (U4, proptest).

Line budget: <120. Status: proposal only; READ-ONLY worker, no code changed and no tests run.
