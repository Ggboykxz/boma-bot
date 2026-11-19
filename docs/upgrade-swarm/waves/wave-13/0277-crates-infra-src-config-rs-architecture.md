# v0.4.0 architecture proposal — `crates/infra/src/config.rs` (infra config structure)

Worker #277 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no code changed).

Status of claims: "current state" statements are CONFIRMED by direct reads of `config.rs`,
`lib.rs`, `db.rs`, `crates/bin/src/config.rs`, `crates/infra/src/openai.rs` and `spec/04-nonfunctional.md` /
`spec/05-known-defects.md` in this worktree. Forward-looking items are proposals (GUESS until
implemented, tested and measured).

## Current structure (CONFIRMED)

- `config.rs` is 163 lines: `ConfigError` (4 variants), `DatabaseSettings { url: String,
  max_connections: u32, acquire_timeout: Duration }`, a validating `new()`, accessors, a hand-written
  `Debug` that replaces the whole URL with `<redacted: contains credentials>` (REQ-054), and 5 unit
  tests (REQ-053/REQ-054). `lib.rs` re-exports `ConfigError`/`DatabaseSettings`; `db.rs` is the only
  consumer of `url()` (via `PgConnectOptions::from_str`) and of the two numeric accessors.
- The module doc states the intended layering: "the binary parses the environment; this module owns
  the *rules*." The binary (`bin/src/config.rs:268-274`) reads `BOMA_DATABASE_URL`,
  `BOMA_DB_MAX_CONNECTIONS` (default 10) and `BOMA_DB_ACQUIRE_TIMEOUT_MS` (default 5000) and calls
  `DatabaseSettings::new`.
- The library errors hardcode the binary's variable names: `ConfigError::OutOfRange` carries
  `name: "BOMA_DB_MAX_CONNECTIONS"` and `ZeroDuration` carries `name: "BOMA_DB_ACQUIRE_TIMEOUT_MS"`.
  That is a reverse dependency from rule-owner to env-parser: a second caller (test, another binary,
  a future config file) would emit a misleading name.
- Validation is split across crates: `new()` checks trim/non-empty, `postgres://`/`postgresql://`
  prefix and `1..=100` / nonzero timeout, but actual URL parseability is only checked later in
  `db.rs::connect_options`. A `DatabaseSettings` can therefore be "valid" yet unusable, which
  conflicts with the boot-time fail-closed rule (spec 04.1: 228 env keys, invalid environment fails
  the process).
- The URL is an ordinary `String`: redaction depends entirely on one manual `Debug` impl. The crate
  already depends on `secrecy` (`crates/infra/Cargo.toml:38`) and uses `SecretString` in
  `openai.rs`/`secret_box.rs`, but `config.rs` does not; the same assurance is manual here.
- Bounds and defaults live in two places (library `1..=100`, binary `10`/`5000`), with no named
  constants or rationale; there is no property test for the scheme/range rules.

## Guiding principle for v0.4.0

Keep infra the pure owner of *rules and secret-safety*, keep the binary the sole owner of *environment
parsing*, and make invalid/unredacted states unrepresentable in the type system rather than in a
hand-written formatter. Preserve the four error variants, the REQ-053/REQ-054 test names and every
current default value; any externally visible change is a DEVIATION to record in `docs/PARITY.md`
before shipping (AGENTS.md).

## Upgrades (5 concrete items)

1. **Wrap the URL in `secrecy::SecretString` and make redaction structural.** Store
   `url: SecretString`, expose `url(&self) -> &SecretString`, and call `expose_secret()` only inside
   `db.rs::connect_options`; switch the manual `Debug` to `.finish_non_exhaustive()` so a future field
   cannot leak by omission. The crate already has the dependency.
   *Risk:* API break for `url() -> &str` and any `Debug`-based string matching; sqlx needs `&str`.
   *Mitigation:* `db.rs` is the only caller (CONFIRMED by grep) so the change is two files; keep the
   rendered text containing `redacted` verbatim, rerun `req_054_...` and add a test that `Clone`d and
   formatted values still hide the password.

2. **Remove `BOMA_*` names from library errors (fix the layering inversion).** Carry generic setting
   identifiers (`"max_connections"`, `"acquire_timeout"`) in `OutOfRange`/`ZeroDuration`; let the
   binary's `ConfigError::Database` adapter prepend the env key in its `Display`.
   *Risk:* startup error text changes and a golden/log assertion could drift; field-name mismatch
   could hide the offending variable from operators. *Mitigation:* keep the message template
   (`>` must be between …`) and add a binary-level test asserting the final error still contains
   `BOMA_DB_MAX_CONNECTIONS`; startup errors are not client-facing, so no wire DEVIATION.

3. **Validate connection-string parseability at construction, with named bounds.** Reuse the same
   `PgConnectOptions::from_str` that `db.rs` uses (single source of truth) inside `new()`, and replace
   `1..=100` and the two scheme literals with documented constants (`MIN/MAX_POOL`, `PG_SCHEMES`).
   *Risk:* a second parse duplicates work and could accept/reject a URL differently from sqlx, or
   reject a currently working deployment URL (a fail-closed regression at boot). *Mitigation:* call
   the identical sqlx parser from both places, map parse failure into a new `ConfigError::InvalidUrl`
   variant, and keep `EmptyUrl`/`NotPostgresUrl` for the existing REQ-053 tests; proptest scheme
   casing and trailing-space inputs.

4. **Co-locate rules, defaults and parsing helpers in infra.** Add `DEFAULT_MAX_CONNECTIONS: u32 = 10`,
   `DEFAULT_ACQUIRE_TIMEOUT_MS: u64 = 5000` and a `DatabaseSettings::from_pairs`-style constructor
   over a `BTreeMap`; reduce the binary to reading raw pairs. Consider `MaxConnections(NonZeroU32)`
   and `AcquireTimeout(Duration)` newtypes so out-of-range values cannot be constructed.
   *Risk:* moving defaults can silently change behavior if the binary's values ever diverge;
   newtypes ripple into `db.rs` accessors and binary tests. *Mitigation:* assert the constants equal
   the current binary defaults in a test, keep the binary's `from_pairs` as an adapter, and expose
   primitive getters so pool options are unchanged.

5. **Property-test the config contract instead of relying on one fixture.** Use `proptest` (already a
   workspace default) over arbitrary passwords, URLs, pool sizes and timeouts: (a) no `Debug`/
   `Display`/`Error` rendering of any generated setting contains any password substring, (b) every
   accepted setting builds a `PgConnectOptions`, (c) every rejected input maps to the expected
   `ConfigError` variant. Keep REQ ids in the test names (`tools/trace_check.py` counts declarations).
   *Risk:* generated "passwords" that are substrings of the redaction marker cause false failures;
   property tests on `SecretString` need careful exposure scoping. *Mitigation:* generate passwords
   from a disjoint alphabet and assert on the formatted output only; this test is the safety net that
   makes items 1 and 3 safe to merge.

## Cross-cutting risks / sequencing

- Sequence 5 → 2 → 1 → 3 → 4: write the redaction/validation property net first, then fix layering,
  then the secret type, then construction-time validation, then defaults/newtypes.
- Keep all four existing error variants, the three current default values, the REQ-053/REQ-054 test
  names, and byte-identical wire behavior; record any externally visible delta in `docs/PARITY.md`.
- Gate every step with `cargo fmt --check`, `clippy -D warnings`, infra unit tests, binary config
  tests and the golden suite. No new dependency is required (items 1 and 5 reuse `secrecy`/`proptest`).

## Out of scope for this module

`HttpConfig` in `crates/api/src/config.rs`, environment reading in `crates/bin/src/config.rs`, and
pool/migration behavior in `crates/infra/src/db.rs` belong to other workers; this proposal only
restructures the shape, validation and secret-handling of the infra settings type.
