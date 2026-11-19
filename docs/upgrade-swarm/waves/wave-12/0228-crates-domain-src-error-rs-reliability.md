# Worker #228 — `crates/domain/src/error.rs` — reliability lens — v0.4.0 proposal

## Scope and status

Read-only review of `crates/domain/src/error.rs` (145 lines) plus its direct consumers
`crates/app/src/error.rs`, `crates/api/src/error.rs`, `crates/api/src/rpc.rs`,
`crates/api/src/public.rs`, `crates/domain/src/run.rs`, and `Grep` of `DomainError::` call sites.
No code was changed. All claims below are CONFIRMED from those files unless marked GUESS.

## What the module does today

`DomainError` is a 19-variant enum of pure rule violations. Display deliberately hides the
payload of `InvalidEmail`, `InvalidCursor` (fixed safe sentence) but echoes `WeakPassword.reason`
and `InvalidName(String)` verbatim (lines 76-77). `Debug` is derived on every variant (line 8).
`impl std::error::Error for DomainError {}` supplies no `source()` (line 105). The only
classification helper is `is_invalid_transition()` (lines 119-123).
GUESS-free consumer facts: `AppError::Rule(#[from] DomainError)` uses `#[error(transparent)]`
(app/error.rs:12-13); `AppError::is_client_fault()` counts *every* `Rule(_)` as a client fault and
`is_retryable()` counts only `Unavailable` (app/error.rs:45-66); `From<AppError> for ApiError`
maps every `Rule(_)` to `bad_request()` / 400 (api/error.rs:191-195). `ApiError::explained()`
forwards a `String` verbatim into a 400 body (api/error.rs:87-99), and rpc.rs:3291 calls
`ApiError::explained(error.to_string())` on a `DomainError`. `InvalidName` is used as a generic
adapter-failure bucket: connector.rs, model_http.rs, oauth.rs, plugin.rs, mcp_http.rs,
messaging_http.rs and others construct it from `reason.to_string()` / `other.to_string()`.

## Failure modes

1. **Context loss.** Display drops the rejected value for email/cursor, so a log line carries no
   discriminator beyond the category; with no `source()` and no structured `code()`, callers cannot
   recover what failed. Worse, `InvalidName(String)` erases the originating error *type* into text
   at dozens of call sites, so a config problem, a provider refusal and a bad name are the same
   variant. The `WeakPassword | InvalidName` shared Display arm (line 77) further merges two
   distinct categories into one free-form string, making message-based triage impossible.
2. **Wrong classification.** The taxonomy lives entirely outside this module and collapses:
   every rule violation is a client fault, every rule violation is HTTP 400. `EmailAlreadyTaken`
   (409), `ArchivedBot` (409/403), `DelegationRefused` (403), `PinnedModelUnreachable` (503, a
   deployment fault) and `MessageTooLong`/`InvalidPageSize` (400) are indistinguishable to the
   mapper. `PinnedModelUnreachable` is the sharpest case: a dependency failure is reported to
   clients as "the request is not valid" and is never retried by `is_retryable()`.
3. **Secret leakage.** The derived `Debug` prints every payload, including the address and cursor
   that Display was written specifically to hide (`format!("{:?}", InvalidEmail(..))` leaks). Since
   `InvalidName` wraps arbitrary infra strings and its Display echoes them verbatim, any token,
   URL or provider message embedded in those strings can travel through `ApiError::explained()`
   into a client-visible 400. `DelegationRefused(reason)` is also echoed (line 99); its comment
   says "refusal-safe", but that safety is a convention at ~15 call sites, not enforced here.

## Concrete hardening for v0.4.0

- **H1 — redact at the type boundary.** Replace derived `Debug` with a manual impl that prints the
  variant name and safe fields only, rendering `InvalidEmail`, `InvalidCursor`, `WeakPassword` and
  `InvalidName` payloads as `[redacted]`. Keep Display unchanged where golden tests depend on it
  (`req_012_domain_error_renders_a_readable_message` asserts "completed" survives). Add a test
  asserting neither `to_string()` nor `format!("{:?}")` contains a sample address or cursor.
- **H2 — expose a stable taxonomy from this module.** Add `pub enum ErrorClass { Validation,
  Conflict, Forbidden, Unavailable }` and `DomainError::class()`, plus `is_retryable()` and
  `is_client_fault()` derived from it. Split the shared Display arm so `WeakPassword` and
  `InvalidName` have separate messages. Wire `app::AppError` and `api::ApiError` to the new class
  so `PinnedModelUnreachable` becomes 503/retryable and `EmailAlreadyTaken` becomes 409. Add an
  exhaustive matrix test over all variants asserting class × status × retryable, mirroring the
  existing exhaustive transition-table test in run.rs:441-453.
- **H3 — stop using `InvalidName` as the adapter catch-all.** Add a `DependencyRefused(String)`
  (or `AdapterFailed`) variant with a fixed safe Display and migrate connector/model/oauth/plugin
  call sites to it in a follow-up wave. Where a lower error exists, carry it as `#[source]` via
  `thiserror` so `Error::source()` is meaningful; leave the domain crate IO-free by accepting an
  already-classified string plus a static kind, not a boxed IO error.
- **H4 — make evolution safe.** Mark the enum `#[non_exhaustive]` so adding a rule variant is not a
  breaking change, and add constructors for every string-carrying variant so callers cannot build
  one with an unsafe payload (a single choke point for future redaction/validation).
- **H5 — regression tests.** Keep the existing `req_040` non-quoting test; add tests for
  Debug redaction, class mapping, and `source()` presence, with REQ ids in the test names per
  AGENTS.md.

## Risks and mitigations

- **Externally visible behavior change:** H2 changes status codes for some rule errors. This is a
  DEVIATION under AGENTS.md — record it in `docs/PARITY.md` and wait for approval before shipping.
  Mitigation: land H1/H3/H4 first (no wire change), H2 behind the approved parity entry.
- **Source break:** `#[non_exhaustive]` breaks exhaustive matches in downstream crates today.
  Mitigation: audit all matches (grep already shows consumers mostly match via helpers/`matches!`),
  add the attribute in the same minor version as the new constructors.
- **Churn:** migrating `InvalidName` touches ~100 construction sites across app/infra. Mitigation:
  do it wave-by-wave; H3 does not have to be atomic with H1/H2.
- **Over-redaction harms diagnosability:** removing values from Debug can hide the one detail an
  operator needs. Mitigation: the H2 `class()`/stable code gives a safe structured discriminator,
  and a deliberately verbose, `tracing`-gated diagnostic path can be designed in the app layer —
  never by leaking through Debug or the client envelope.
- **Display stability:** several tests and golden fixtures assert rendered text. Mitigation: keep
  all current Display strings byte-identical in v0.4.0; H2 changes only the classification API,
  not the message.
