# v0.4.0 architecture proposal — `crates/api/src/pages.rs`

Lens: architecture (static web root serving / SPA fallback structure, `BOMA_WEB_ROOT`).
Proposal only — no code changed. **CONFIRMED** = verified by reading this worktree in this
session; **GUESS** = needs an oracle/spec check before implementation.

## As-is (CONFIRMED)

- `pages.rs` owns only REQ-039: the sandboxed shared-page document (`render_shared_page_html`,
  `:40`) and the pinned same-origin runtime asset (`PAGE_RUNTIME_JS` via `include_str!`, `:27`).
  Its sole handler is `page_runtime_js` (`:72-104`): SHA-256 `ETag` computed once in `OnceLock`
  (`:50-61`), `If-None-Match` → 304, `text/javascript`, `public, max-age=31536000, immutable`.
- All `BOMA_WEB_ROOT` / SPA fallback logic lives elsewhere, in `router.rs`: `web_fallback`
  (`:260-313`) plus `resolve_static` (`:173-198`), `PREFIX_FALLBACKS`/`resolve_prefix_fallback`
  (`:200-221`), `deny_list` (`:223-235`) and `content_type` (`:237-258`). The module named
  "pages" is not where page/web serving happens — a split-brain ownership of web documents.
- `HttpConfig` holds `web_root: Option<String>` (`config.rs:74`); `validate_web_root` (`:162-174`)
  checks only `is_dir()` + `index.html`, with `BOMA_WEB_ROOT` as the error name
  (`HttpConfigError::Invalid`, `:41-48`). Nothing validates the prefix shells or `404.html`.
- `resolve_static` does synchronous filesystem syscalls inside the async handler —
  `Path::is_file()` (`:186`), `std::fs::canonicalize(root)` (`:193`, recomputed on **every**
  request), plus `.is_file()` in `resolve_prefix_fallback` (`:220`). AGENTS.md forbids blocking
  calls on the async runtime.
- SPA shells are a hardcoded const tied to export fixture names (`artifact_golden`,
  `token_unknown`), ordered `app/artifacts/` → `/p/` → `/app/` (`router.rs:206-213`, ADR-013).
- `content_type` is a hand-maintained extension→MIME table (`router.rs:237-258`); the fallback
  sets `no-cache`/`no-store`, with no ETag, `Last-Modified`, `Range` or compression. Asset policy
  diverges from the immutable/ETag policy in `pages.rs`.
- Good invariants to keep: fallback sits inside the `root` router so global security headers,
  body limit, timeout and request logging apply (`router.rs:131-155`; regression test
  `req_055_the_web_fallback_carries_the_same_security_headers_as_the_api`, `http_surface.rs:1024`);
  deny-list protects `/api`, `/rpc`, `/internal`, probes and metrics (`:223-235`); traversal is
  blocked by segment check + canonicalized `starts_with(root)` (`:177-197`, test `:1187`).
- Tests: one REQ-039 test in `pages.rs:106-139`; twelve REQ-055 tests in `http_surface.rs:1015-1194`.

## Concrete upgrades (ranked)

**U1 — Give web-document serving one home.** Move `web_fallback`, `resolve_static`,
`resolve_prefix_fallback`, `deny_list`, `content_type` and `PREFIX_FALLBACKS` out of `router.rs`
into `pages.rs` (or a `pages::web` submodule) so `pages.rs` truly owns all document/asset serving;
leave `router.rs` a thin route table. Introduce a `WebRoot` type built at startup that the
fallback calls, so routing and filesystem resolution are separable in tests.
*Risk:* large move of behavior pinned by 12 REQ-055 tests; must stay byte-identical. Keep the
fallback mounted inside the middleware-wrapped router (the headers regression test is the guard),
keep REQ-039/REQ-055 test names (trace_check counts declaration lines), and avoid new public
`ApiError` shapes.

**U2 — Replace the hand-rolled resolver with `tower-http` FS serving.** Adopt `ServeDir` with
`append_index_html_on_directories(true)` + a `ServeFile` fallback, layered behind the existing
deny-list/accept/method gate; tower-http supplies correct MIME via `mime_guess`, conditional and
`Range` handling, `spawn_blocking`, and hardened path containment. Keep `404.html` and the prefix
shell fallback explicitly via the service fallback chain.
*Risk:* `ServeDir` answers more methods/shapes and sets `Last-Modified`/`Accept-Ranges`/Range
responses and different cache semantics — externally visible changes needing `docs/PARITY.md`
DEVIATION entries. Verify it cannot shadow `/api`, `/rpc`, probes or the pinned runtime route, and
re-run traversal tests. GUESS: confirm against the original whether Range/304 is parity or a new
behavior before promising it.

**U3 — Canonicalize and validate the whole root once, at startup.** Turn `web_root: Option<String>`
into a typed, canonicalized path (`camino::Utf8PathBuf`), validated in `validate_web_root`: root
exists, is a dir, and all referenced shells + `404.html` exist and stay under the root. Store the
canonical base so the per-request `canonicalize(root)` disappears; keep candidate containment via
a relative-path check and use `spawn_blocking` (or tower-http, U2) for any remaining FS stat.
*Risk:* camino is a new dependency (cargo-deny + `docs/DEPENDENCIES.md`); `with_web_root` signature
changes and fixtures pass `String` — keep an ergonomic adapter or update fixtures. Fail-fast on a
root that is valid at boot but swapped/deleted at runtime: missing files must still degrade to the
JSON 404, not a panic (AGENTS.md denies `unwrap`/`expect`/`panic` in non-test code).

**U4 — Make the SPA shell map data-driven.** Replace the `PREFIX_FALLBACKS` const with a manifest
in the export (e.g. `boma-spa.json` mapping path prefixes to shell files) or a typed config loaded
and validated once alongside `web_root`. A new screen family then needs no rebuild, and the
`artifact_golden`/`token_unknown` fixture coupling disappears.
*Risk:* a new external-format parser demands property/fuzz tests and a safe default when the
manifest is absent or malformed (fall back to the current three prefixes, never a 500). Config
surface and DEVIATION gate grow if shell selection changes; document the schema and validate
every mapped shell stays inside the root (same containment as U3). GUESS: check the export's real
shell inventory before defining the manifest.

**U5 — Unify asset/cache/conditional policy across both serving paths.** Define one policy module
used by both `page_runtime_js` and the static fallback: immutable + content-hash ETag for hashed
assets (`/_next/static/...`), `no-cache` + ETag for HTML shells, and one place for compression and
conditional responses. Optionally reuse the `runtime_etag` SHA-256 helper for static files.
*Risk:* changing cache headers, adding compression or Range is externally visible (DEVIATION +
`docs/PARITY.md`, and benchmark/verify with the same command per AGENTS.md). CPU cost of
compression matters on low-end devices (criteria #4). Keep `SHARED_PAGE_CSP`, the `sandbox
allow-scripts` invariant and the `</script>` island neutralization untouched — U2/U5 must not
touch the sandboxed document's security contract.

## Sequencing / verification

U1 (structure) → U3 (root typing/validation) → U2 (tower-http) → U4 (manifest) → U5 (policy).
After each step: `cargo fmt --check`, `cargo clippy -- -D warnings`, the `pages.rs` unit test and
the full `http_surface.rs` REQ-055 suite, plus the OpenAPI contract test. Guardrails that must stay
green throughout: `req_039_the_document_has_no_inline_script_and_neutralises_the_island`,
`req_055_the_web_fallback_carries_the_same_security_headers_as_the_api`,
`req_055_an_unknown_api_path_keeps_the_json_404_even_with_the_export_next_to_it`,
`req_055_a_parent_dot_path_never_resolves_to_the_export`. Record every header/status change as a
DEVIATION in `docs/PARITY.md` and get approval before shipping; U2/U4 need a decision ADR under
the stack policy (new crate / config shape).
