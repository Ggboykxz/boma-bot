# Worker #198 — `crates/bin/src/config.rs` reliability proposal (v0.4.0)

Lens: reliability. Posture: READ-ONLY analysis; no source changes here.
Evidence (CONFIRMED): read `crates/bin/src/config.rs` (901 lines), the sibling wave-9
proposal `0132-crates-api-src-config-rs-reliability.md`, and `AGENTS.md`.

## Role of this module
`Config::from_env` / `from_pairs` is the single startup validation boundary for every binary
`BOMA_*` variable. It is fail-fast by design (`?` chain, typed `ConfigError`), and the module
docstring (lines 1-6) promises "Secrets never reach a log line: `Debug` and `Display` both print
the redacted form (REQ-054)". A reliability defect here is either a boot that panics/misleads or
a credential that escapes into logs.

## Failure modes

### 1. Secret leakage
- FM-1 **CONFIRMED — active leak, not latent.** `ScimSettings` (line 195, `token: Option<String>`)
  and `AutomationSettings` (line 206, `token: Option<String>`) derive `Debug` and hold bearer
  tokens as **plain `String`**, unlike `ProviderConfig` (227, `secrecy::SecretString`) and the
  hand-written `BillingSettings::Debug` (176-189). `Config` derives `Debug` (line 81) and embeds
  both, so `format!("{config:?}")` prints `scim.token` and `automation.token` verbatim. This
  directly breaks the REQ-054 promise. The existing `req_054_config_display_and_debug_redact_the_database_url`
  test (786) only asserts the DB password — it passes while these two tokens leak.
- FM-2 `ConfigError::Invalid { name, value }` (32-37) echoes the operator's raw, trimmed value.
  No current secret routes through `optional_parsed`, but the pattern is one refactor away from
  printing a credential; an arbitrarily long bad value also floods logs.
- FM-3 `optional()` (430-434) **trims every value**, including tokens and the DB URL. A password
  with intentional leading/trailing whitespace is silently changed, producing an auth failure whose
  cause is invisible in logs (the real value is never printed).

### 2. Missing env / fail-fast
- FM-4 `from_env` uses `std::env::vars()` (246), which **panics** if any environment variable is
  not valid UTF-8 (possible from some orchestrators/CI). That is a startup panic, not a typed
  `ConfigError`, and contradicts the "fail fast with a typed error" contract.
- FM-5 `from_pairs` collects into a `BTreeMap` (263-266); duplicate keys silently last-win. Harmless
  for real env, but the testable API hides a misconfiguration in callers that build pairs.
- FM-6 `from_pairs` fails on the **first** bad variable via `?`. An operator fixes one value,
  restarts, and finds the next: slow, one-at-a-time feedback with no complete diagnostics.
- FM-7 `artifact_dir: None` is documented (110-111) as "per-process ephemeral directory with a
  warning", but **no warning is emitted anywhere in this module**. A deployment that forgets
  `BOMA_ARTIFACT_DIR` silently loses all artifacts on restart — data loss with no operator signal.

### 3. Invalid config / weak validation
- FM-8 `parse_provider` (408-423) accepts any `endpoint` string and any `model`; a malformed
  endpoint (`not-a-url`) boots fine and fails on the first model call instead of at startup.
  API key has no non-trivial-length check, unlike every other secret here (email 16, Slack 8,
  Stripe 16, SCIM 16, automation 16).
- FM-9 Numeric/byte settings have no bounds: `BOMA_DB_MAX_CONNECTIONS` (270), `*_TIMEOUT_MS`
  (284-293), `BOMA_MAX_BODY_BYTES` (294) accept `0` or absurd values. Only the automation wait
  timeout (650-655) is range-checked. A zero pool or body limit can wedge the service at runtime.
- FM-10 `passkey_rp_id` (398-405) strips a port with `rsplit_once(':')`, which mangles IPv6 hosts
  (`https://[::1]:5173` → `[::`) and yields a wrong relying-party id; passkeys then bind to the
  wrong RP and fail at sign-in, far from the config mistake.
- FM-11 `parse_public_base_url` (545-562) only checks the scheme prefix and non-empty suffix;
  it never validates host syntax. Values like `http://?` pass and produce broken webhook URLs.
- FM-12 `billing_capability` (694-703) uses `.ok()?`, swallowing a bad `BOMA_BILLING` parse for
  the `/ready` report. Today `parse_billing` still fails the boot, so this is masked; it is a
  latent divergence between what `/ready` reports and what was validated.

## Concrete hardening (with risks)
| # | Change | Risk / cost |
|---|--------|-------------|
| H1 | Wrap `ScimSettings.token` and `AutomationSettings.token` in `secrecy::SecretString`; give both a manual `Debug` that prints only `configured`/`None`, as `BillingSettings` does. | Field types change (`Option<SecretString>`); downstream comparisons must use `expose_secret()`. Behavior-neutral for logs; the right first fix. |
| H2 | Extend the req_054 test to assert `format!("{config:?}")` contains neither a SCIM nor an automation token fixture. | Test-only; makes FM-1 a durable gate. |
| H3 | Replace `ConfigError::Invalid.value: String` with a fixed reason (or redacted/truncated value); never store a raw env value in an error. | Changes the error text — DEVIATION if client-visible; check `docs/PARITY.md`. Keep `name` for operator clarity. |
| H4 | Use `std::env::vars_os()` and map non-UTF-8 keys/values to a typed `ConfigError::Invalid`, losing no boot to a panic. | Slightly more code; need a stable error message for invalid Unicode. |
| H5 | Stop trimming secret-bearing vars: parse `required`/`optional` without `trim` for tokens/URLs, or trim only for non-secret flags. | Could break operators who rely on accidental trimming; ADR + release note. |
| H6 | Collect all validation errors into a `Vec<ConfigError>` and report them together at boot. | Changes fail-fast control flow and error shape; likely DEVIATION, needs approval. |
| H7 | Emit an explicit startup warning when `artifact_dir` is `None` (or require it when not degraded). | New log line; requiring it breaks ephemeral/dev deployments — gate on `BOMA_DEGRADED`. |
| H8 | Validate `BOMA_PROVIDER_ENDPOINT` as an http(s) URL and enforce a min key length in `parse_provider`. | Rejects currently-accepted configs; could break an exotic but valid endpoint — ADR, bounds from spec. |
| H9 | Range-check numeric settings (>0; sensible upper bounds from spec/ADR) inside `from_pairs`. | Invented bounds are a GUESS; must be sourced, not guessed, per AGENTS.md. |
| H10 | Parse the RP host with `url::Url` (or handle bracketed IPv6) in `passkey_rp_id`; validate `public_base_url` host syntax. | Adds a dependency or parser; changes passkey RP for IPv6 deployments (DEVIATION). |
| H11 | Route `/ready` billing capability through the same validated `BillingSettings` instead of re-parsing env. | Small refactor; removes the FM-12 divergence. |

## Verification (test-first; REQ id in the test name)
- `req_054_config_debug_never_prints_scim_or_automation_tokens` — assert both fixtures absent from
  `format!("{config:?}")` (makes FM-1/FM-2 a gate).
- `req_053_non_utf8_env_yields_typed_error_not_panic` — feed an `OsString` that is not UTF-8.
- `req_053_invalid_config_error_never_echoes_the_raw_value`.
- `req_053_numeric_settings_reject_zero_and_absurd_values`.
- `req_015_provider_endpoint_must_be_a_url` / `req_015_provider_key_has_min_length`.
- `req_066_missing_artifact_dir_emits_a_warning`.
- `req_064_passkey_rp_id_handles_ipv6_hosts`.

## Open decisions (GUESS where unresolved)
- H1/H2 are safe and internally consistent with the module's own REQ-054 promise — do these first.
- H3/H5/H6/H8/H10 alter config acceptance or error text: likely DEVIATIONS; check `docs/PARITY.md`
  and `spec/05-known-defects.md` and get approval before shipping.
- Numeric bounds (H9) must come from spec/ADR, never an invented constant.
- Confirm whether an outbound warning mechanism already exists at the binary edge before H7.
