# Worker #130 — `crates/api/src/client.rs` reliability proposal (v0.4.0)

- Module: `crates/api/src/client.rs`; lens: reliability; mode: READ-ONLY proposal.
- CONFIRMED (read this session): `client.rs` (all 122 lines); call sites in
  `crates/api/src/{public,authroutes}.rs`; `crates/api/src/config.rs` L267-272;
  `crates/bin/src/config.rs` `parse_ip_list` L451-466 and the `BOMA_TRUSTED_PROXIES`
  default L295; `spec/05-known-defects.md` D-04; `.env.example` L17.
- Proposals and risks are GUESS (design inference; not implemented, not tested).
- No wire/API/data-format change is proposed; anything externally visible is flagged as a
  DEVIATION below and needs `docs/PARITY.md` plus approval before shipping.

## Scope note — mapping the brief onto this module

This file is *not* an outbound HTTP client. It is a pure, synchronous IP resolver for
per-client rate limiting: `ClientAddr` is an Axum `FromRequestParts` extractor reading
`ConnectInfo<SocketAddr>` from request extensions, and `resolve_client_ip(peer, headers,
trusted)` picks the bucket key (trusted-proxy XFF, else peer, else loopback). There is no
socket, no `.await`, no timeout, no connection pool and no error type here. I therefore map
the three named failure modes onto the real reliability surface: timeouts → unbounded
hot-path work and stale peer data; connection reuse → peer-identity stability across
keep-alive and proxy hops; error mapping → silent, conflated fail-closed fallbacks with no
observability. I explicitly recommend NOT adding network clients, pools or timeouts here.

## Failure modes

### FM1 — "Timeouts": no deadlines, but unbounded hot-path work (CONFIRMED)
- `resolve_client_ip` L53-74 scans every header value from `get_all("x-forwarded-for")`
  (L63) and every comma entry (L67), calling `parse::<IpAddr>()` per entry with no cap on
  value/entry count. An attacker-influenced XFF forwarded by a trusted proxy makes every
  rate-limited request do unbounded O(N) parsing. Hyper's header-byte limit bounds bytes,
  not this CPU work.
- `trusted.contains(&peer)` L59 is a linear scan of the whole trusted list on every request;
  small today, but avoidable hot-path cost.
- No size/time bound and no instrumentation: a regression that inflates XFF or the trusted
  list degrades latency silently.

### FM2 — "Connection reuse": peer identity can be stale, shared or spoofed (CONFIRMED)
- `ClientAddr` L21-38 reads `ConnectInfo` once from request extensions. On a reused
  keep-alive connection the peer is fixed at accept time (correct), so two callers behind a
  proxy reusing one downstream socket share one bucket key.
- If the server is not built with `into_make_service_with_connect_info`, `ConnectInfo` is
  absent and L58 maps *every* caller to `Ipv4Addr::LOCALHOST`. All anonymous callers share
  one bucket — either a global 429 storm or one global bypass depending on the limiter. No
  startup check detects this silent, reliability-critical misconfiguration.
- XFF trust-chain risk: L46-47 takes the *left-most valid* entry, correct only if the
  trusted proxy sanitizes inbound XFF. A proxy that appends the real client on the right but
  leaves an attacker-supplied left-most value intact lets the attacker choose the bucket key
  and evade the throttle (compounds D-01/D-04). `get_all` (L63) treating multiple headers
  identically widens this ambiguity.
- Format ambiguity: `parse::<IpAddr>()` accepts IPv4-mapped IPv6 (`::ffff:198.51.100.4`)
  and zone-suffixed IPv6, which key differently from plain `198.51.100.4` — a caller can
  rotate bucket keys by changing the textual form of the same address. Bracketed/port forms
  (`[::1]:1`) fail to parse and are skipped: an inconsistent fallback.

### FM3 — "Error mapping": fail-closed but conflated and unobservable (CONFIRMED)
- `Rejection = Infallible` L25 is good for availability (a missing peer never becomes a
  400/500) and resolution is fail-closed. But four causes collapse to one answer with no
  signal: no peer (loopback), unlisted proxy (peer), non-UTF-8 header (`to_str()` Err,
  skipped L64), and garbage/parse-failure header. Operators cannot distinguish a spoof
  attempt from a misconfiguration from a legitimate in-process call.
- No `tracing` import here: no event/counter for "XFF honored", "fallback used" or
  "invalid header seen", so the D-04 abuse path is unmonitorable.
- External error mapping lives in `ApiError`/`error.rs`; this module returns no errors, so
  there is no client-visible error format to change.

## Concrete hardening for v0.4.0 (GUESS — not implemented)

- **H1 (FM1):** Cap work — process at most the first N header values and first M entries
  (e.g. N=4, M=16), short-circuiting on the first valid IP (already implicit at L69);
  replace `trusted.contains` with a precomputed `HashSet<IpAddr>` in config. *Risks:* a
  legitimately long proxy chain beyond M resolves to a less-specific entry → bucket change
  → DEVIATION; constants are a guess needing topology confirmation; startup allocation only.
- **H2 (FM2):** Canonicalize the chosen IP: map IPv4-mapped IPv6 to v4, reject
  unspecified/multicast/loopback-as-forwarded, and key on the canonical form. Add a startup
  assertion that HTTP is wired with `ConnectInfo`, or an explicit logged "loopback fallback
  active" warning in non-test builds. *Risks:* canonicalization changes bucket keys → one
  limits reset (DEVIATION, record it); rejecting reserved addresses could drop a valid
  caller; the warning must not surface as a JSON error or fail liveness.
- **H3 (FM2):** Define the XFF trust-chain contract: require the trusted proxy to strip
  inbound XFF (or bound accepted hops to a configured chain depth), with a test that a
  forged left-most value from an untrusted hop is ignored even when a trusted proxy appends
  the true client. *Risks:* hop-count needs config that does not yet exist → new env var is
  a separate proposal in `docs/STATE.md`, not shipped here; wrong settings break valid
  multi-proxy deployments.
- **H4 (FM3):** Return an internal typed outcome (`Resolved::{Peer, Forwarded,
  TrustedFallback, NoPeer}`) from a private helper, keep `resolve_client_ip`'s `IpAddr`
  signature unchanged, and emit a `tracing` debug event plus a metric for fallback/invalid
  counts (no IPs or PII in logs). This makes D-04 abuse observable without touching the API
  error format. *Risks:* hot-path tracing needs level checks; metric names/cardinality must
  be fixed; the enum is dead weight if nobody consumes the events.
- **H5 (tests, FM1-FM3):** Add proptest coverage for this parser of an external format
  (arbitrary bytes; IPv4/IPv6/mapped/zone/bracket forms; empty and huge lists; multiple
  headers) plus unit tests for "no peer → loopback", "untrusted multi-header spoof ignored"
  and the canonicalization cases, REQ-tagged per project rules. *Risks:* proptest is a new
  dev-dependency (verify `Cargo.toml`); a test pinning current left-most semantics must be
  updated if H2/H3 land.

## Verification (proposed; NOT run — read-only)
- After each change: `cargo fmt --check`, `cargo clippy -- -D warnings`,
  `cargo test -p boma-api`.
- Property: for all inputs `resolve_client_ip` terminates, stays bounded and returns a
  canonical IP that is the peer, loopback, or a valid forwarded address.
- Golden/regression: existing D-04 tests L91-121 stay green; add spoof and mapped-address
  cases to `spec/golden/` only if they assert wire behavior.

## Constraints / parity
- Keep the fail-closed contract and the `Infallible` extractor: a missing peer must never
  5xx a request.
- Keep `resolve_client_ip`'s public signature and the left-most-valid rule unless H2/H3 are
  approved — changing bucket keys or caller selection is externally visible → DEVIATION in
  `docs/PARITY.md`, await approval.
- No secrets, IPs or personal data in logs; no timeouts, pools or clients added to this pure module.
