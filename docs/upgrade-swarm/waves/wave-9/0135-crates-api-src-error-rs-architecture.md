# 0135 — crates/api/src/error.rs — architecture lens (v0.4.0)

- Target: `before-m5` (v0.3.0-rc5), module `crates/api/src/error.rs` (328 lines). Read-only review; no code edited.
- Related files read: `crates/app/src/error.rs` (`AppError`, 9 variants), `spec/03-api-contract.md` §3.1, `spec/openapi.yaml` (components.schemas.Error, lines 497-504), `docs/STATE.md` (REQ-027 notes), plus a usage scan of `crates/api/src` (`ApiError::new` ×22, `ApiError::explained` ×2).
- Spec anchors: §3.1 "one stable envelope … internal details never cross" ; REQ-027 (`COMPUTERS_UNAVAILABLE` 503, fail-closed), REQ-040 (envelope shape), REQ-052 (provider words), REQ-053/054 (503-internal, no-store), AGENTS "typed errors … one stable API error format, never leak internals".

## The shape today

Three concepts are cleanly separated and worth keeping: `ErrorCode` (8 wire codes + `status()`), `ErrorBody` (`{code, error}`, the two-field golden envelope), and `ApiError` (a struct with private `code`, `message: Cow<'static, str>`, `status`). `IntoResponse` is the single place that writes `cache-control: no-store` + JSON, and `From<AppError>` is the intended single app→HTTP mapping point. Five tests cover the status table, golden shape, uppercase serialization, the 503-but-INTERNAL case and no-store.

The architecture weakens in four places:

1. **Two sources of truth for status.** `ApiError.status` is stored, not derived. `new()` derives it from the code, but `unavailable()` (503 + code `INTERNAL`) and `computers_unavailable()` (503 + code `COMPUTERS_UNAVAILABLE`) hand-write a divergent pair. The struct can represent any code/status combination, including meaningless ones — the invariant lives in constructor discipline, not in the type.
2. **The taxonomy disagrees with the contract.** The wire enum has 8 codes; `spec/openapi.yaml` lists only 7 — `COMPUTERS_UNAVAILABLE` is missing even though REQ-027 makes it a real, documented response (CONFIRMED: `STATE.md` lines 1191/1647). `utoipa::ToSchema` generates from the Rust enum, so the generated document and the hand-written spec cannot both be right. No contract test ties them.
3. **Mapping is not actually central.** `AppError`'s rustdoc claims the api crate is "the only place that knows about HTTP status codes", yet ~22 ad-hoc `ApiError::new(ErrorCode::…, "literal")` calls and hand-rolled conversions sit in `rpc.rs`, `public.rs`, `authroutes.rs` and `automation.rs`; `rpc.rs:1636` even overrides `AppError::Rule` from `bad_request()` to `Conflict`, and `public.rs`/`rpc.rs` wrap infra errors via `ApiError::from(AppError::from(error))`. Messages are duplicated (`"That already exists."` appears in both `conflict()` and `From<AppError>`).
4. **One unbounded input path.** `explained(String)` (REQ-052) lets arbitrary third-party text into the envelope with no length/control-char check; the comment says it is "already bounded" but the bound is not visible here (GUESS — verify the provider adapter before relying on it).

## Concrete upgrades for v0.4.0

### 1. Make `ApiError` a typed enum; derive status from one match (highest value)
Replace the struct with an enum whose variants carry their message (borrowed `&'static str` or the owned escape hatch) and whose `status()`/`code()` are a single `match` over variants. Encode the two intentional divergences as named variants (`Unavailable` → code `INTERNAL` + 503; `ComputersUnavailable` → 503), so no other code/status pairing is representable. `ErrorBody` and the two-field wire shape stay exactly as-is.
Risks: (a) all ~22 call sites and the golden fixtures must keep identical status/code/message — this is a refactor, not a behavior change; (b) keep `Cow<'static, str>` semantics so static messages remain zero-alloc; (c) `#[derive(PartialEq, Eq)]` is used by tests — preserve it; (d) do not fold `INTERNAL` and `Unavailable` into one variant, REQ-053 depends on 500 vs 503 being distinguishable.

### 2. Reconcile the code set with `openapi.yaml` and add a contract test
Either add `COMPUTERS_UNAVAILABLE` to the spec's `Error.code` enum (preferred — it matches shipped REQ-027 behavior) or retire the code; record the choice in `docs/STATE.md` and, if it changes the public contract, `docs/PARITY.md`. Add a test that compares the utoipa-generated `Error` schema with `spec/openapi.yaml`, per the AGENTS contract-test rule.
Risks: (a) editing the spec's enum is externally visible — treat it as correcting stale documentation, not a silent API change; (b) generated schemas may also disagree on `required: [error]` and the missing `code` requirement — fix in the spec, not by weakening the test; (c) keep the test strict so future drift fails CI.

### 3. Enforce one transport-mapping point
Add typed `From` impls / an `IntoApiError` trait for `DomainError` and the infra error types callers currently wrap, and route all conversions through `error.rs`; delete duplicated literals by delegating `From<AppError>` to the named constructors. Keep a single documented override policy for the rare contextual case (`rpc.rs` Rule→Conflict).
Risks: (a) the `api` crate already depends on `app`/`domain`, so no cycle — but do not add an infra dependency just for a `From`; use a local trait if needed; (b) the Rule→Conflict override may be a real rule-specific decision — confirm with its REQ before folding it into `From<AppError>`; (c) flattening typed errors must not leak internals — only the safe message may survive.

### 4. Gate the owned-message escape hatch with a `SafeMessage` newtype
Make `explained()` (and any future owned-message constructor) go through `TryFrom<String>` that caps length and rejects control characters, so an unbounded provider string cannot enter the envelope. Keep the API surface small: one bounded constructor, all other messages static.
Risks: (a) truncation/cleaning changes user-visible text — if a golden covers a provider refusal, record a DEVIATION in `docs/PARITY.md`; (b) verify the existing "already bounded" claim first (GUESS) to avoid a second, conflicting cap; (c) `TryFrom` cannot be `const` — fine, `explained()` is not `const` today.

### 5. Extract one response-mapping helper; keep the envelope stable
Move the `(status, no-store, Json(ErrorBody))` assembly into a shared `error_response(status, body) -> Response` used by `IntoResponse`, so the header/body policy is defined once and can later be reused by non-error responses. If request ids are needed, attach them as a **header**, never as a body field.
Risks: (a) `req_040` asserts exactly two body fields — adding `requestId` to `ErrorBody` breaks the golden; (b) `no-store` must remain on every error, including 5xx; (c) a header addition is externally visible — check `docs/PARITY.md` before shipping.

## Explicit non-goals
No new error codes beyond REQ-027, no change to the two-field envelope, no leaking of driver/stack/tenant detail, no second error library (`thiserror` in `app`/`domain`, no `anyhow` here), no weakening of the existing REQ-040/053/054 tests, and no request-id body field. All wording changes are DEVIATION candidates requiring `docs/PARITY.md` and approval.
