# 0124 — `crates/api/src/authcap.rs` — reliability lens — v0.4.0 proposal

Target: `crates/api/src/authcap.rs` (120 lines, DTO + one constructor + two unit tests). Sister
sources read this session: `crates/api/src/health.rs`, `crates/api/src/state.rs`,
`crates/api/src/router.rs`, `crates/bin/src/config.rs:384-405`, `apps/web/lib/api/rpc.ts:797-829`,
`spec/golden/auth-capabilities.json`, `docs/TRACEABILITY.md` (REQ-040 harness), ADR-029 (REQ-066).
READ-ONLY review at `before-m5`; no code changed. CONFIRMED = read in tree; GUESS = runtime-only.

## Current behavior (read, not run)

- `AuthCapabilities` is a public bag of six `bool`s and three `Option`s. Only `password_only()`
  (`:45-57`) builds a coherent value; every field stays `pub`, so any caller can mutate one — the
  module's own test flips `local_first` directly (`:106`). `SsoProvider.availability` is a free-form
  `String` (`:67`), described as "reachable right now" but never probed.
- `crates/bin/src/config.rs:388-395` builds the one report at startup from `HttpConfig` and
  `BOMA_WEB_ORIGIN`: `resetUrl = origin + /reset-password`, `passkeyRpId = host(origin)`,
  `local_first = http.local_first()`. `sso`, `emailCode`, `billing`, `nativePasskeys` are hardcoded
  `None`/`false` — there are no such routes in `router.rs:35-55`.
- `AppState` stores it as `Arc<AuthCapabilities>` (`state.rs:17,35`); `health::auth_capabilities`
  (`:186-188`) only clones it. No runtime refresh, no dependency probe, no validation on serve.
- The client re-declares the shape by hand (`rpc.ts:798-817`) and casts JSON with `as AuthCapabilities`
  (`:828`) — no runtime check. `localFirst === true` skips sign-in entirely (REQ-066); the server
  separately reads `http.local_first()` in `authenticate` (`state.rs:88-92`).
- Global middleware sets `Cache-Control: no-store` on every API response (`router.rs:148-151`), so
  the report is not cached today. The golden fixture pins the wire byte-for-byte.

## Failure modes

1. **Capability drift.** (a) Report vs. routes: the struct can advertise a method with no mounted
   endpoint, or hide one that exists; nothing tests the correspondence. `availability: String` can
   say "available" about an SSO provider that is never configured or probed — a permanent,
   fail-open advertisement. (b) Report vs. client: the hand-written TS interface and unchecked cast
   mean a field renamed or retyped on the server compiles cleanly and silently changes what the
   screen renders; this also violates AGENTS.md criterion 6 (generate clients from OpenAPI).
   (c) Internal: `local_first` has two sources (the report and `HttpConfig`), correct today only
   because one constructor copies the other; `#[serde(default)]` on `sso`/`local_first` (`:18,38`)
   makes a partial report silently look like a different deployment.
2. **Fail-open.** `local_first: true` is an unauthenticated, skip-login flag (`health.rs:175`
   notes the route is deliberately unauthenticated). `HttpConfig::with_local_first` refuses a
   non-loopback bind (`config.rs:217-221`), but the report has no equivalent guard: any future
   constructor that sets the flag without that check hands every caller a local-owner path.
   `#[serde(default)]` on security-relevant fields defaults missing data to "nothing configured",
   and the absence of `deny_unknown_fields` ignores fields whose meaning changed — both fail toward
   hiding a configured method rather than refusing a malformed report.
3. **Stale config / derivation.** The report is frozen at boot: changing `BOMA_LOCAL_FIRST` or
   the web origin needs a full restart. During a rolling restart old and new replicas answer the
   same URL with different capabilities; a client that fetched `localFirst: true` from an old pod
   keeps skipping sign-in after the operator turned it off. `passkey_rp_id` (`config.rs:398-405`)
   strips a port with `rsplit_once(':')`, so an IPv6 origin yields a malformed RP id (e.g.
   `[::1]:5173` → `[::1`, port stripped from inside the brackets) — passkeys register against a
   realm that does not match the served origin (CONFIRMED by reading; runtime effect GUESS).

## Proposed hardening (v0.4.0), with risks

- **H1. Validated construction.** Keep the fields `pub` for serde, but add one
  `AuthCapabilities::from_config(...)` as the only non-test constructor and a `validate(&self)`
  asserting: `local_first ⇒ loopback bind` (pass the bind in), `sso.is_some() ⇒ a known availability
  state`, `passkey_rp_id.is_some() ⇒ a valid host`. Call it at startup and in `AppState::new`.
  *Risk:* changes the library's public construction path; the wire stays identical so no golden
  churn, but the test constructors must migrate.
- **H2. Type `availability`.** Replace `String` with a serde-renamed enum
  (`available | unavailable | unknown`) preserving the same JSON strings; default to `unknown`,
  never `available`. *Risk:* DEVIATION if any past report emitted another string — record in
  `docs/PARITY.md`; otherwise the golden is unchanged.
- **H3. One source for `local_first`.** Pass a validated "loopback bind" fact into the constructor
  and delete the `capabilities.local_first = http.local_first()` assignment; keep the existing boot
  refusal as the single enforcement point. Add `req_066_capabilities_flag_matches_bind` beside the
  `config.rs` REQ-066 tests. *Risk:* dependency direction — `authcap.rs` must not import
  `HttpConfig` (api-internal), so pass a `bind_is_loopback: bool`, not the config.
- **H4. Fail-closed parsing.** Split wire from domain: an `AuthCapabilitiesWire` keeps the current
  `#[serde(default)]` for legacy/golden replay and adds `deny_unknown_fields`; a `TryFrom` converts
  it into the strict served type, rejecting invalid values instead of silently hiding a method.
  *Risk:* the REQ-066 legacy test (`:103-119`) must keep passing — leave the default only on the
  wire DTO, not on the served type; adds one type plus a conversion test.
- **H5. Generate the client.** Replace the hand-written `rpc.ts` interface with a type generated
  from `spec/openapi.yaml` and replace the `as` cast with a boundary check that rejects a response
  missing `passwordAuth`/`localFirst` *before* the skip-login decision. *Risk:* needs an ADR and a
  build-step change (criterion 6); a minimal `typeof` guard is the cheap stopgap until then.
- **H6. Cache and skew discipline.** Assert `Cache-Control: no-store` in a route test so a future
  layer cannot drop it; document that a `localFirst` flip must be a stop-the-world deploy (drain old
  pods first). If cross-version skew must be survivable, add a `generation` field (the concept
  already exists at `wire.rs:359`) and refetch on change. *Risk:* a new field is additive but touches
  `openapi.yaml`, the golden and the TS client — DEVIATION, needs approval; prefer doc + test for
  v0.4.0.
- **H7. Derivation and consistency tests.** Fix `passkey_rp_id` for IPv6 literals/trailing slashes
  and return `None` when the host is not a valid RP id. Add a table-driven test tying the report to
  the mounted routes: password flags true iff those routes exist; `sso/emailCode/billing/
  nativePasskeys` false/None while they do not. *Risk:* IPv6 loopback stops advertising passkeys
  (today it advertises a bogus id) — fail-closed but externally visible; note in `docs/PARITY.md`.
- **H8. Observability.** Log one non-secret startup summary of the report and count each serve;
  alert when replicas disagree on `localFirst`. *Risk:* never log `resetUrl` with a token or any
  secret; the report itself carries none.

## Success criteria / tests

Extend existing harnesses rather than adding new ones: `crates/api/tests/rpc_surface.rs`
(`req_040_the_capabilities_route_reports_the_sign_in_methods`), the `authcap.rs` unit tests, the
`config.rs` REQ-066 tests, the golden replay and the contract test against `spec/openapi.yaml`.
Assert: invalid constructor values are refused; the legacy wire still deserialises; `no-store` is
present; the report matches mounted routes; the client guard rejects a partial body.

## Bottom line

Tiny and correct today, but an unvalidated, mutable, startup-frozen advertisement that drives
skip-login. H1–H4, H7 and H8 are local and wire-neutral; H5 needs an ADR, and H2/H6 may be
DEVIATIONS requiring `docs/PARITY.md` before shipping.
