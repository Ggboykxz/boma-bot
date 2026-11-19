# v0.4.0 architecture proposal — `crates/api/src/rpc.rs` (RPC dispatch / method registry)

Worker #151 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no code changed).
Only file written by this worker is this proposal.

Status of claims: "current state" statements are CONFIRMED by direct reads of `rpc.rs`
(all 4535 lines), `router.rs`, `error.rs`, `openapi.rs`, `state.rs` and
`crates/api/tests/rpc_surface.rs` in this worktree, plus a worktree-wide symbol search for
`procedures()`. Forward-looking items are proposals (GUESS until implemented and tested).

## Current structure (CONFIRMED)

- `rpc.rs` is four layers in one 4535-line file: `namespaces` string constants (69-128), the
  `procedures()` list of ~125 `(namespace, procedure)` pairs (130-264), the router + dispatch
  (266-730), and ~100 handler fns with ~79 request structs (733-4534). `#[allow(too_many_lines)]`
  is used three times to suppress the symptom.
- **Two parallel sources of truth for the surface.** `procedures()` is documented as "the
  contract: a procedure that is not here is not reachable" (130-135), but a worktree-wide search
  finds **no caller** — it is dead metadata, so the list can silently diverge from the real
  dispatch. The authoritative table is the `match (namespace.as_str(), procedure.as_str())`
  in `rpc_inner` (339-724), ending in `_ => Err(ApiError::not_found())` (723).
- Arms are highly uniform: `handler(state, auth, body).await.map(|reply| Json(reply).into_response())`.
  Four outliers: `threads.subscribe` delegates to `realtime::subscribe` and already returns a
  `Response` (346); `memory.provider` (461-464) and `computer.provider` (498-500) are inline
  static replies; the catch-all is the fourth.
- Auth is resolved once at the top of `rpc_inner` (318-321) then threaded explicitly as
  `auth: AuthContext` into every handler. Feature gating is an ad-hoc if-block before the match:
  boards/tickets (325-329), messaging (331-333), billing (335-337) all answer uniform 404 when
  disabled, by comparing namespace *strings*.
- The artifacts body-limit exception is a **second, hand-written route** in `router()`
  (270-275) with its own duplicated literal `"artifacts"` / `"create"` and a cap derived from
  `boma_domain::ARTIFACT_MAX_BYTES`. It is not derived from the procedure registry (D-07).
- Body parsing is uniform and strict: `parse<T>` (1846-1851) uses `serde_json::from_slice` and
  `deny_unknown_fields` (86 structs), so unknown fields become `BAD_REQUEST` per spec §3.1.
- Many request structs are structurally identical but separate (`run_id`-only
  cancel/retry/pause/resume; `ThreadHeadRequest` shared by three threads arms; `ComputerRequest`,
  `McpServersServerRequest`, `SecretsBotRequest`, …). Some role/policy checks are duplicated
  inline in handlers (e.g. `DestinationsCreate/Delete` map `AppError::Forbidden` to bespoke
  messages, 4464-4474 and 4500-4507; admin procedures repeat space-resolution logic).
- `dispatch` and `rpc_inner` exist only to share code between the generic route and the special
  artifacts route (280-317). `openapi.rs:7-10` deliberately excludes `/rpc/{namespace}/{procedure}`
  from the generated document, so the registry is the only machine-readable surface description.

## Guiding principle for v0.4.0

One declarative table should be the single source of truth for the RPC surface: name, handler,
feature gate, body-limit class and reply kind. Dispatch should be generated from it, so a
procedure cannot exist in one list and be missing from the other. Move handler bodies into
per-namespace submodules without changing a single wire byte — procedure names, request fields,
status codes and error messages stay as the golden fixtures expect.

## Upgrades (3-5 concrete items, each with a risk)

1. **Registry as the single source of truth.** Replace `procedures()` and the giant match with
   one static `ProcedureSpec` table keyed by `(&'static str, &'static str)` carrying a handler
   function pointer, a `Feature` gate, a `BodyClass` and a reply kind. Derive `procedures()` from
   that table and generate dispatch with a macro that matches on a `#[derive]`d `Procedure` enum,
   keeping arms monomorphised (no boxing on the hot path). Add a test asserting the table equals
   the procedure set declared in `spec/03-api-contract.md`.
   *Risk:* an `async fn`-pointer registry needs boxed futures or a macro; boxing adds an
   allocation per call. *Mitigation:* prefer macro-generated enum dispatch, benchmark the hot
   path, and keep `_ => not_found` behavior byte-identical (404, uniform envelope).

2. **Declarative gates and body-limit classes.** Move the boards/messaging/billing checks
   (325-337) and the artifacts cap (270-275) into registry metadata. The router derives its few
   cap-class routes from the table instead of hard-coding a second namespace/procedure pair, and
   dispatch checks `spec.gate` against `state.app().services` before calling the handler.
   *Risk:* `DefaultBodyLimit` is a layer, so a per-procedure cap must be applied before body
   buffering; a wrong cap class could regress D-07 (oversized artifact) or reject a golden
   upload. *Mitigation:* keep a small fixed set of classes, test the artifact cap and the normal
   64 KiB cap against `crates/api/tests/rpc_surface.rs`, and preserve 404-not-403 for disabled
   surfaces.

3. **Split by namespace family.** Keep `rpc.rs` as router + registry + shared `parse`/reply
   helpers; move handlers and request structs into `threads.rs`, `bots.rs`, `computer.rs`,
   `admin.rs`, `notifications.rs`, … under `crates/api/src/rpc/`. Re-export `namespaces` and
   `procedures()` from the crate root so downstream code and docs keep compiling.
   *Risk:* large mechanical diff with many private helpers (`mcp_view`, `artifact_view`,
   `check_bot_live`) and shared imports; a dropped `deny_unknown_fields` or changed serde rename
   would be a silent wire break. *Mitigation:* one namespace per commit, run golden + contract
   suites after each, allow zero behavior diff, and drop the now-unneeded `too_many_lines`
   allows only after the move.

4. **Uniform handler contract and co-located policy.** Introduce one internal handler signature
   (e.g. a `Call { state, auth, body, headers }` plus a small `Reply` enum covering typed-JSON
   and raw-`Response` cases) so subscribe and the two static replies stop being special arms.
   Attach role/policy metadata next to each procedure and stop re-mapping `Forbidden` by hand in
   individual handlers. De-duplicate the `run_id`-only / `bot_id`-only request structs where the
   wire shape is genuinely identical.
   *Risk:* routing replies through `serde_json::Value` adds cost and can change key order/format;
   an `async fn` trait needs RPITIT or boxing; re-mapping errors could change the stable envelope.
   *Mitigation:* keep typed `Json<T>` via an associated reply type, verify `error.rs` envelope
   goldens and every REQ-named test, and record any externally visible difference as a DEVIATION
   in `docs/PARITY.md` before shipping.

5. **Per-procedure observability.** Wrap dispatch in a `tracing::Instrument` span carrying
   `namespace`, `procedure` (from the registry, never raw path input), request id from
   `correlation::log_http`, outcome code and latency; emit counters keyed by procedure and error
   code. This makes hot/failing procedures visible and gives rate-limiting a per-procedure hook.
   *Risk:* ~125 procedures × high QPS means cardinality and overhead; logging a body would leak
   `botSecrets` values and MCP client secrets (REQ-046/REQ-033). *Mitigation:* log only registry
   strings and coarse outcome classes, sample spans, never log bodies, and benchmark.

## Cross-cutting risks / sequencing

- Sequence: 1 → 2 (registry first, then its metadata) → 3 (split only after the table is stable)
  → 4 (uniform contract) → 5 (telemetry). Items 3 and 5 are otherwise independent.
- **Parity:** all five are internal restructurings; any change to a 404/400 status, error message,
  request field or reply shape is an externally visible DEVIATION requiring `docs/PARITY.md` and
  maintainer approval (AGENTS.md). Target: zero deviations, golden fixtures replay unchanged.
- Keep REQ ids in test names — `tools/trace_check.py` counts declarations, not comments.
- Verification gate per step: `cargo fmt --check`, `clippy -D warnings`, unit + integration
  (`crates/api/tests/rpc_surface.rs`), golden replay and the OpenAPI contract test.

## Out of scope for this module

Authentication internals (worker #121 / `auth.rs`), provider-specific authorization rules and
usecase logic (`crates/app`) are out of scope; this proposal only restructures how RPC procedures
are declared, gated, dispatched and observed.
