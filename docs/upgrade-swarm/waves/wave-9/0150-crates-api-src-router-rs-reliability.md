# Worker #150 — `crates/api/src/router.rs` — LENS: reliability — v0.4.0 proposal

- **Status:** PROPOSAL (read-only review; no source changed). CONFIRMED = verified by reading this file and `correlation.rs`, `automation.rs`, `rpc.rs`, `realtime.rs`, `config.rs` in this worktree; GUESS = needs a test/oracle run.
- **Scope:** assembly of the HTTP surface (`router()`), the global layer stack, and the catch-all/`web_fallback` 404-vs-SPA decision (REQ-040, REQ-055).

## Summary

The route table and the fallback policy are individually coherent, but reliability rests on three implicit contracts that nothing tests: the exact ordering of duplicated middleware stacks, axum's fallback/merge and per-route-body-limit semantics, and a hand-maintained deny-list plus two different containment rules for static files. The single unit test in the module covers liveness JSON, none of these.

## Failure modes

### Middleware ordering (M)

- **M1 (HIGH, CONFIRMED):** the layer stack is duplicated: `root` at lines 135–155 and the automation surface at `automation.rs:502–522`. Any header/cap/logging change must be applied twice; drift is already baked in (global timeout vs automation's `wait+5s`, global cap vs 64 KiB). No parity assertion exists.
- **M2 (MED, CONFIRMED):** per-route `DefaultBodyLimit::route_layer` caps (16 KiB screen-target, 33 KiB submissions, 25 MiB email, 33 MiB import, 65 KiB billing, 17 KiB CSP, `ARTIFACT_MAX_BYTES*2` in `rpc.rs`) coexist with the global `.layer(DefaultBodyLimit::max(...))`. Precedence depends on axum's extension-insertion order. Reordering `.layer()` calls silently clamps the 25 MiB route to the global cap (max 1 MiB per `config.rs`) or removes a cap.
- **M3 (MED, CONFIRMED):** `correlation::log_http` is added last and is therefore outermost — the only layer that inserts `x-request-id` (`correlation.rs:205`). Moving it inside the timeout, or dropping it, strips the id from 408/413/404 replies. `state.clone()` for middleware (line 26) plus `with_state` (134) plus automation's clone means three `AppState` clones; safe only if cloning stays cheap.
- **M4 (MED, GUESS):** the global `TimeoutLayer` (408) wraps all of `main`, including `/rpc/threads/subscribe`, which returns a long-lived `text/event-stream` (`realtime.rs:3,133–134`). Tower's timeout bounds time-to-response, not the stream body — but this is undocumented here; an upgrade or a handler that awaits before returning could truncate SSE. Soak-test it.
- **M5 (LOW, CONFIRMED):** no CORS, compression or proxy/trusted-peer layer in this module; those guarantees must be added by the caller. Reusing `router()` in another binary silently drops them.

### Catch-all shadowing (S)

- **S1 (HIGH, CONFIRMED):** `.fallback(web_fallback)` is attached to `root`, but the returned builder nests `automation` *before* merging `root` (156–158). Unknown `/api/v1/automation/*` therefore hits automation's default bare 404, not the stable `ApiError` JSON envelope REQ-040 promises for every unknown path. It also relies on axum's "merge adopts a single fallback" behaviour; two explicit fallbacks panic at startup.
- **S2 (MED, CONFIRMED):** `deny_list` (223–235) is hand-maintained and matches probes/metrics by exact path. A future reserved prefix (`/api/v2`, `/ws`, `/events`) falls through to the SPA shell and answers HTML 200 for an unknown API path.
- **S3 (MED, CONFIRMED):** static-vs-dynamic precedence is implicit in matchit (e.g. `/api/v1/messaging/install/slack/callback` vs `/api/v1/messaging/{platform}/inbound/{installationId}`). A new literal colliding with a `{token}` shape can be shadowed with no compiler error.
- **S4 (MED, CONFIRMED):** `Uri::path()` is raw — used for traversal filtering (177–181), file lookup (183–197) and log templates. `%2e%2e` is not decoded, so the `..` check misses encoded forms (lookup still fails safely, 404); conversely a legitimately encoded filename (space as `%20`) never resolves. Encoded ids reach `template()`, but prefix redaction keeps tokens out of logs.

### 404 vs SPA fallback (F)

- **F1 (HIGH, CONFIRMED):** `resolve_prefix_fallback` (215–221) only calls `.is_file()` on `root.join(constant)` — it lacks the `canonicalize().starts_with(root)` containment check that `resolve_static` applies (193–197). A symlink inside the export (e.g. `app/index.html` pointing outside `BOMA_WEB_ROOT`) is read and served. `resolve_static` rejects such a path, then `resolve_prefix_fallback` reintroduces it.
- **F2 (MED, CONFIRMED):** `accepts_html` (264–268) treats `*/*` as HTML, so non-browser clients requesting an unknown non-reserved path receive the SPA shell or HTML 404 instead of JSON.
- **F3 (MED, CONFIRMED):** `HEAD` reads the whole file into memory solely to compute `Content-Length` (274, 276–285), and `is_file()`/`read()` are separate calls (TOCTOU on a mutable export).
- **F4 (MED, CONFIRMED):** no `Vary: Accept` on fallback responses; a cache/CDN can serve an HTML shell to a JSON client or vice versa.
- **F5 (LOW, CONFIRMED):** cache policy is split (`no-cache` assets, `no-store` 404.html, global `if_not_present`) and undocumented.

## Concrete hardening (prioritized)

- **P0 — containment & 404 parity:** give `resolve_prefix_fallback` the same canonicalize+`starts_with(root)` guard as `resolve_static` (F1); add an explicit automation fallback returning the stable envelope (S1); pin the axum version and add a router-construction smoke test that builds the full router and asserts no panic (M1/S1).
- **P1 — order & limits:** extract one `apply_common_layers(state, body_cap, timeout)` helper used by both surfaces (M1); add `oneshot` tests asserting `x-request-id` and all three security headers on 404/408/413 and each per-route body cap, including that the 25 MiB email route is not clamped (M2/M3); replace `deny_list` with one reserved-prefix table shared by routing and fallback, defaulting to JSON 404 (S2).
- **P2 — fallback quality:** require explicit `text/html` for the SPA shell, keep `*/*` on JSON (F2); use `metadata().len()` for HEAD and a single open/read (F3); add `Vary: Accept` (and `Accept-Encoding` if compression is added) (F4); percent-decode once at the boundary before filtering/lookup (S4); document the caching split (F5).
- **P3 — coverage:** add enumerating tests that every literal route resolves (S3), an SSE soak test confirming the timeout does not bound the stream (M4), symlink and `..`/`%2e%2e` fixtures (F1/S4), and a note describing the required outer CORS/compression/proxy stack (M5).

## Risks & mitigations

- **DEVIATION exposure (AGENTS.md):** F2 and S1 change externally visible 404 shapes for some clients; record in `docs/PARITY.md` and get operator approval before shipping. Keep the stable JSON envelope byte-identical.
- The shared-layer refactor (P1) must preserve wrapping order exactly; the header/request-id tests are the guard, so land them first.
- Symlink/containment fixtures need a temp export; keep them under `tests/` with `index.html` present so `validate_web_root` still passes.
- Moving SSE out of the timed surface (M4 remediation) is a P3 *option*, not a default — it changes timeout semantics and must be an ADR.

## Acceptance tests (suggested names)

- `req_055_prefix_fallback_never_escapes_web_root` (F1)
- `req_040_unknown_automation_path_uses_stable_envelope` (S1)
- `req_040_reserved_prefix_returns_json_not_shell` (S2)
- `req_055_security_headers_and_request_id_on_error_paths` (M3)
- `req_055_per_route_body_caps_are_not_clamped` (M2)
- `req_055_accept_wildcard_gets_json_404` (F2)
- `req_055_head_has_content_length_without_body` (F3)

## Decisions needed (GUESS → operator/spec)

1. Is the bare automation 404 intentional, or must it use the REQ-040 envelope (S1)?
2. Should `*/*` keep the SPA shell for compatibility with the original UI's fetch defaults (F2)?
3. Is CORS/compression/proxy handling guaranteed by the binary today (M5)?
4. Should `/rpc/threads/subscribe` be explicitly exempt from the global timeout (M4)?
