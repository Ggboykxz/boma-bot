# 0129 — crates/api/src/client.rs — architecture lens

Target: `before-m5` (v0.3.0-rc5), module `crates/api/src/client.rs` (122 lines: one `ClientAddr`
extractor, one `resolve_client_ip` function, four unit tests). Read-only review; nothing edited.
Neighbours read: `authroutes.rs`, `public.rs`, `state.rs`, `config.rs`, `correlation.rs`, `lib.rs`,
`crates/bin/src/config.rs` (`parse_ip_list`), `crates/api/tests/`, `spec/05-known-defects.md` (D-04),
`AGENTS.md`. Claim status: CONFIRMED = read in this tree; GUESS = inferred, needs verify.

## Role today

This is the spoof-proof caller-identity boundary for per-IP throttles (D-01 + D-04). `ClientAddr`
is a `FromRequestParts` extractor reading `ConnectInfo<SocketAddr>` from request extensions with
`Rejection = Infallible`; `resolve_client_ip(peer, headers, trusted)` maps a missing peer to
loopback, returns the peer unchanged unless it is exactly listed in `BOMA_TRUSTED_PROXIES`, and for a
trusted peer walks `get_all("x-forwarded-for")` left-to-right, returning the first `IpAddr` that
parses (CONFIRMED). Config parsing is fail-fast on a malformed list and defaults to empty, so
default-deny is honoured (CONFIRMED). The security logic itself is sound and parity is intact; the
debt is structural — resolution and key construction are copy-pasted, the parser is untested at
property level, and the trust model is deliberately minimal (CONFIRMED).

## Structure findings

- **Nine direct call sites** (one in `authroutes::over_limit`, eight in `public.rs`) repeat the
  identical `resolve_client_ip(peer.0, &headers, state.http().trusted_proxies())` + `format!`
  pattern; four more SCIM routes reach it through `scim_guard`. Every new throttled handler must
  remember to do this correctly (CONFIRMED).
- **Rate-limit keys are ad-hoc strings** — `auth:{bucket}:{ip}`, `plugin-submissions:{ip}`,
  `scim:{ip}` — built from raw `IpAddr` `Display`, with no shared namespace/type (CONFIRMED).
- **Parse surface is wider than the tests.** The trusted branch handles multiple `x-forwarded-for`
  header lines, comma lists, whitespace and arbitrary bytes via `to_str()`, but only four
  hand-picked unit tests exist; no proptest/fuzz for an attacker-controlled header, no integration
  test driving a real axum listener with `ConnectInfo` (the D-04 "non-listed peer" obligation is
  not proven end to end) (CONFIRMED).
- **Trust model is exact-IP, single-header-semantics.** No CIDR for proxy fleets, no `Forwarded`
  (RFC 7239), no IPv4-mapped-IPv6 normalization, no hop count; `get_all` lets a caller-supplied
  header line precede the proxy's appended one (CONFIRMED). Whether this changes real deployments
  is GUESS.
- **`ClientAddr` is a public tuple** (`pub Option<SocketAddr>`) forcing `.0` at every site, and
  conflates "no ConnectInfo" with loopback — documented, but one stripped header would merge all
  callers into one bucket (CONFIRMED). The `#[allow(clippy::unused_async_trait_impl)]` is expected.

## Proposals for v0.4.0

### 1. One `ClientIdentity` extractor owns resolution and key derivation
Add `FromRequestParts<AppState>` extractor that reads peer, headers and `trusted_proxies()` and
yields `ClientIdentity { ip: IpAddr }` with a `rate_key(namespace)` helper; migrate the nine direct
sites and `over_limit`/`scim_guard` to it, keeping `ClientAddr` as a deprecated shim for v0.4.0.
Risks: (a) pure refactor — byte-identical resolved IP and key strings must be pinned by a golden
test before migration, or D-01 counters silently reset on deploy; (b) `Rejection` stays `Infallible`
so no handler behavior changes; (c) extractor ordering in handler signatures must be checked — a
body/header extractor placed before it can consume the request; (d) do not move rate limiting into
the API crate — the extractor only resolves identity, `boma-app` still owns counters.

### 2. Harden XFF parsing with a vetted boundary, preserving parity
Replace the hand-rolled header walk with a maintained parser (e.g. `forwarded-header-value` /
`axum-client-ip`, plus `ipnet` for CIDR trust if operators need proxy ranges), added via `cargo add`
with docs.rs review and a `docs/DEPENDENCIES.md` entry. Canonicalize IPv4-mapped IPv6 and define
single-header precedence. Risks: (a) right-to-left hop counting or `Forwarded` support would change
results for chained proxies — externally visible, so keep left-most-valid semantics or file a
DEVIATION; (b) CIDR trust widens who may steer the header — document as an explicit operator choice,
never a default; (c) a prefix-vs-exact match change alters D-04 tests; (d) verify the crate's license
against `cargo-deny` before merging.

### 3. Close the D-04 test obligation: property + end-to-end tests
Add proptest properties: for any header list and any peer not in `trusted`, result == peer; for any
header with no parseable IP, result == peer; result is always derived from peer or a header entry;
no panic on arbitrary bytes. Build header values via `HeaderValue::from_bytes`. Add one axum
integration test that sends a forged `x-forwarded-for` from an unlisted `ConnectInfo` and asserts a
429 (or the rate-key effect) is unchanged. Risks: (a) REQ ids / `D-04` must appear in test *names*
(`tools/trace_check.py` counts declarations, not comments); (b) arbitrary-byte strategies must avoid
panicking on invalid `HeaderValue` construction in the harness itself; (c) keep the existing four
unit tests as regression anchors.

### 4. Make `ClientAddr` a real newtype; separate "unknown peer"
Replace the public tuple with a private field + `peer()` accessor (and `From<Option<SocketAddr>>`
for test ergonomics), then decide explicitly whether to keep loopback fallback or introduce a
distinct `Unknown` identity. Risks: (a) an `Unknown` bucket changes rate-limit grouping for
in-process/no-`ConnectInfo` callers — a DEVIATION needing `docs/PARITY.md` and owner approval, so
default to keeping loopback and note the footgun; (b) touching the extractor touches every handler
signature — migrate in the same change as proposal 1; (c) preserve `Infallible` rejection.

### 5. Make resolved identity observable without logging personal data
Attach a salted hash of the resolved IP to the `http.request` span in `correlation.rs` (raw IP never
logged — AGENTS.md forbids personal data), and emit it only for throttled requests where triage
needs it. Risks: (a) a per-process salt cannot be correlated across replicas — coordinate with the
D-01 shared-counter work on a stable key; (b) hashing must not leak into response headers or the
`x-request-id`; (c) do not log the forwarded header itself.

## Non-goals and verification

No new endpoints, no wire/field/CLI change, no change to 429 wording or status, no second HTTP
client layer, no spec edit. Verify with: `cargo fmt --check`, `clippy -D warnings`, the full test
suite, a diff of resolved keys before/after migration, the D-04 non-listed-peer integration test,
and `cargo-deny` on any new crate. Record every claim in `docs/STATE.md` as CONFIRMED or GUESS.
