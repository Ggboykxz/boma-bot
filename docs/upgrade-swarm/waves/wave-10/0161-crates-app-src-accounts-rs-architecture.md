# v0.4.0 architecture proposal — `crates/app/src/accounts.rs` (accounts / sessions / recovery)

Worker #161 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no code changed).

Status of claims: all "current state" statements below are CONFIRMED by direct reads of
`accounts.rs`, `ports.rs` (AccountStore/UserStore/SessionStore/PasswordResetStore/PasswordHasher/
TokenGenerator), `service.rs`, `error.rs`, `usecases.rs` and `authroutes.rs` in this worktree.
Forward-looking items are proposals (GUESS until implemented and tested).

## Current structure (CONFIRMED)

- The module carries three entry shapes for one concern: a stateful `Accounts { app, session_ttl_seconds }`
  with `sign_up`/`sign_in` (`:51-204`), two *free* recovery functions taking `&App` + TTL
  (`request_password_reset` `:221-273`, `reset_password` `:284-336`), and thin re-export wrappers in
  `usecases.rs:41-72`. `reset_password` reconstructs `Accounts::new(app.clone(), session_ttl_seconds)`
  (`:334`) solely to reach the private `grant_session` — a clone and a second constructor for one call.
- `grant_session` (`:177-203`) is a separate store call from `create_account` (`:92-118`); the
  transaction comment (`:66-69`) promises atomicity for the *account* rows only. `sign_up` can leave a
  created account with no session, and a retry then hits `AppError::Conflict` on the address.
- `reset_password` chains `consume` → `revoke_all_for_user` → `set_password` → `delete_for_user` →
  `find` → audit → `grant_session` (`:297-335`); three of these security-relevant writes are
  `let _ =` best-effort with no metric or log. The atomicity comment covers only the token consume.
- Uniform refusal is stringly-typed: `RECOVERY_REFUSED` is wrapped in
  `AppError::Rule(DomainError::InvalidName(..))` (`:210`, `:298-301`), reusing a *name* error for a
  recovery-token refusal. Mapping is exhaustive in `service.rs:603-620` and `api/src/error.rs:193-211`.
- Secrets are plain `String`/`&str`: `SignUp.password`, `SignIn.password`, `SessionGrant.token`,
  recovery `token`/`password`, all deriving `Debug` (`:17-47`) — a logged request prints the password.
- Timing mitigation is heuristic and duplicated: the unknown-user branches call `hasher.hash(email)`
  (`:139`, `:231`) while the known-user path calls `verify`, and the comment at `:228-230` admits the
  mint-and-insert branch remains distinguishable.
- `expires_at` is an RFC3339 `String`, compared lexicographically in `usecases.rs:87`; TTL policy is
  split between a per-call `session_ttl_seconds` (passed from `authroutes.rs:192,234,271`), the module
  constant `RECOVERY_TTL_SECONDS` (`:207`), and invite TTLs elsewhere.
- The three in-module tests (`:338-369`) exercise `boma_domain` directly and never call `Accounts`;
  the real flows are covered only from the API integration suite (D-08 asks for explicit evidence).

## Guiding principle for v0.4.0

One cohesive, typed auth service with one construction site and one config object; typed errors and
typed secrets at its boundary; store-level atomicity for multi-write security transitions; and
evidence (tests/metrics) for every "same answer either way" claim.

## Upgrades (3-5 concrete items)

1. **One service, one construction site.** Fold `request_password_reset` and `reset_password` into
   `impl Accounts` (or a `PasswordReset` collaborator owned by it); keep the existing free functions in
   `usecases.rs` as thin delegating wrappers so call sites do not change in the same commit. Remove the
   `app.clone()` at `:334` and the per-call TTL by reading a typed `AuthConfig { session_ttl_seconds,
   recovery_ttl_seconds }` from `Services` (validated at startup, satisfying D-08).
   *Risk:* touches `usecases.rs` and `authroutes.rs` signatures if wrappers are removed too early.
   *Mitigation:* two-step landing — wrappers first (no behavior change), signature cleanup behind a
   follow-up; keep default TTL values byte-identical, or record a DEVIATION in `docs/PARITY.md`.

2. **Typed errors instead of reused domain variants.** Add a dedicated `AppError::InvalidRecoveryToken`
   mapped to the same uniform 400 envelope, and use it at `:298-301` instead of
   `DomainError::InvalidName("the recovery link is invalid or it expired")`. Keep `RECOVERY_REFUSED`
   as the single wire-level string.
   *Risk:* `AppError` matches are exhaustive in `service.rs:603-620` (add a `Failure` reason) and
   `api/src/error.rs:193-211`; golden fixtures may assert the current error text. *Mitigation:* update
   both maps in one commit, preserve the response body string, run the golden/contract suites.

3. **Atomicity and honest best-effort writes.** Move session minting into the account-creation
   transaction (generate token before the port call; `create_account` gains the session row and returns
   the grant), or add an idempotency key so a retried sign-up resumes instead of conflicting. Collapse
   the reset chain into one store port call (`consume_and_reset`) that consumes the token, revokes
   siblings, sets the hash and kills sessions in one transaction. Emit a counter for every swallowed
   audit/revoke failure instead of `let _ =`.
   *Risk:* changes `AccountStore`/`PasswordResetStore` signatures in `ports.rs` and their infra adapters
   and test fakes — the widest change here; token generation must stay outside the DB call.
   *Mitigation:* stage the port addition (add new method, migrate callers, delete old one); keep the
   single-use/`expires_at > now` SQL predicate from `:294-301` intact and add a concurrency test.

4. **Secrets and time as types.** Wrap `SignUp.password`, `SignIn.password`, reset `token`/`password`
   and `SessionGrant.token` in `secrecy::SecretString` (redacted `Debug`), and consider a `Timestamp`
   newtype for `expires_at` with real ordering rather than string comparison. Map wire DTOs to these
   secrets at the API boundary (`authroutes.rs:219-243,268-277`).
   *Risk:* `Debug`/`PartialEq` shapes and API DTO construction change; `Timestamp` spans `Session` in
   `boma_domain`, ports and infra. *Mitigation:* ship secret typing first (local to this crate + the
   auth routes), treat `Timestamp` as a separate cross-crate ADR; add a test that no password appears
   in `format!("{request:?}")` or in tracing output.

5. **Evidence at this boundary.** Add app-level tests with in-memory `App` fakes for both sign-in
   branches, deactivated users, expiry, single-use recovery and concurrent consume; add proptest for
   email/locale normalization and password-policy boundaries. Introduce a `PasswordHasher::verify_or_dummy`
   port so the unknown-user path performs the *same* Argon2 work as the known path, replacing the
   `hash(email)` heuristic, and back the claim with a timing benchmark.
   *Risk:* more fakes to maintain; adding a port method touches the infra adapter and every test fake;
   timing parity is noisy in CI. *Mitigation:* assert equality of work by construction (same port call)
   rather than by a tight wall-clock threshold; keep REQ ids (040/041/042) and D-08 in test names —
   `tools/trace_check.py` counts declarations, not comments.

## Cross-cutting risks / sequencing

- Sequence 2 (errors) and 4 (secrets) are local and low-risk; do them first. 1 (one service + config)
  prepares 3 (transactional ports) but can land independently. 5 is continuous and gates all others.
- Items 1 and 4 can alter wire-visible behavior (error text, TTL defaults, DTO debug shape). Per
  AGENTS.md, record each as a DEVIATION in `docs/PARITY.md` and wait for approval before shipping.
- Preserve the anti-enumeration guarantees: identical response bytes for unknown/malformed addresses
  (`:223-232`) and for absent/used/expired recovery tokens (`:277-301`). Every refactor must keep these.
- Verification gate per step: `cargo fmt --check`, `clippy -D warnings`, unit + `tests/` integration,
  golden/contract suites, and the timing benchmark for item 5.

## Out of scope for this module

Cookie emission/parsing, the `FromRequestParts` extractor and principal typing live in
`crates/api/src/auth.rs` (wave-9, worker #121); Argon2 policy and the rate-limit counters (D-01) live
in infra/services. This proposal only restructures the accounts service and its observable contracts.
