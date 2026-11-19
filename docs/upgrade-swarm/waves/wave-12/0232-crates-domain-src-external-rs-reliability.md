# 0232 — `crates/domain/src/external.rs` — reliability lens — v0.4.0 proposal

Worker #232 · module `crates/domain/src/external.rs` (39 lines) · lens: reliability · READ-ONLY review
of `rebuild-workspace/.worktrees/before-m5` (v0.3.0-rc5). Proposal only; no code changed here. Spec
anchor: REQ-059 (external agents over standard protocols). CONFIRMED = read this session; runtime
outcomes = GUESS.

## Current behavior (read, not run)

- Two plain public-data structs, `ExternalAgent` and `NewExternalAgent`: ids are typed newtypes
  (`BotId`, `SpaceId`) but `base_url`, `agent_name`, `secret_name`, `created_at`, `updated_at` are raw
  `String`/`Option<String>`. No constructor, no `validate_*`, no `TryFrom`, no invariant method.
  CONFIRMED.
- This is the **only** domain module with zero validation: `mcp`, `routine`, `ticket`, `bot`, `skill`
  etc. all expose `validate_*` or a fallible `new`/`parse` returning `DomainError`. CONFIRMED.
- URL/scheme/secret checks live in the app layer (`usecases.rs:9506-9530` `external_agent_base_url_must_parse`,
  `:9560-9567` secret reveal) and run only on the `connect` use case. Other construction paths bypass
  them: infra `into_domain_row` (`stores/external_agents.rs:39-49`) and testkit `MemoryExternalAgentStore`
  (`memory.rs:8804`) build `ExternalAgent` directly. CONFIRMED.
- Store write is a blind upsert (`0054_external_agents.sql:61-68`): `created_at` preserved, `updated_at`
  replaced, no version/idempotency column. CONFIRMED.
- Worker reads config fresh every turn (`worker.rs:623-649`); a failed secret reveal collapses to an
  empty bearer (`:637-647`) and the run still starts. CONFIRMED.

## Failure modes

1. **Invalid state (CONFIRMED).** Any caller can build a config with an empty/whitespace `base_url`,
   a malformed URL, an embedded credential (`https://user:pass@…`), a metadata endpoint, an empty
   `secret_name`, or non-RFC3339 timestamps. Only the app's `connect` path rejects these; a restored
   row, a memory/test fixture, or a future write path can inject a record that later makes the worker
   dial an attacker-controlled or unusable address. Because validation is not a domain rule, a proptest
   cannot assert "every `ExternalAgent` is well-formed" — the type admits corrupt states, contrary to
   the AGENTS.md "make invalid states unrepresentable" rule.
2. **Attach/connect failures (CONFIRMED shape, GUESS runtime).** Validation errors are `DomainError::InvalidName`
   smuggled through `AppError::Rule`, so callers get generic "invalid name" text for a URL or secret
   fault; there is no typed `InvalidExecutorUrl`/`MissingBearer` to branch on. The secret is checked at
   connect time but can be rotated/deleted later; at use time the reveal error is swallowed to an empty
   bearer, so the executor call fails deep in the turn with an opaque 401 rather than a clear "credential
   missing" — and `external_agent_status` keeps reporting `connected: true` (`usecases.rs:9469-9476`)
   because it never re-validates the secret or the URL.
3. **Session/config drift (CONFIRMED).** There is no generation, revision, or session id on the record.
   `connected_at` is just `updated_at` as a free string — no parse, no monotonic comparison, so a
   clock skew or backdated write cannot be detected. The worker re-reads config per turn, so a config
   replaced between turns (another tab, an operator fix, a re-connect) silently switches the executor
   mid-conversation with no binding to the session that was started. Concurrent connects are last-write-wins
   with no idempotency key, so a retried `connect` can clobber a newer config. Cross-tenant safety rests
   entirely on RLS + query filters; the domain type carries a `space_id` that is never checked against
   the record's own bot at construction.

## Concrete hardening (v0.4.0), with risks

- **A. Domain-level validators + fallible constructors.** Add `validate_external_base_url(url: &str)` to
  `external.rs` (reuse the `mcp.rs:174` `validate_mcp_server_url` pattern: parse, https-only outside
  loopback, reject credentials and metadata hosts), plus `validate_secret_name`, timestamp parse checks,
  and `NewExternalAgent::try_new(...) -> Result<Self, DomainError>` / `ExternalAgent::try_from_row(...)`.
  Move `external_agent_base_url_must_parse` off the app layer onto this rule so there is one owner.
  *Risk:* narrowing accepted inputs is externally visible if a legacy/restored row carries a now-invalid
  URL — audit `0054` data and decide reject-vs-repair before shipping; a DEVIATION entry may be needed.
- **B. Typed error variants.** Add `DomainError::InvalidExecutorUrl(String)` and `MissingExecutorCredential`
  (safe, value-free) so the API can classify 400 vs credential faults instead of "invalid name". *Risk:*
  touches the shared enum and its `Display`/mapping in `error.rs`; keep messages free of the raw URL
  (credentials) to avoid leaking into client error text and logs.
- **C. Make invalid states unrepresentable.** Consider `BaseUrl` and `SecretName` newtypes validated at
  construction, and a parsed `Timestamp` newtype in place of raw strings. *Risk:* ripples into wire DTOs,
  infra row mapping and testkit; high churn for a pure module — sequence after A and gate on an ADR.
- **D. Config generation / session binding.** Add a `generation: u64` (or version) to the record, bumped
  on every connect, and have the worker capture it when a session starts and re-check before each turn
  (`worker.rs:623`); on mismatch, fail/restart the turn instead of silently switching executors. *Risk:*
  needs a migration and a store/app/worker change, and changes runtime behavior (a reconnect now ends an
  in-flight session) — likely a DEVIATION requiring owner approval and a spec/STATE decision.
- **E. Fail loud on credential loss.** Stop swallowing the reveal error at `worker.rs:637-647`: if
  `secret_name` is set but cannot be revealed, fail the turn with `MissingExecutorCredential` and have
  `external_agent_status` surface a degraded flag. *Risk:* turns that previously "worked" with an empty
  bearer will now fail visibly — intended, but verify no deployment relies on an unset secret for an
  anonymous executor (GUESS).
- **F. Idempotent connect.** Carry a caller-supplied idempotency key / expected-generation on `connect`
  so a retry cannot clobber a newer config. *Risk:* wire/port signature change; check contract tests and
  `docs/PARITY.md`.

## Risks / non-goals

- **R1** Domain stays pure: validators are shape-only, no network/IO, no dependency on `reqwest`/`url`
  beyond what the crate already permits — keep the domain IO-free.
- **R2** No secret *value* ever enters the type, logs, or errors; only names, per the REQ-059 contract.
- **R3** D/E/F alter externally visible behavior (accepted inputs, runtime failure, wire shape) — route
  through `docs/STATE.md` + `docs/PARITY.md` and wait for owner approval before shipping.

## Success criteria / tests (REQ-named, domain unit + proptest)

- `REQ-059_validate_external_base_url_accepts_https_and_loopback_http`.
- `REQ-059_validate_external_base_url_refuses_credentials_metadata_and_plaintext`.
- `REQ-059_external_agent_constructor_rejects_blank_url_and_blank_secret_name`.
- `REQ-059_external_agent_row_rejects_unparseable_timestamps` (proptest over arbitrary strings).
- `REQ-059_every_constructed_external_agent_satisfies_its_invariants` (proptest).
- `REQ-059_generation_mismatch_is_detected_before_the_worker_switches_executor`.

## Bottom line

The module is correct on the happy path but is a validation vacuum: invariants are enforced in one app
use case, not in the domain, so alternate write paths can persist unusable configs; credential loss and
config/session swaps are silent. A–C are local and reversible; D–F genuinely harden drift but change
visible behavior and need owner sign-off.
