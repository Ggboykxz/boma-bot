# v0.4.0 architecture proposal — `crates/api/src/router.rs` (route composition / middleware layers)

Worker #149 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no code changed).

Status of claims: all "current state" statements below are CONFIRMED by direct reads of
`router.rs`, `rpc.rs` and `automation.rs` in this worktree. Forward-looking items are proposals
(GUESS until implemented, tested and measured).

## Current structure (CONFIRMED)

- `router(state: AppState) -> Router` (`router.rs:24-159`) is the single composition root: it builds
  a ~98-line flat chain of 30+ `.route()` calls in `main`, merges `rpc::router()` (`:125`), attaches
  `web_fallback`, applies state + all global layers on `root` (`:131-155`), then nests the already
  finalised automation router at `/api/v1/automation` (`:156-158`). The whole table carries
  `#[allow(clippy::too_many_lines)]` (`:23`).
- Global layers, in registration order on `root`: `DefaultBodyLimit(http.body_limit_bytes())`,
  `TimeoutLayer` -> 408 (`http.request_timeout()`), three `SetResponseHeaderLayer::if_not_present`
  (`X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`, `Cache-Control: no-store`),
  and `correlation::log_http` via `from_fn_with_state` (`:135-155`). Because tower/axum apply later
  layers outermost, correlation wraps the headers, which wrap the timeout, which wraps the body
  cap — a load-bearing order that no test currently pins down.
- Eight routes override the body cap with inline `.route_layer(DefaultBodyLimit::max(...))` literals
  (`:58,65,73,77,81,89,93,105`), ranging from 16 KiB to `33 * 1024 * 1024` / `25 * 1024 * 1024 + 1024`.
- State is cloned twice: `http = state.http().clone()` (`:25`) for config values, `middleware_state =
  state.clone()` (`:26`) solely for `log_http`, while `with_state(state)` consumes the original.
- The automation surface duplicates the global middleware stack in `automation::router()`
  (`automation.rs:484-524`): same three headers + `log_http` + a 64 KiB body cap, but its own timeout
  (`wait_timeout_seconds + 5`) and state applied inside. This is a deliberate deviation (long-poll
  `wait=true` must sit outside the global timeout) implemented by copy-paste, with a long doc comment
  as the only guard against drift.
- `web_fallback` + `resolve_static` + `resolve_prefix_fallback` + `deny_list` + `content_type`
  (`:173-317`) are ~145 lines of static-export logic living in the router file, with the API fallback
  contract (REQ-040/055, ADR-013) encoded as string predicates that must stay in sync with the route
  table.

## Guiding principle for v0.4.0

One composition root, one shared middleware builder, and route groups that are declarative rather
than one long builder chain. Preserve every public path, status code, header and body cap byte-for-byte:
`router.rs` is where parity is easiest to break invisibly. Any wire-visible change is a DEVIATION to
record in `docs/PARITY.md` before shipping (AGENTS.md).

## Upgrades (5 concrete items)

1. **Extract one parameterised `common_layers` builder.** Add e.g. `middleware.rs` with
   `fn apply_common_layers(router: Router<AppState>, cfg: LayerConfig) -> Router<AppState>` taking
   `{ body_limit, timeout, header_mode }`; have both `root` and `automation::router()` call it so the
   three security headers and `log_http` are defined exactly once, with automation supplying its
   longer timeout and "no global timeout" semantics via config.
   *Risk:* collapsing the two stacks can silently drag long-poll automation under the short global
   timeout or change layer order. *Mitigation:* keep the deviation explicit in `LayerConfig`, add a
   test that an automation `wait=true` route is not cut off by the global timeout, and a header test
   proving each header appears once (not twice) on both surfaces.

2. **Split the flat route table into named sub-routers by surface.** Build `probes()`, `auth()`,
   `public()`, `scim()`, `assets()` sub-routers (or one `route_groups.rs`) and compose them with
   `.merge()` at their existing literal prefixes; drop the `too_many_lines` allow. Keep
   `rpc::router()` and the nested automation router as they are — they are already grouped.
   *Risk:* `.nest()` changes path matching and can introduce/remove trailing-slash behaviour, shift
   fallback resolution and the `deny_list`, and alter `spec/openapi.yaml`. *Mitigation:* use
   `.merge()` with full literal paths (no new prefixes), define paths as shared constants consumed by
   both `deny_list` and the table, and gate on the contract test against `spec/openapi.yaml`.

3. **Turn the eight inline body caps into a typed policy table.** One `fn body_cap(bytes) -> ...`
   helper plus a `BODY_CAPS: &[(&str, usize)]`-style table (or per-route constants) replaces magic
   numbers and expressions; the D-07 comment attaches to the named `artifact`/`email_inbound`/
   `account_import` limits instead of being repeated.
   *Risk:* `.route_layer` acts only on matched routes and composes with the global `DefaultBodyLimit`
   in a non-obvious way; a wrong scope could reject valid large uploads (import, inbound email) or
   accept oversized payloads — a security regression. *Mitigation:* one unit test per cap asserting
   "limit-1 accepted, limit+1 rejected" on the real router, keep the existing cap values verbatim,
   and benchmark no hot-path cost.

4. **Single state injection point; document and test layer order.** Convert internal builders to
   `Router<AppState>`, drop the redundant `middleware_state` clone, and call `.with_state(state)` once
   at the outermost boundary. Add an integration test that asserts, on a normal route, on the 404
   fallback and on an automation route: all three headers present exactly once, a correlation id
   echoed, and a timeout producing 408 (not 500).
   *Risk:* moving `with_state` changes where `State<AppState>` is available to `web_fallback` and
   `log_http`; a mis-ordered `.layer()` can swallow the 408 or double-add headers. *Mitigation:* keep
   `with_state` last, write the layer-order contract as executable assertions (AGENTS.md forbids
   relying on comments), and run the full integration + golden suites.

5. **Move the static-export fallback out of `router.rs`.** Relocate `resolve_static`,
   `resolve_prefix_fallback`, `PREFIX_FALLBACKS`, `deny_list`, `content_type` and `web_fallback` into
   `static_fallback.rs`, exposing one `fallback(state) -> ...`; share one path-prefix list between the
   route table and `deny_list` so the two cannot drift.
   *Risk:* symlink canonicalisation and prefix ordering (`/app/artifacts/` before `/app/`) are
   security- and UX-sensitive; a refactor could reintroduce traversal or serve the wrong shell.
   *Mitigation:* replay every static-fallback fixture (path, method, accept, symlink case) as
   property/golden tests before and after the move; keep REQ-040/055/ADR-013 ids in test names
   (`tools/trace_check.py` counts declarations).

## Cross-cutting risks / sequencing

- Sequence 4 → 1 → 3 → 2 → 5: pin order/state and add the conformance tests first (they are the
  safety net), then deduplicate middleware, then caps, then route grouping, then the fallback move.
- Keep every path, cap, header value and status code unchanged; each externally visible delta is a
  DEVIATION in `docs/PARITY.md` requiring approval before shipping.
- Gate each step with `cargo fmt --check`, `clippy -D warnings`, unit + `tests/` integration, golden
  and OpenAPI contract suites; no new dependency is required for items 1-5.

## Out of scope for this module

Authentication/principal design (`auth.rs`), per-procedure RPC dispatch (`rpc.rs`), the body of
`log_http` (`correlation.rs`) and the automation handlers themselves (`automation.rs`) are other
workers' modules; this proposal only restructures how routes and middleware are composed here.
