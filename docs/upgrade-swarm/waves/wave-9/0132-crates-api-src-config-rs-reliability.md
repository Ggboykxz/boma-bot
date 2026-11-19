# Worker #132 — `crates/api/src/config.rs` reliability proposal (v0.4.0)

Lens: reliability. Posture: READ-ONLY analysis; no source changes here.
Evidence (CONFIRMED): read `crates/api/src/config.rs` (458 lines), `crates/bin/src/config.rs`
(`from_pairs` 257-305, `ConfigError` 22-60, secrecy use at 227/417/786, redaction test at 786) and `AGENTS.md`.

## Role of this module
`HttpConfig` is the validation boundary for all HTTP `BOMA_*` settings. Env parsing lives in the
binary (`from_pairs`), which supplies defaults, calls `HttpConfig::new`, then runs the
`with_*` / `validate_web_root` chain. Both the typed errors and the `Display` output are
operator-visible, so a mistake here is either a failed boot or a misleading one.

## Failure modes

### 1. Invalid env
- FM-1 Syntactic parse failures (`BOMA_BIND_ADDR`, `*_TIMEOUT_MS`, `BOMA_MAX_BODY_BYTES`,
  `BOMA_TRUSTED_PROXIES`, TTL/bool flags) happen in the binary, not here; this enum has no
  "value did not parse" variant. A caller using `unwrap`/`expect` would turn malformed env into a
  panic (forbidden by AGENTS.md outside tests) instead of a typed boot error. GUESS: current helpers
  use `?`, so today it surfaces as `ConfigError::Invalid`, not a panic.
- FM-2 `from_pairs` fails on the first bad variable (`?` chain). The operator fixes one value,
  restarts, and discovers the next: slow feedback, never a complete list of misconfigurations.
- FM-3 No upper bound on `request_timeout` / `readiness_timeout`. `new()` rejects zero and enforces
  readiness <= request, but a multi-day `Duration` is accepted, contradicting REQ-053 ("an
  unbounded request is a stuck task"). Numeric values are range-checked only after parsing.
- FM-4 `trusted_proxies` is accepted verbatim; an unspecified address (`0.0.0.0`) silently widens
  who may spoof `x-forwarded-for` (D-04). Empty = trust none is a good default.

### 2. Missing / implicit defaults
- FM-5 `new()` requires bind, both timeouts, body limit and proxy list: no `Default` impl, no single
  `from_env` here. Defaults are split between the binary (3000 / 30s / 5s / 64 KiB) and this module
  (TTL 30 d, secure cookies true, web_root None, local_first false). Drift risk.
- FM-6 `DEFAULT_INVITATION_TTL_SECONDS` (line 92) is unused in this file: dead or applied elsewhere.
  A reader may assume it is enforced when it is not.
- FM-7 Session TTL silently defaults to 30 days if `with_session_ttl_seconds` is never called,
  despite D-08 saying the operator states the lifetime. A forgotten var yields a long-lived
  credential instead of a boot error.
- FM-8 `with_secure_cookies(false)` is unguarded: a plain-HTTP production bind silently loses the
  `Secure` attribute. Contrast `with_local_first`, which correctly refuses a non-loopback bind.
- FM-9 `validate_web_root()` is a separate step, so `HttpConfig` can reach runtime with an
  unvalidated `web_root` if a caller skips it.

### 3. Secret leakage in logs
- FM-10 `Display` is clean today (bind, timeouts, body limit, `trusted_proxies.len()`, `local_first`):
  no values, no secrets, no proxy addresses. Preserve this exactly.
- FM-11 `HttpConfig` and `HttpConfigError` both derive `Debug`, and `Config` derives `Debug` while
  embedding `HttpConfig`. Any future secret field added here without a `secrecy` wrapper would be
  printed by `{:?}` of the whole config. Latent, not current.
- FM-12 `HttpConfigError::Invalid { name: String, reason: String }` accepts arbitrary owned strings;
  a future caller could place an env *value* in `reason` and the derived `Debug` would log it.
  `LocalFirstNonLoopback { bind }` also prints the internal bind IP (minor topology disclosure).
- FM-13 `ConfigError::Invalid { name, value }` in the binary echoes raw values. GUESS: safe for the
  numeric/bool HTTP vars, unsafe pattern if reused for a credential.

## Concrete hardening (with risks)
| # | Change | Risk / cost |
|---|--------|-------------|
| H1 | Add a typed parse error (name + source, no raw value) here and route env parsing through a single `HttpConfig::from_pairs`, never panicking. | Overlaps binary `ConfigError::Invalid`: needs one owner (ADR). |
| H2 | Accumulate validation errors into a `Vec<HttpConfigError>` so one boot reports every bad variable. | Breaking `?` call sites and `PartialEq` error tests; DEVIATION if the error format is client-visible. |
| H3 | Add `MAX_REQUEST_TIMEOUT` / `MAX_READINESS_TIMEOUT` constants and reject values above them. | Could reject long uploads/SSE; bounds must come from spec/ADR, not a guess. |
| H4 | Reject unspecified/multicast entries in `trusted_proxies` at construction. | New typed error; breaks operators who trust `0.0.0.0` (which is unsafe anyway). |
| H5 | One constructor with documented defaults; retire the scattered binary defaults. | Couples api to env; keep a `from_pairs` variant for tests. |
| H6 | Apply or remove `DEFAULT_INVITATION_TTL_SECONDS` and cover it with a test. | If consumed elsewhere, removing regresses — CONFIRMED unused here only. |
| H7 | Make session TTL explicit on non-loopback binds, or at least warn with the resolved value at startup. | Breaks deployments relying on 30 days; likely DEVIATION, needs approval. |
| H8 | Guard `with_secure_cookies(false)` like `local_first` (loopback / explicit dev flag only). | Breaks plain-HTTP LAN dev; ADR required. |
| H9 | Make `validate_web_root` the only path: store `ValidatedWebRoot(PathBuf)`, canonicalized absolute, remove the optional step. | Public `web_root()` changes `&str` -> `&Path` (DEVIATION); one-time blocking FS call (use `spawn_blocking`). |
| H10 | Manual `Debug` for `HttpConfig`; change `Invalid { name, reason }` to `&'static str` name + fixed reason, never a value; drop/relabel the bind IP in the error. | Small; keep `PartialEq` tests constructing the same constants. |
| H11 | Wrap future credential fields in `secrecy::SecretString` and extend the req_054 test to `HttpConfig`'s own `Debug`. | No current field to wrap; test-only change now. |

## Verification (test-first; REQ id in the test name)
- `req_053_http_config_rejects_an_oversized_timeout`, `req_053_trusted_proxy_list_refuses_unspecified`.
- `req_054_http_config_debug_never_prints_a_value` (no value/reason substring in `format!("{:?}")`).
- `req_066_invalid_pairs_report_all_errors_at_once`, `d_08_session_ttl_has_no_silent_default_on_public_bind`.
- Golden: `Config::from_pairs` with two bad values yields exactly two stable-order diagnostics.

## Open decisions (GUESS where unresolved)
- Env-parsing ownership (this crate vs binary): resolve before H1/H5; likely an ADR.
- Whether aggregated errors alter a client-visible format: check `docs/PARITY.md` before H2.
- H3 bounds must come from `spec/05-known-defects.md` or an ADR, never an invented number.
