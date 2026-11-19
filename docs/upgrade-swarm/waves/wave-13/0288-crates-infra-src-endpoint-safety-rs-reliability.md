# Worker #288 — `crates/infra/src/endpoint_safety.rs` — Reliability (v0.4.0)

Scope: runtime reliability of the REQ-064 outbound-endpoint guard. Evidence is static
reading of `endpoint_safety.rs`, `net_safety.rs`, `app/src/ports.rs`, `api/src/rpc.rs`
and `app/src/push.rs` on `before-m5`; no runtime test was run by this worker. Current
code is CONFIRMED; proposals and risks are GUESS unless tied to a cited line. English
per repo disk convention. Structural refactors are covered by worker #287 (wave-13);
this proposal stays on failure modes and how the check behaves under stress.

## Failure mode 1 — DNS rebinding / decaying validation (CONFIRMED shape)

`check()` (`endpoint_safety.rs:12-21`) resolves the host at *subscribe* time via
`net_safety.rs:132-136` and returns `Ok(())` after `.map(|_| ())`, discarding
`CheckedOutbound.resolved` (kept "for the audit trail", `net_safety.rs:24`). Delivery
happens later in `push.rs:438-448`, where `reqwest` re-resolves the same hostname on
every send. Nothing pins the connection to the validated IPs and nothing re-checks.

Reliability consequence: the decision has an implicit, unbounded TTL. A DNS rotation,
TTL expiry, CDN failover or attacker-controlled answer can change the target between
check and use, so protection silently decays for the whole lifetime of a subscription
with no event, no metric and no expiry signal. This is both a security TOCTOU and a
correctness bug: a "validated" subscription is not actually validated at send time.
The module doc ("behind `push`'s SSRF boundary") overstates what one write-time call
guarantees.

Hardening:
- Return/persist the validated IP set with a short TTL and re-run `check_outbound_url`
  before delivery when it expires (fail-closed, but keep delivery best-effort per REQ-064).
- Or connect to a pinned address from `resolved` while preserving `Host`/SNI, so the
  OS cannot re-resolve to a different target.
- Record a metric (refusals, re-check failures, TTL expiries) keyed by reason only —
  never log the endpoint URL or push keys.

Risks: re-checking adds a DNS lookup per send and a transient resolver failure would
drop a legitimate push (must stay silent, not fail the run); pinning is a transport
port change (CONFIRMED need to check docs.rs for the reqwest resolve API — GUESS on
API shape); a TTL cache still leaves a stale window. None changes the wire if refusals
stay silent and the client error string is unchanged.

## Failure mode 2 — Private-IP bypass and resolver failure (CONFIRMED coverage gaps)

`is_private_address` (`net_safety.rs:61-84`) covers RFC1918, loopback, 169.254/16,
100.64/10, 0/8, IPv6 ULA/link-local/unspecified and IPv4-mapped (`to_ipv4_mapped` =
::ffff/96 only). It does **not** cover other IANA special-use ranges: IPv4
192.0.0.0/24, 192.0.2.0/24, 198.18/15, 224/4, 240/4, 255.255.255.255; IPv6
2001:db8::/32, 64:ff9b::/96 (NAT64 embeds an IPv4), 2002::/16 (6to4 embeds an IPv4)
and 2001::/32 (Teredo). An embedded private v4 in a tunnel prefix can pass the
segment test. Metadata hosts are matched by exact string (`87-92`), so a trailing-dot
FQDN or bracketed IPv6 literal does not match that list; the resolved-range check
usually still catches those, so this is defence-in-depth, not the primary gap.

Two reliability faults compound it: `lookup_host` has **no timeout and no retry**
(`132-136`), so a stalled resolver holds the async call open; and any resolution or
private-address failure collapses into one `NotResolvableOrPrivate` (`40`, `134-149`),
which `endpoint_safety.rs:16-20` flattens again into `InvalidName("…refused")`.
A transient DNS outage is therefore indistinguishable from an attack: spurious
subscribe rejections, zero diagnosability, no audit event despite the D-05 comment.
The strict "ANY address private ⇒ refuse" rule (`147`) also means one stray AAAA from
a broken dual-stack record permanently blocks a legitimate public endpoint.

Hardening:
- Block the full IANA special-use set and decode v4-embedded IPv6 (6to4/Teredo/NAT64)
  before the segment test; reuse the `url` crate's canonical host, which already
  normalises decimal/octal IPv4 literals.
- Split `Unresolvable` (retryable, bounded backoff) from `PrivateTarget` (permanent);
  wrap `lookup_host` in `tokio::time::timeout` with a typed config value; keep
  fail-closed on timeout.
- Preserve the reason in a dedicated error variant and emit one structured warn plus
  a counter; never print the URL. Keep the RPC client string fixed.

Risks: tightening ranges can refuse endpoints previously accepted (e.g. TEST-NET
used by local stubs) — an externally visible change needing a DEVIATION entry and
owner approval; retry/backoff adds subscribe latency; a new error variant touches
`AppError` (GUESS: check other match sites before changing). Do not weaken the
port-443/HTTPS rules while restructuring.

## Failure mode 3 — Redirect chains are enforced elsewhere, and can fail open (GUESS runtime)

The checker validates exactly one URL and never follows redirects. The no-redirect
guarantee lives in `push.rs:471-482`, not in this module: `http_client()` sets
`.redirect(Policy::none())` and a 5 s timeout, but ends with
`.unwrap_or_else(|_| reqwest::Client::new())`. That fallback is a default client —
up to 10 redirects and no timeout — so if `build()` ever fails, a 3xx from a
validated public host to a private address would be followed without re-validation.
`net_safety.rs:11-13` explicitly makes redirects "the caller's to refuse", so every
future caller of this port must re-implement the rule; other paths already bypass the
checker (`wave-12/0252`: Teams destinations).

Hardening:
- Build the no-redirect client once at startup and fail fast on error; delete the
  `Client::new()` fallback so the safe policy cannot be silently dropped.
- Add a contract test with a local mock redirector: a 302 to a private IP and a 302
  to metadata must never be fetched (no third-party target).
- If any caller ever needs redirects, re-run `check_outbound_url` on every hop before
  following; a redirect is a new URL and the previous validation does not cover it.

Risks: startup fail-fast may reject unusual TLS/cert configs that previously fell
back — gate behind validation at boot, not a silent retry; a push vendor that
legitimately redirects would start failing pushes (DEVIATION decision for the owner);
mock-based tests do not prove the behaviour of the real `web-push` request builder
(GUESS — verify against docs.rs before relying on it).

## Priority for v0.4.0

1. Split `Unresolvable` from `PrivateTarget`, add lookup timeout, emit reason metrics
   (small, no wire change, biggest observability win).
2. Expand private-range coverage and test the embedded-v4 cases; record any new
   refusals in `docs/PARITY.md`.
3. Remove the fail-open HTTP client fallback and add the private-redirect contract test.
4. Phase last: re-check at delivery or pin to `resolved`, with REQ-064-tagged tests
   (`req_064_*`) that keep delivery silent on transient failure.

Invariants preserved: HTTPS-only, no credentials, port 443, no metadata/private/
loopback, write-time check at subscribe, fixed client refusal string.
