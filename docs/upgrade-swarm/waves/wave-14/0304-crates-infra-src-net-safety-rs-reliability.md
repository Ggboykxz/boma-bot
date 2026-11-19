# v0.4.0 reliability proposal — `crates/infra/src/net_safety.rs` (worker #304, lens: reliability)

## Current design (CONFIRMED — read in this session)

`check_outbound_url` parses the URL, enforces scheme/port/credentials rules, resolves the host once
with `tokio::net::lookup_host`, rejects the URL if *any* resolved address is private/metadata, and
returns `CheckedOutbound { host: String, port, resolved: Vec<IpAddr> }`. The doc comment states that
redirects are the caller's responsibility (`redirect::Policy::none()`) and that "a following request
never be validated twice." `resolved` is documented as "Kept for the audit trail (D-05)."

**Core reliability gap (CONFIRMED):** the guard validates an address set but hands the caller only a
*hostname*. The audited `resolved` IPs are never pinned to the connection the caller actually makes.
Validation and use are two separate DNS events. This is a check-to-use (TOCTOU) seam, not a closed
boundary. Everything below follows from that.

## Failure modes

### 1. DNS rebinding / TOCTOU (CONFIRMED from code; severity: critical)

An attacker-controlled name (TTL=0) returns a public IP during `lookup_host`, then `127.0.0.1` or
`169.254.169.254` when the caller's HTTP client re-resolves `host` at connect time. The guard has no
way to detect or prevent the second answer. CNAME chains that flip answers produce the same result.
Because `resolved` is audit-only, even a correct-looking audit trail can accompany a request that
reached metadata. A second, subtler case: a multi-answer response where a private address appears
only on the *second* resolution is accepted (first resolution saw all-public).

### 2. IPv6 / IPv4-embedded address gaps (CONFIRMED from code; severity: high)

`is_private_address` correctly unwraps `::ffff:a.b.c.d` via `to_ipv4_mapped()` and blocks
`::/16`-segment-zero, `fe80::/10`, `fc00::/7`. It does **not** decode other IPv4-embedded forms that
can tunnel to private IPv4:

- `2002::/16` 6to4 — IPv4 is bits 16–47 (`2002:0a00:0001::` = 10.0.0.1).
- `2001:0000::/32` Teredo — IPv4 obfuscated in the low 32 bits.
- `64:ff9b::/96` NAT64 — IPv4 in the low 32 bits; a NAT64 gateway can reach RFC1918.
- Deprecated site-local `fec0::/10` (0xfec0 is outside the checked `0xfe80..=0xfebf` window).

Multicast `ff00::/8` and reserved `100.64/10`-adjacent IPv6 special ranges are also unhandled. Note:
these are reachable only if the host network actually has 6to4/Teredo/NAT64 routes — GUESS, verify
per deployment.

### 3. Redirect bypass (CONFIRMED as a contract gap; severity: high)

`CheckedOutbound` imposes no redirect policy. `Policy::none()` is a *doc convention*, not an
invariant of the type. Any caller that configures `Policy::limited()`/`follow()` gets redirects to
`http://169.254.169.254/...` or `http://127.0.0.1/...` with no re-validation — the exact bypass D-05
exists to stop. The one-shot resolution model cannot support safe redirects even if revalidation is
desired, because each hop needs its own pinned, re-resolved IP.

### 4. Related reliability defects (CONFIRMED)

- **No DNS timeout.** `lookup_host` is unbounded; a hanging resolver stalls the guard indefinitely,
  violating the workspace rule "a timeout on every outbound call."
- **Metadata hostname check is textual only** (`METADATA_HOSTS.contains(host_str)`). Trailing-dot
  forms (`metadata.google.internal.`) and CNAME aliases evade it; today they are caught only if the
  resolved IP equals a literal metadata address — reliable for `169.254.169.254`/`fd00:ec2::254`,
  not guaranteed for future metadata IPs.
- **`allow_private` releases scheme and port, permitting arbitrary schemes** (`ftp://` → port 21,
  etc.). Intended for LAN HTTP services; undocumented and worth constraining to `http`/`https`.

## Concrete hardening for v0.4.0

1. **Pin the validated IP to the connection (closes #1).** Change `CheckedOutbound` to carry
   ` resolved: Vec<SocketAddr>` plus the original `host`; require callers to dial the pinned address
   with reqwest's `resolve_to_addrs(host, addrs)` so TLS SNI/Host stay the hostname. Add a local-DNS
   rebinding integration test (public answer first, private on second). GUESS: verify reqwest
   `resolve_to_addrs` keeps SNI = URL host and is keyed correctly in the pool — confirm before coding.
2. **Resolver-time enforcement.** Prefer a custom `resolve` that re-runs `is_private_address` at
   *every* connection/IP selection, so a flipped answer is rejected even after check time. If that
   is too invasive for v0.4.0, at minimum pin + short-lived `CheckedOutbound` (document that the
   value must be used immediately) and a `tokio::time::timeout` around `lookup_host` (start ~5 s).
3. **Cover embedded IPv4 (closes #2).** Extend `is_private_address` to decode 6to4 (`0x2002`),
   Teredo (`0x2001,0x0000`), NAT64 (`0x0064,0xff9b`) and site-local `fec0::/10`; feed decoded IPv4
   back through the v4 branch. Add a `proptest` that no private IPv4 embedded in these prefixes is
   ever classified public.
4. **Make redirect policy an invariant (closes #3).** Either have this module construct the
   `reqwest::Client` with `redirect::Policy::none()`, or expose a `make_client()`/`CheckedRequest`
   type that cannot be built otherwise. If any hop must follow redirects, re-run `check_outbound_url`
   with a redirect callback and pin each hop; never reuse the first hop's answer.
5. **Normalize metadata matching.** Strip a single trailing dot from `host_str` before comparison and
   keep the resolved-IP metadata check authoritative. Constrain `allow_private` to `http`/`https`.

## Risks

- Pinning IPs changes connection pooling keys and port/SNI behavior; wrong `Host`/SNI breaks TLS cert
  validation for virtual-hosted integrations (DEVIATION risk — record in `docs/PARITY.md` if visible).
- Re-resolving per hop adds DNS latency; a too-short timeout yields false refusals on poor networks.
- Blocking NAT64/6to4 may refuse legitimate public IPv6 destinations that embed public IPv4 — scope
  the decoder to *private* embedded values only, not the whole prefix.
- Forcing `Policy::none()` may break callers that relied on redirects; treat as a behavior change and
  gate behind operator approval per AGENTS.md.

## Verification to add

`is_private_address` property test across all prefixes; rebinding and redirect-to-metadata
integration tests with a controllable local resolver/wiremock; a test that a hanging DNS call is
cut off by the timeout. All claims above are CONFIRMED from source reading except items marked GUESS.
