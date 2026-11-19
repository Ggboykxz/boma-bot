# 0123 — crates/api/src/authcap.rs — architecture (v0.4.0)

Worker #123 · module `crates/api/src/authcap.rs` · lens: architecture · READ-ONLY.
Proposals only; no code in this change. Line refs against `before-m5`.

## REQ mapping (honesty check)

`authcap.rs` implements REQ-040 (the sign-in-method report) and REQ-066 (`localFirst`),
not REQ-017. REQ-017 is the *approval gate* — a per-space/per-user policy that gates side
effects and makes them exactly-once — and lives in `crates/domain/src/card.rs`,
`crates/app/src/worker.rs` and the approval ports, not here. The only "gate" this file owns
is the **auth capability render gate**: the client renders a sign-in method when its field
is present/true and renders nothing when it is absent. This proposal treats "capability
gate structure" as that report/gate. GUESS: the REQ-017 tag is a swarm mis-tag — confirm
with the orchestrator; do not manufacture an REQ-017 link in code.

## What the file does (structure today)

A 120-line API-layer DTO module (no IO, no async):

1. `AuthCapabilities` (`:16-40`) — 9 fields: `sso: Option<SsoProvider>`, five bool method
   flags (`password_auth`, `password_reset`, `email_code`, `billing`, `native_passkeys`),
   two optional strings (`reset_url`, `passkey_rp_id`) and `local_first`.
   `#[serde(rename_all = "camelCase")]`, `#[allow(clippy::struct_excessive_bools)]`.
   Every field is `pub` and mutable; only the two most recent Option fields carry
   `#[serde(default)]`.
2. `password_only(reset_url, passkey_rp_id)` (`:45-57`) — the sole constructor: password
   and reset on, everything else off. `bin/src/config.rs:388-395` calls it then mutates
   `local_first` in place; the four test suites do the same.
3. `SsoProvider` (`:63-68`) — a button `name` plus `availability: String` (unbounded).
4. Two unit tests (`:70-120`): REQ-040 asserts all nine JSON keys exist; REQ-066 asserts
   `localFirst` and that an older report without it still deserialises.

Exposure: held as `Arc<AuthCapabilities>` in `AppState` (`state.rs:17,35`), served
unauthenticated at `GET /api/auth/capabilities` (`health.rs:173-188`, `router.rs:35`),
registered in OpenAPI (`openapi.rs:37`). `/readyz` reports a *different* struct,
`ReadinessBody` (`health.rs:31-56`), so the two capability reports are already distinct.

Gap (CONFIRMED from `config.rs:388-395`): config can only ever produce a password-only
report; `sso`, `email_code`, `billing`, `native_passkeys` are never derived from `BOMA_*`
config, so REQ-040's "deployment chooses methods" is only partly reachable today.

## Proposed upgrades (v0.4.0)

### U1. Make the gate's states unrepresentable
Keep the nine wire fields (names are a public contract, REQ-040) but stop letting callers
build arbitrary combinations: add `AuthCapabilities::builder()` / `from_config(...)` that
enforces invariants — `password_reset ⇒ password_auth`, `reset_url.is_some() ⇒
password_reset`, `passkey_rp_id.is_some() ⇒ native_passkeys || password_auth`,
`local_first` only when a local owner resolver exists — plus a `validate()` called at
startup (fail fast, per the AGENTS config rule).
- Risk: tests and `config.rs` currently assign `capabilities.local_first = true` directly;
  keep that field writable or add `with_local_first(self, bool)` and migrate both. Do not
  change field names or types, or the OpenAPI contract diverges (a DEVIATION).

### U2. Type the SSO availability state
Replace `SsoProvider.availability: String` with a small enum (`available` / `degraded` /
`unavailable`) using serde renames, aligning with `ReadinessBody.degraded` (REQ-053). Add
an `#[serde(other)] Unknown` variant if the original emits free-form strings.
- Risk: wire-shape change if the oracle sends an unlisted value — differential-test the
  original's `/api/auth/capabilities` before pinning the enum; otherwise keep `String`
  and add a validated accessor. Any emitted change needs a docs/PARITY.md entry.

### U3. One construction path for every method
Replace `password_only()` + post-mutation with `from_config(&HttpConfig, web_origin)` that
derives all REQ-040 methods (SSO when an OIDC provider is configured, email code when mail
is configured, passkeys when the RP id is set, billing when a provider is set), leaving
`password_only()` as a test/golden helper. This closes the CONFIRMED gap above.
- Risk: larger change in `bin/src/config.rs` and all four test fixtures; GUESS which env
  flags map to which method — verify against `spec/00-overview.md` and the original, and
  record new behavior as a DEVIATION awaiting approval. Keep the `password_only` signature
  intact.

### U4. Wire-contract discipline and versioning
Give every additive field `#[serde(default)]` (bools included) so any older report
round-trips, decide `deny_unknown_fields` explicitly (strict vs forward-compatible), and
add a golden that pins the exact nine-key JSON shape plus a contract test against
`spec/openapi.yaml`. Optionally add a `schemaVersion` marker.
- Risk: bool defaults can mask a mis-built report; `deny_unknown_fields` breaks older
  clients; a version field changes the report (DEVIATION). Mitigate: validate in U1 so
  defaults cannot hide invalid states, and defer the version field until approved.

### U5. Name the gate honestly and expose one method iterator
Document at the top that this is the *client render gate* (absent = not rendered) and that
the REQ-017 and REQ-030 gates live elsewhere. Add an internal `SignInMethod` enum derived
from the fields with `enabled_methods()`, giving client-facing logic one source of truth;
consider renaming the module to `auth_report.rs` behind a `pub use` shim later.
- Risk: enum and fields can drift into two sources of truth — derive the iterator from the
  fields, never store both. A rename multiplies import churn across `lib.rs`, `health.rs`,
  `state.rs`, `openapi.rs` and tests; keep the filename in v0.4.0.

## Execution order & gate

U5 (doc-only) → U4 (serde defaults + golden/contract test) → U1 (builder + validate) →
U3 (single construction path, REQ-040 gap) → U2 (typed availability, only after the oracle
check). Each step: tests-first with REQ ids in test names, `cargo fmt --check`,
`cargo clippy -- -D warnings`, `cargo test`, OpenAPI contract test green, and no wire
change merged without a docs/PARITY.md entry and owner approval.

## What not to touch

REQ-017 approval/idempotency logic in domain/app; the screen capability seal (REQ-030);
REQ-066 `localFirst` semantics; the unauthenticated nature of the route and REQ-041
`cache-control: no-store`; the `Arc<AuthCapabilities>` in `AppState`; the nine wire field
names.

CONFIRMED by reading `authcap.rs`, `health.rs`, `state.rs`, `router.rs`, `openapi.rs`,
`bin/src/config.rs:388-395`, `spec/01-requirements.md` REQ-017/040/053/066 and sibling
wave proposals. GUESS: the REQ-017 tag's intent, the original's SSO availability values,
and the env-to-method mapping — verify before coding.
