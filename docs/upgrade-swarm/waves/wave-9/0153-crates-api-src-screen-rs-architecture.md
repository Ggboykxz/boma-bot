# v0.4.0 architecture proposal — `crates/api/src/screen.rs` (screen proxy / route helper)

Worker #153 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no code changed).

Status of claims: every "current state" statement below is CONFIRMED by direct reads of `screen.rs`,
`router.rs`, `error.rs`, `realtime.rs`, `openapi.rs`, `app/src/usecases.rs` and `spec/openapi.yaml`
plus `crates/api/tests/screen.rs` in this worktree. Forward-looking items are proposals (GUESS
until implemented, tested and measured).

## Current structure (CONFIRMED)

- `screen.rs` is a single flat module: module doc (`:1-8`), `SCREEN_TARGET = "/screen"` (`:26`),
  `ScreenTargetRequest { path }` (`:29-33`), `ScreenFrameEvent` (`:36-43`, `Serialize` only), the
  `utoipa::path` annotation (`:51-63`), the handler `screen_target` (`:64-101`), a private
  hand-rolled `bearer_token` (`:103-110`) and one unit test (`:112-132`). No module-level
  `route()`/`routes()` helper exists.
- The route is composed *elsewhere*: `router.rs:57-59` hard-codes the full path string, the `post`
  method and `.route_layer(DefaultBodyLimit::max(16 * 1024))`. The same `16 * 1024` is repeated as
  a manual `body.len()` check in the handler (`screen.rs:72`) — two independent copies of one cap.
  By contrast `rpc::router()` and `automation::router()` own their route tables (`router.rs:125,130`).
- The SSE body is built by string interpolation (`screen.rs:88-91`): two `format!`-embedded JSON
  objects with field names `screenGeneration`/`pngBase64` spelled as literals, duplicating the
  names derived by `ScreenFrameEvent` (`:37,40,42`). There is no typed `meta` event. `realtime.rs`
  instead emits SSE through `axum::response::sse::Event::default().event().json_data()` (`:138-144`).
- Error policy is inconsistent: `resolve_screen_capability(...).await?` (`:80`) uses
  `From<AppError>` (`error.rs:191-214`), so a store failure becomes `ApiError::unavailable()` (503,
  `error.rs:163-170`); `screen_frame(...).map_err(|_| ApiError::forbidden())?` (`:84-86`) flattens
  every backend failure to 403. The rustdoc (`:49-50`) advertises 400/503 while `spec/openapi.yaml:373-376`
  lists only 200/403/413, and an inline comment (`:69-70`) concedes the mismatch.
- The capability is an opaque `&str` from header to usecase (`:71,80`); the seal/hash/constant-time
  compare lives in `usecases.rs:6306-6334` (good layering), but the API layer neither types nor
  hides the secret. `ScreenTargetRequest` has no `deny_unknown_fields`.
- OpenAPI registers `ScreenTargetRequest` (`openapi.rs:39`) but not `ScreenFrameEvent`; the 200
  response is `body = String` (`screen.rs:58`) and `spec/openapi.yaml:374` has no content schema.

## Guiding principle for v0.4.0

Make this module own its route: path literal, method, body cap, DTOs and SSE encoding in one place,
with one typed secret and one explicit error policy. Preserve every wire byte — path, status, headers,
cap bytes — or record the delta as a DEVIATION in `docs/PARITY.md` before shipping (AGENTS.md).

## Upgrades (5 concrete items)

1. **Give the module a route helper.** Add `pub const SCREEN_TARGET_PATH: &str = "/api/internal/screen-target"`,
   `pub const SCREEN_TARGET_MAX_BYTES: usize = 16 * 1024` and `pub fn route() -> Router<AppState>`
   (or `routes()`) that registers the `post` handler and its `DefaultBodyLimit` cap; `router.rs`
   becomes `.merge(screen::route())`. This removes the path string from `router.rs`, co-locates the
   cap with its manual check, and matches the `rpc`/`automation` module pattern.
   *Risk:* a merge/registration change could duplicate or drop the route, and double-apply the body
   cap. *Mitigation:* keep the full literal path and the `16 * 1024` value verbatim, register exactly
   once, gate on the OpenAPI contract test and the cap boundary test ("limit-1 accepted, limit+1
   rejected"). Coordinate with worker #149's body-cap policy table so the two proposals agree.

2. **Encode SSE from typed DTOs, once.** Derive `ToSchema` on `ScreenFrameEvent`, add a typed meta
   event, and replace the `format!` body with `axum::response::sse::Event::default().event(..).json_data(..)`
   as in `realtime.rs`, ideally via one shared SSE helper in the api crate.
   *Risk:* `json_data` changes whitespace and field order; `crates/api/tests/screen.rs:224-225`
   asserts substrings (`event: frame`, `screenGeneration": 1`). *Mitigation:* pin the exact wire
   frame as a golden test before the switch, assert byte-for-byte equality after, and add a unit
   test proving the DTO field names cannot drift from the emitted JSON.

3. **State one explicit error policy.** Decide and document which failures stay the uniform 403
   (security: unknown/stale capability — keep) and which become 503 (`AppError::Unavailable` from
   either usecase, via `?`/`From<AppError>`). Replace the mixed `?`/`map_err` with that single policy
   and reconcile the rustdoc (`:49-50,69-70`) with the implemented statuses.
   *Risk:* distinguishing "no backend" from "bad capability" can become an oracle, and any 403→503
   shift is wire-visible. *Mitigation:* keep the uniform 403 if parity demands it and add `tracing`
   at debug level for the internal reason (never the token); if 503 is chosen, record a DEVIATION
   and wait for approval. Either way add tests for store-down, frame-command-down and stale-token.

4. **Type and seal the bearer at the boundary.** Replace private `bearer_token` with a shared
   extractor returning a `BearerCapability(String)` newtype (wrap in `secrecy::SecretString`,
   `#[derive(Debug)]` redacted), whose rejection implements `IntoResponse` as `ApiError::forbidden()`
   so the envelope stays identical.
   *Risk:* extractor rejection shapes and handler signatures change across the crate; the secret
   can leak if logged. *Mitigation:* keep the exact 403 `ErrorBody`, add a test asserting the
   rejection bytes equal today's, and ban the newtype from `Debug`-carrying logs. Coordinate with
   worker #121's auth/extractor work before touching shared `auth.rs`.

5. **Make the request DTO and OpenAPI match the code.** Add `#[serde(deny_unknown_fields)]` to
   `ScreenTargetRequest`, consider a one-value target type so only `/screen` is representable, and
   expose the SSE payload schema in `utoipa` instead of `body = String`.
   *Risk:* `spec/openapi.yaml:374` declares no 200 content schema, so adding one breaks the contract
   test and `docs/PARITY.md`; `deny_unknown_fields` changes only internal validation (still 403).
   *Mitigation:* update `spec/openapi.yaml`, `docs/STATE.md` and `spec/EXPORT.md` provenance in the
   same change, or keep `body = String` and register the DTO for internal reuse only; run the
   contract and golden suites both ways.

## Cross-cutting risks / sequencing

- Sequence 1 → 2 → 3 → 4 → 5: own the route and pin the wire bytes first, then unify the error
  policy and secret typing, then touch the schema. Each step is independently revertible.
- No new dependency is required for 1-3; items 4-5 need `secrecy`/`utoipa` (already workspace deps).
- Keep REQ-030 in every new test name (`tools/trace_check.py` counts declarations, not comments).
- Gate each step with `cargo fmt --check`, `clippy -D warnings`, unit + `tests/` integration and the
  OpenAPI contract/golden suites; verify with commands, mark claims CONFIRMED only after running them.

## Out of scope for this module

Capability minting/rotation, generation invalidation and the seal/constant-time compare
(`usecases.rs:6235-6366`), the global middleware stack (`router.rs`), and the `openapi.rs` tag
registry (which declares `ops` twice at `:44-45`) belong to other workers; this proposal only
restructures the screen route helper and its local helpers/DTOs.
