# Worker 0197 — `crates/bin/src/config.rs` architecture proposal (v0.4.0)

Lens: architecture. Scope: read-only review of the 901-line file; no code was changed and no
tests were run (read-only assignment). All findings below are **CONFIRMED by source inspection**
of this file plus targeted greps in `crates/` (see Evidence). Items marked RISK are forward-looking.

## Current structure (as read)

- One flat `Config` struct (lines 85–148) holding ~25 fields, with an explicit
  `#[allow(clippy::struct_excessive_bools)]` acknowledging the flag pile. It mixes transport
  (`http`, `database`), telemetry, capability reporting, feature switches (`board`, `metrics`,
  `print_*`), integration clients (`computer`, `provider`, `email_inbound`, `slack`, `billing`,
  `scim`, `automation`) and local paths (`stdio_allowed`, `artifact_dir`, `public_base_url`).
- Satellite settings structs already exist (`BillingSettings`, `ScimSettings`,
  `AutomationSettings`, `ProviderConfig`, `TelemetrySettings`), but the top level did not
  finish the grouping job.
- Single testable entry point `Config::from_pairs` → `BTreeMap` → typed parse; `from_env` delegates.
- `ConfigError` (24–60) mixes typed transparent variants from other crates with four hand-rolled
  stringly variants: `EmailInbound(String)`, `Billing(String)`, `Slack(String)`,
  `PublicBaseUrl(String)`.
- Six hand-written "all-or-none" parsers (`parse_provider`, `parse_email_inbound`, `parse_slack`,
  `parse_billing`, `parse_scim`, `parse_automation`) repeat the same match shape with divergent
  error variants.

## Target structure for v0.4.0

Keep `from_pairs` as the one validated boundary; reorganise the payload into cohesive groups:

```
Config { http, database, telemetry, capabilities,
         integrations: IntegrationsConfig,   // computer, provider, stdio_allowed,
                                              // email_inbound, slack, public_base_url,
                                              // billing, scim, automation, push(VAPID)
         features:    FeaturesConfig }       // board, metrics, artifact_dir,
                                              // print_invite_link, print_reset_link
```

`IntegrationsConfig`/`FeaturesConfig` are internal to the binary crate; add `#[derive(Debug)]`
with custom redacting impls only where secrets appear. Preserve every `BOMA_*` name and every
`req_0xx_...` test name (traceability gate).

## Concrete upgrades

### 1. Wrap every secret in `secrecy::SecretString` inside `Config` (security + consistency)
`ProviderConfig::api_key`, email relay token and Slack secrets are already `SecretString`, but
`ScimSettings.token` (199), `AutomationSettings.token` (209) and `BillingSettings.webhook_secret`
(166) are plain `String` under derived/custom `Debug`. Because `Config` derives `Debug`,
`format!("{config:?}")` currently prints the SCIM and automation bearer tokens in clear text;
REQ-054's redaction test (786–795) only checks the database URL, so the hole is untested.
Change: make those three fields `Option<SecretString>`, give `ScimSettings`/`AutomationSettings`
custom `Debug` that prints only `"configured"`, and add one test seeding all three secrets and
asserting none appear in either `Debug` or `Display` of the full `Config`.
Risk: `crates/bin/src/commands.rs` (67–70, 307–312) clones `Option<String>` and converts at service
build time; those call sites must be updated to pass `SecretString` through. Public struct fields
change type — binary-crate internal, no wire/API impact. Verify no `.expose_secret()` lands in a
`tracing` field.

### 2. Fold VAPID/push into the single typed config (cohesion)
**CONFIRMED:** `BOMA_VAPID_*` is *not* parsed by `Config`; `commands.rs::vapid_config()` (104–106)
reads the environment a second time, while `config.rs` nevertheless hosts `vapid_tests`
(848–901) for `boma_app::push::VapidConfig`. This breaks the module's own "read once at startup"
contract and scatters validation across two files. Change: add `push: Option<VapidConfig>` to
`IntegrationsConfig`, parse it inside `from_pairs` (reusing `VapidConfig::from_env(&env)` logic),
and have `commands.rs` consume `config.integrations.push` instead of re-reading env. Move
`vapid_tests` to target the new field so the REQ-064 names stay in this crate.
Risk: `commands.rs::run` ordering; `testkit` builds `VapidConfig` separately (fine — builder path,
not the binary). Keep the "missing key ⇒ inert, malformed key ⇒ fail fast" semantics (tests 852/860).

### 3. Unify all-or-nothing parsing and replace stringly errors with typed ones (maintainability)
The six parsers return four different error shapes: some `Missing`, some `Invalid`, some a group
string; `parse_scim` reports SCIM failures through `ConfigError::Billing` (625, 631, 634) — a
copy-paste defect that mislabels diagnostics. Change: add a small generic helper for "all present
or all absent" groups and one typed variant, e.g.
`ConfigError::Group { name: &'static str, detail: &'static str }`, plus a distinct `Scim` variant;
keep the operator-facing message text byte-stable where possible.
Risk: error text is operator-visible; changing it may count as a DEVIATION — record in
`docs/PARITY.md` and keep the exact strings unless approved. Tests must keep asserting the same
variants where they already do (738–752, 798–818).

### 4. Validate URLs with a real parser, not string prefixes (correctness)
`parse_public_base_url` (546–562) only checks `starts_with("http://")` plus an emptiness check, and
`passkey_rp_id` (398–405) hand-parses the web origin. `BOMA_PUBLIC_BASE_URL=http://%%%` is accepted.
Change: parse with the `url` crate (via `cargo add`, docs.rs checked), derive host and RP id from
the parsed `Url`, and reject userinfo/fragments for a base URL.
Risk: values previously accepted but malformed now fail at boot — externally visible; log a
DEVIATION and obtain approval before shipping. Adds one dependency (document name/version/license
in `docs/DEPENDENCIES.md`; `url` is MIT/Apache-2.0).

### 5. Resolve the dead `BOMA_SANDBOX` override and add parser property tests (consistency + tests)
**CONFIRMED:** `BOMA_SANDBOX=docker` is set in the capability test (826) but never read by
`from_pairs`; `capabilities.sandbox` is forced from `computer.provider.name()` (327). Either honour
the override (preferred: `optional(&env, "BOMA_SANDBOX").unwrap_or_else(|| computer...)`) or remove it
from `.env.example` and the test. Separately, `parse_ip_list`/`parse_string_list`/`domain_shape_ok`
and the URL parser are external-format parsers with no property coverage.
Change: add `proptest` cases for comma lists (whitespace, empty segments, duplicates) and DNS shape;
add one invariant test that no seeded secret appears in any rendered `Config`.
Risk: honouring `BOMA_SANDBOX` changes `/ready` output for deployments that set it — DEVIATION
check. `proptest` is a dev-dependency only; keep REQ ids in test names (trace gate).

## Sequencing / cross-cutting risks

1. Do #1 and #3 first (mechanical, no behavior change); run `cargo fmt --check`, `clippy -D warnings`,
   tests after each step.
2. #2 next (moves VAPID ownership, touches `commands.rs`); keep REQ-064 tests green.
3. #4 and #5 last — both can change observable behavior; land only after DEVIATION entries in
   `docs/PARITY.md` and owner approval, per `AGENTS.md`.
4. Preserve `#[allow(clippy::struct_excessive_bools)]` removal only if the flag group genuinely
   shrinks; do not silence lints to make the refactor compile.
5. Every claim in this file is CONFIRMED by source/grep inspection only; re-run tests in the
   implementing session before flipping any status to verified-by-test.
