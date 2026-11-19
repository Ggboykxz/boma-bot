# v0.4.0 architecture proposal — `crates/api/src/config.rs` (HTTP config)

Worker #131 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no code changed).

Status of claims: every "current state" statement below is CONFIRMED by direct reads of
`crates/api/src/config.rs`, `crates/bin/src/config.rs`, `crates/infra/src/config.rs`,
`crates/infra/src/secret_box.rs`, `crates/app/src/push.rs` and the `HttpConfig` call sites in
`crates/api/tests/`, `crates/conformance/tests/` in this worktree. Forward-looking items are
proposals (GUESS until implemented and verified).

## Current structure (CONFIRMED)

- `HttpConfig` is one flat struct of nine private fields — `bind`, `request_timeout`,
  `readiness_timeout`, `body_limit_bytes`, `trusted_proxies`, `session_ttl_seconds`,
  `secure_cookies`, `web_root`, `local_first` (config.rs:64-78) — with only getters and a derived
  `Debug`. There are no newtypes: a `usize` is a byte limit, an `i64` is a TTL.
- Validation is split across two fallible phases. `new()` checks the five core parts
  (config.rs:102-140); the other four are attached by consuming builder methods
  (`with_session_ttl_seconds` → `Result`, `with_secure_cookies` → infallible,
  `with_web_root` → infallible, `with_local_first` → `Result`), and `validate_web_root()` is a
  *separate* fallible step (config.rs:150-223). Nothing in the type forces a caller to invoke any
  of them, so the module's "validated once at startup" promise is a convention, not a guarantee.
- Environment parsing lives in another crate: `bin/src/config.rs:278-305` maps `BOMA_BIND_ADDR`,
  `BOMA_REQUEST_TIMEOUT_MS`, `BOMA_READINESS_TIMEOUT_MS`, `BOMA_MAX_BODY_BYTES`,
  `BOMA_TRUSTED_PROXIES`, `BOMA_SESSION_TTL_SECONDS`, `BOMA_SECURE_COOKIES`, `BOMA_WEB_ROOT` and
  `BOMA_LOCAL_FIRST` onto `new()` + the builder chain. `HttpConfigError` hardcodes those env names
  in its messages (config.rs:19, 32, 42, 51, 111, 116, 167), coupling the API crate to the
  deployment's env contract while the parsing itself is untestable here.
- This crate does not follow the pattern already used elsewhere: `SecretBox::from_env(&BTreeMap)`
  (infra/secret_box.rs:49) and `VapidConfig::from_env(&BTreeMap)` (app/push.rs:50) parse inside the
  owning crate from an injected map, so tests exercise the real env → config path.
- Bounds are asymmetric: body limit 4 KiB..=1 MiB and session TTL 300 s..60 d are bounded, but the
  two timeouts are only checked non-zero (config.rs:109-121). `DEFAULT_INVITATION_TTL_SECONDS`
  (config.rs:92) has no corresponding field — a dangling constant.
- Cross-field checks exist for readiness ≤ request timeout and `local_first` ⇒ loopback bind, but
  not for `secure_cookies = false` on a public bind or for `trusted_proxies` contents.
- `Display` prints six fields (config.rs:275-288); derived `Debug` prints all nine, including the
  `web_root` path. `infra/src/config.rs:106-114` shows the intended alternative: a hand-written,
  redacted `Debug` (REQ-054).
- All unit tests in this file call `new()` directly (config.rs:290-458); no env ingestion is covered.

## Guiding principle for v0.4.0

Make configuration one typed boundary: the owning crate parses the environment, validates every
cross-field invariant, and returns a value whose very type is valid. Keep env/deployment naming in
the binary edge, not in domain error strings.

## Upgrades (5 concrete items)

1. **One fallible `HttpConfig::from_env(&BTreeMap<String, String>)`.** Move the mapping at
   `bin/src/config.rs:278-305` into this crate and have the binary call it once; make the builder
   chain crate-private afterwards. Invalid states stop being representable: no caller can obtain an
   `HttpConfig` that skipped `validate_web_root()`. *Risk:* the bin's `ConfigError::Http` `#[from]`
   and error text are consumed elsewhere; moving parsing changes where a bad var is reported.
   *Mitigation:* keep `new()` public for tests/goldens during migration, preserve the exact
   rendered messages, and add table-driven tests for missing/malformed/out-of-range values of all
   nine vars.

2. **Newtypes and grouped sub-structs.** Introduce `BodyLimit`, `SessionTtl`, `TimeoutMs` with the
   bounds as associated consts, and group the flat fields into `Limits`, `Cookies`, `Static`,
   `Proxies` sections that `HttpConfig` composes. Bounds then live with the type instead of in
   `new()`. *Risk:* changes every getter call site (`bind()`, `session_ttl_seconds()`, etc.),
   including `AuthCapabilities` in `bin/src/config.rs:388-394` and test fixtures. *Mitigation:*
   land accessors as additive thin delegates first, migrate callers, then deprecate the old names;
   keep field names stable for golden fixtures.

3. **Bound the remaining values and harden cross-field rules.** Add `MAX_REQUEST_TIMEOUT_MS` and
   `MAX_READINESS_TIMEOUT_MS` (an unbounded request is a stuck task, per AGENTS.md/REQ-053), reject
   `secure_cookies = false` on a non-loopback bind unless a separate explicit acknowledgement var
   is set, and reject `trusted_proxies` entries that mirror the bind address. Fold
   `validate_web_root()` into construction (REQ-055). *Risk:* tightening limits is externally
   visible — existing deployments may break — so AGENTS.md requires a DEVIATION entry in
   `docs/PARITY.md` and approval. *Mitigation:* choose generous ceilings (e.g. 5 min request,
   30 s readiness), document them in `.env.example`, and announce them before v0.4.0.

4. **Stable, redacted rendering and a decision on the dangling constant.** Replace the derived
   `Debug` with a hand-written one in the style of `infra/src/config.rs:106-114`, and make `Display`
   the single summary used by `bin/src/config.rs:370-382`. Either wire `DEFAULT_INVITATION_TTL_SECONDS`
   into a bounded `invitation_ttl` field with its own env var, or delete it — no silent drift.
   *Risk:* adding/removing an env var changes the config surface (record in `docs/STATE.md`), and
   tests may assert the current `Display` string. *Mitigation:* keep the existing field order and
   text, add a test asserting no path/secret-bearing field appears in either rendering (mirroring
   `req_054_...never_prints_the_password`), and treat the TTL choice as a proposal for the owner.

5. **Contract and property tests inside the owning crate.** Property-test every numeric parser over
   the full `u64`/`usize` range plus boundary values (AGENTS.md: fuzz/property tests for external
   formats), and add a test that every `BOMA_*` name appearing in `HttpConfigError` has a reader in
   `from_env`, so the two cannot drift apart. *Risk:* new dev-dependency must go through `cargo add`
   and `docs/DEPENDENCIES.md`; use the workspace `proptest` if already present. *Mitigation:* keep
   REQ ids in test names (040/053/054/055/066, D-08) because `tools/trace_check.py` counts
   declaration lines.

## Cross-cutting risks / sequencing

- Sequence 1 → 4 (additive, same error text) → 2 → 3 → 5; property tests accompany each step.
- The REQ-066 loopback-only `local_first` invariant (config.rs:217-223) must never be widened.
- No secrets are stored here today; the automation and master-key tokens live elsewhere. Do not
  pull them into this struct without `secrecy::SecretString` and a redacted `Debug`.
- Verification gate for each step: `cargo fmt --check`, `clippy -D warnings`, unit + `tests/`
  integration, plus golden and contract suites.

## Out of scope for this module

The binary's aggregate `Config`, database/pool settings, provider credentials and feature flags
(`BOMA_BOARD`, `BOMA_MESSAGING`, …) are separate structures; this proposal only restructures the
HTTP configuration boundary and its environment contract.
