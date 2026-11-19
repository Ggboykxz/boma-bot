# 0278 — `crates/infra/src/config.rs` — reliability proposal (v0.4.0)

Worker #278 · module `crates/infra/src/config.rs` · lens: reliability · READ-ONLY review of
`rebuild-workspace/.worktrees/before-m5` (v0.3.0-era source). Proposals only; no code changed.
Claims: **CONFIRMED** = read in this tree this session; **GUESS** = inference needing a test.

Scope: this 163-line module (`ConfigError`, `DatabaseSettings::new`, redacted `Debug`, four unit
tests) plus its consumers: `crates/bin/src/config.rs::from_pairs` (:268-276, env parsing + defaults),
`crates/infra/src/db.rs` (`connect_options` :93-95, `pool_options` :86-90, `readiness` :125-131),
and fixtures in `crates/infra/tests/{common,database}.rs`. Spec anchors: 04-nonfunctional.md §4.1
("configuration is environment-only, validated at boot; invalid environment fails the process"),
§4.5/§4.6 ("no secret material in logs", redaction in the log path), REQ-053/REQ-054 (module header),
AGENTS config rule ("typed config, validated at startup; secrets wrapped in secrecy and never logged").

## What is already right (keep; do not re-propose)

- Fail-fast shape exists: `new()` is the single validation gate, `from_pairs` calls it at boot before
  the pool is built (CONFIRMED). Errors are typed `thiserror` with static names/numeric values — no
  URL or credential text in any `ConfigError` variant (CONFIRMED).
- `Debug` redacts `url` and the `req_054` test asserts the secret substring is absent (CONFIRMED).
- Pool bounds `1..=100` and a zero-timeout guard reject the two classic misconfigurations (CONFIRMED).

## Failure modes

### F1. Validation is shallow; "validated at startup" is only partly true (high)

`new()` checks only `trim()` + a case-sensitive `postgres://`/`postgresql://` prefix (CONFIRMED,
:60-66). It never parses the URL: `postgres://` with no host/user, an uppercase scheme, an embedded
`?host=` override, bad percent-encoding or a missing database all pass here and fail later in
`PgConnectOptions::from_str` inside `db.rs::connect_options` (CONFIRMED split of responsibility,
GUESS on which malformed strings actually slip through sqlx). Consequences: the binary's typed error
boundary is bypassed (an `sqlx::Error`, not `ConfigError`, escapes boot), `connect_lazy` still accepts
settings that `connect` rejects, and the driver error is the thing that may quote the target
(`db.rs:130` already admits driver errors "can quote the connection target"). No upper bound on
`acquire_timeout` either: an enormous value makes `connect()` hang far past any readiness budget
(CONFIRMED absence, GUESS on a real hang). No relationship check between pool size and timeout
(CONFIRMED).

### F2. Secret protection is a `Debug` impl, not a type invariant (high)

`url: String` is stored in the clear and `url()` is a public `#[must_use] -> &str` returning the full
credential string (CONFIRMED, :41, :87-91). Redaction lives only in `Debug` (:106-114); any future
`Display`, `Serialize`, `tracing` field, or log built from `settings.url()` leaks — the type does not
make the safe path the default, contrary to the AGENTS "wrap secrets in secrecy" rule (CONFIRMED
shape, GUESS on an existing reachable sink). `PartialEq/Eq/Clone` also expose the secret by value
(CONFIRMED, :39). The `req_054` fixture carries a live-looking password with a "secret-scan: allow"
marker (:156), showing the repo relies on scanner exceptions rather than on the type (GUESS on tooling).

### F3. Defaults and bounds drift across two crates (medium)

The defaults (10 connections, 5000 ms) and the env names live in `bin/config.rs` (:270-275), while
this module re-hardcodes the *same* names in error messages (`BOMA_DB_MAX_CONNECTIONS`,
`BOMA_DB_ACQUIRE_TIMEOUT_MS`, :69/:77) and the `1..=100` literal (:67). There is no exported constant
or `Default` impl, so a future edit to either side silently desynchronises messages, ranges and
defaults (CONFIRMED duplication, GUESS that a drift already happened). The header cites REQ-054 and
"the binary parses the environment" but never names the keys it validates against (CONFIRMED). Test
helpers hardcode their own sizes (8, 4, 1), so tests do not pin the production defaults (CONFIRMED).

## Concrete hardening (v0.4.0)

1. **P1 — parse, don't prefix.** Add a private `parse_url` that fully parses the URL here and map
   failure to a new `ConfigError::InvalidUrl { name }` with no secret/reason echo; call it in `new()`
   so boot fails with a typed error. Risk R1.
2. **P2 — `secrecy::SecretString` for `url`** (already a default crate). Keep custom `Debug`; expose a
   single crate-visible `expose_secret()` used only by `db.rs::connect_options`; add a test that no
   public method returns plaintext. Risk R2.
3. **P3 — single source of truth.** Export `DEFAULT_MAX_CONNECTIONS`, `DEFAULT_ACQUIRE_TIMEOUT_MS`,
   `MAX_POOL_SIZE`, `ENV_DATABASE_URL`, `ENV_MAX_CONNECTIONS`, `ENV_ACQUIRE_TIMEOUT_MS` here; import
   them in `bin/config.rs`; test that error `name`s equal the parsed keys. Risk R3.
4. **P4 — bound the timeout.** Add `ConfigError::OutOfRange` coverage for `acquire_timeout` (e.g.
   `> 0..=readiness budget`) and reject pool/timeout combos that cannot converge; document the chosen
   ceiling. Risk R4.
5. **P5 — error-boundary redaction test.** Table-drive every `ConfigError` variant, format each, and
   assert no fixture secret substring; coordinate with `db.rs:130` so driver text stays out of info
   logs. Risk R5.
6. **P6 — property/fuzz tests.** `proptest` for `parse_url`: never panics, rejects non-Postgres
   schemes, empty/whitespace and no-host URLs, and every accepted URL re-parses identically; feed
   generated malformed strings (AGENTS requires property coverage for external-format parsers). R6.
7. **P7 — startup integration test.** `from_pairs` with a syntactically bad URL must fail at config
   with `ConfigError`, not at `connect` with `sqlx::Error`; a syntactically valid but unreachable DB
   must still yield `503` readiness, not a crash (REQ-053). Risk R7.

## Risks / trade-offs

- R1. Parsing twice (here + sqlx) risks divergent acceptance; align on sqlx's parser and never print
  its raw error. Uppercase-scheme acceptance may change (record a DEVIATION if externally visible).
- R2. `SecretString` touches `db.rs` and every fixture; verify it stays out of `tracing`/`serde`.
- R3. Cross-crate constant refactor; keep error text byte-identical or log a docs/PARITY.md entry.
- R4. New bounds can reject a previously accepted config — a behaviour change needing ADR/DEVIATION.
- R5. Fixture-string assertions are brittle; keep the marker secret unique and clearly non-production.
- R6. Adds `proptest` to infra's dev deps and a URL-generation strategy to maintain.
- R7. Needs a testkit pair source and a lazy-pool double; no network in the test.
- All edits in English; keep `clippy -D warnings`, `cargo fmt --check`, `req_053`/`req_054` green.

## Explicit non-goals

No rename of `BOMA_*` keys, no secret-manager/KMS integration (DEC-10), no change to the redacted
`Debug` shape, no new externally visible config behaviour without a docs/PARITY.md DEVIATION.

## Suggested rollout

Wave A (infra-only, revertable): P2, P5, P6 — type-level secrecy and tests, no API change. Wave B
(cross-crate): P1, P3, P7 with differential boot tests so config errors stay typed. Wave C: P4 behind
an ADR. Acceptance: a bad URL fails at `new()` with a typed, secret-free error; the secret is one
narrowly-scoped accessor deep; defaults and names have one definition. All facts CONFIRMED by reading
this tree; parse/leak/hang impact marked GUESS needs the tests above. No command executed this pass.
