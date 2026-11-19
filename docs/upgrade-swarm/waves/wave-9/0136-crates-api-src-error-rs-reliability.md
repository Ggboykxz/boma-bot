# 0136 — `crates/api/src/error.rs` — reliability lens (v0.4.0)

Worker #136 · module `crates/api/src/error.rs` · lens: reliability · READ-ONLY review of
`rebuild-workspace/.worktrees/before-m5` at v0.3.0-era source. Proposals only; no code changes here.
Claims: **CONFIRMED** = read in this tree this session; **GUESS** = inferred, needs a test. Scope:
`error.rs` (328 lines), its producer `crates/app/src/error.rs`, `DomainError::fmt` in
`crates/domain/src/error.rs`, and `ApiError::explained` / `ApiError::new` call sites in `rpc.rs`.
Spec: `spec/03-api-contract.md` §3.1; REQ-027, REQ-040, REQ-052, REQ-053, REQ-058.

## What the file already does right (keep, do not re-propose)

- Two-field `ErrorBody` envelope with a machine code and a safe message; golden test asserts exactly
  two fields, so internals cannot be added casually (`req_040_the_error_envelope_matches_the_golden_shape`).
- `no-store` on every `IntoResponse` — an error is never a cached auth failure (`req_054`).
- `Cow<'static, str>` messages borrow fixed strings; `#[must_use]` constructors; `ErrorCode::status()`
  is an exhaustive `match` (compiler-enforced).
- `unavailable()` deliberately returns code `INTERNAL` with status `503` so the failure stays
  opaque while telling the client to retry (`req_053`) (CONFIRMED).

## Failure modes

### F1. Secret / internal leakage through `explained(String)` (high)

`ApiError::explained(message: String)` (lines 92–99) emits the string **verbatim** as the `error`
field with `BAD_REQUEST`; the doc comment ("third party's own words, already bounded") is the only
guard — there is no length cap, no allowlist, no TLS (CONFIRMED). Its production call site
`rpc.rs:3291` passes `error.to_string()` for `AppError::Rule(_)` from `models_connect`. Several
`DomainError::fmt` arms interpolate caller- or store-supplied strings: `UnknownStatus(value)`,
`UnknownTrigger(value)`, `UnknownRole(value)`, `UnknownEventType(value)`,
`PinnedModelUnreachable(value)`, `UnknownSystemReason(value)`, `InvalidName(reason)`,
`WeakPassword { reason }`, `DelegationRefused(reason)` (CONFIRMED). Today those values are mostly
caller echoes, but any future Rule built from a stored row (a pinned model name, a role, a reason)
would cross the boundary. Two secondary leaks: (a) no cap on `String` length means an attacker can
amplify a small request into a large response (GUESS until measured); (b) `#[derive(Debug)]` on
`ApiError` would print an echoed provider key into any `tracing` debug log (CONFIRMED structure,
GUESS that a call site logs it).

### F2. Wrong status codes (high)

- `From<AppError>` maps **every** `AppError::Rule(_)` to `bad_request()` (400), including
  `EmailAlreadyTaken` (should read as a conflict), `ArchivedBot` (403/409), and
  `PinnedModelUnreachable` (a deployment problem, not a caller mistake) (CONFIRMED at lines 193–194
  and in `DomainError`). The same `Rule` is mapped to `CONFLICT` (409) at `rpc.rs:1636-1639` — the
  same producer error gets two different statuses depending on the call site (CONFIRMED).
- `unavailable()` is `code: INTERNAL` + `status: 503`: a client branching on `code` sees a fatal
  500-class value while HTTP advertises "retry" — contradictory retry signals (CONFIRMED, and
  asserted as intended by `req_053`). The spec's code list has no `UNAVAILABLE`, so adding one is a
  DEVIATION, not a free fix (CONFIRMED from §3.1).
- `StdioDisabled` → 400 and `RateLimited` → 429 with code `TOO_MANY_REQUESTS`; the former is a
  deployment capability, not malformed input (CONFIRMED; severity low).
- Tenant isolation relies on callers choosing `AppError::NotFound`; `AppError::Forbidden` exists and
  maps to 403. Nothing in this module forbids a future caller from using `Forbidden` for a
  cross-tenant lookup, which §3.1 explicitly forbids (CONFIRMED convention, GUESS that a drift exists).

### F3. Unmapped variants and invariant drift (medium)

- `ApiError.status` is a stored field that `new()` derives from the code but `unavailable()`,
  `computers_unavailable()`, and `explained()` set by hand. No assertion ties it back to
  `code.status()`; a future constructor could pair `INTERNAL` with a cacheable or 2xx status and
  every test would still pass (CONFIRMED).
- `req_040_every_error_code_has_one_status` checks a **literal list** of eight pairs, not enum
  coverage. Adding a ninth `ErrorCode` forces `status()` to compile but leaves this test green even
  with a wrong mapping (CONFIRMED).
- `IntoResponse` hardcodes `no-store`, but axum/tower's own rejections (413 body limit, 405, default
  panics) never pass through `ApiError`, so they bypass the envelope and the cache rule (CONFIRMED
  by construction; whether they leak internals is GUESS).

## Concrete hardening (v0.4.0)

- **P1 — make `explained` a bounded, explicit type.** Rename to `explained_reason(&'static str)` for
  static provider text; for the dynamic case add `explained_caller(String)` that (a) caps at e.g.
  256 bytes by truncation at a char boundary, (b) strips control characters, (c) documents that the
  value must originate from the caller's own input. Replace `rpc.rs:3275` with the static form.
- **P2 — stop echoing `DomainError` verbatim.** At `rpc.rs:3291`, map `Rule` to `ApiError::bad_request()`
  (the existing safe default) and log the domain detail server-side with `tracing` (no message body).
  If provider reason text is contractually required (REQ-052), pass only an allowlisted reason enum,
  never `to_string()`.
- **P3 — centralise status mapping.** Add `ApiError::from_rule(&DomainError)` (or extend
  `From<AppError>`) with an explicit table: `EmailAlreadyTaken`/`ArchivedBot` → `CONFLICT`,
  `PinnedModelUnreachable`/`Unavailable` → retryable. Replace the ad-hoc conflict at `rpc.rs:1636`.
- **P4 — enforce the code/status invariant.** Make `status` private to the constructors and add a
  `debug_assert_eq!(self.status, expected_status(self.code))`, where `expected_status` encodes the
  two sanctioned exceptions (`unavailable()` 503, `computers_unavailable()` 503). Add a test that
  iterates every `ApiError` constructor and asserts the pairs, so a new variant cannot drift.
- **P5 — close the test coverage hole.** Drive `req_040_every_error_code_has_one_status` from a
  exhaustive source (a `const` table with a compile-time length check, or proptest over `ErrorCode`)
  so adding a variant fails the test until mapped.
- **P6 — make the retry signal consistent.** Keep `code: INTERNAL` for opacity (spec has no
  `UNAVAILABLE` code) but emit `Retry-After` on 503 responses and document in `spec/03-api-contract.md`
  that clients must branch on HTTP status, not on `code`, for retry. Any new code is a DEVIATION.
- **P7 — add a correlation header.** Put the request id in a response header on `INTERNAL`/503
  errors so a client can quote it to support without growing the two-field body (keeps the golden).
- **P8 — bound the envelope at the router.** Add a fallback `IntoResponse` layer for tower rejections
  that maps them into `ApiError`, so 413/405/500 also carry `no-store` and the safe envelope.

## Risks / trade-offs

- R1. P2 removes provider reason text from one error response; if REQ-052's golden asserts it, this
  is an externally visible change — record in `docs/PARITY.md` as a DEVIATION and wait for approval.
- R2. P3 changes statuses for `EmailAlreadyTaken`/`ArchivedBot`; clients branching on 400 may break.
  Ship behind the v0.4.0 contract bump and update `spec/openapi.yaml` + contract tests together.
- R3. P4's `debug_assert` is stripped in release; keep the constructor table authoritative, not the
  assert, and test it in debug CI with `-D warnings`.
- R4. P1 truncation can cut a multi-byte reason; truncate on `char_indices` and add a property test.
- R5. P7/P8 touch response shape/headers beyond the module; they need a contract-test update and a
  golden review, otherwise they silently violate the "envelope never grows" rule.

## Explicit non-goals / rejected options

- Do not add fields to `ErrorBody` (golden asserts two fields); correlation goes in a header.
- Do not add an `UNAVAILABLE` machine code in v0.4.0 without an approved DEVIATION — §3.1 fixes the list.
- Do not weaken `no-store`, `deny_unknown_fields`, or any gate; hardening here is additive.

## Suggested rollout

Wave A (low risk, no wire change): P1, P2 (log instead of echo), P4, P5, R4 property test.
Wave B (contract change, needs approval): P3, P6, P7, P8 with `docs/PARITY.md` entries and refreshed
OpenAPI/golden/contract tests. Re-run `cargo fmt --check`, clippy `-D warnings`, and the REQ-040/052/053
tests after each step. Line refs are against `.worktrees/before-m5`; nothing was executed in this pass.
