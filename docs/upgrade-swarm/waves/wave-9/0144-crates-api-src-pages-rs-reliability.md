# 0144 — crates/api/src/pages.rs — reliability (v0.4.0)

Worker #144 · `crates/api/src/pages.rs` · lens: reliability · READ-ONLY review of `before-m5`.
Facts CONFIRMED by reading `pages.rs`, `router.rs` (120–328), `config.rs` (140–174) and
`tests/http_surface.rs` (1015–1121) this session. Proposals and risks are GUESS (design
inference; no code changed, built or tested).

Spec anchors: REQ-039 (sandboxed shared page + pinned runtime), REQ-040 (stable 404 JSON),
REQ-055 (static web export at the origin), ADR-013 (app-prefix shells).
Boundary note (CONFIRMED): the web-export resolution currently lives in `router.rs`
(`resolve_static`, `resolve_prefix_fallback`, `web_fallback`), not `pages.rs`. For v0.4.0 the
reliability fix is to give `pages.rs` a typed page-serving API that `router.rs` merely calls, so
one module owns "how a page is served". All four failure modes below live in that resolution.

## Current behavior (CONFIRMED)

- `pages.rs`: `render_shared_page_html` escapes the title and `</` in the JSON island; the CSP
  is an opaque `sandbox allow-scripts` with `connect-src 'none'`; `page_runtime_js` serves the
  compile-time asset with a SHA-256 `If-None-Match`/304. Escaping is string `replace`, not a
  real HTML sink; the two `expect`s are lint-suppressed as definitionally unreachable.
- `resolve_static` (173–198) rejects only segments equal to `..`/`.` or containing `\`; its real
  defence is `canonicalize()` + `starts_with(root)` on the candidate.
- `resolve_prefix_fallback` (215–221) joins root + shell and checks `is_file()` **only** — no
  canonicalize/`starts_with` guard. `deny_list` (223–235) is a string-prefix check.
- `validate_web_root` (config.rs 162–174) checks `is_dir()` + `index.html` once, at construction.
- `web_fallback` (260–313): GET/HEAD + non-denied path + HTML-ish `Accept`; `or_else` precedence
  (static → prefix shell → `404.html` → JSON 404). HEAD reads the whole file then drops the body.
  Every request re-reads disk; no ETag/limit on export files.

## Failure modes

F1. **Missing web root / TOCTOU (CONFIRMED gap, GUESS impact).** Validation is startup-only; a
root deleted, renamed or swapped after boot makes every `fs::read` fail, degrading to a
404-only shell (or `404.html`) with no error signal while `/readyz` stays green — exactly the
silent failure the config doc says it prevents. A symlinked root passes `is_dir()`.

F2. **Path traversal (CONFIRMED partial defence).** Only raw `..`/`.`/`\` segments are refused;
percent-encoded `..` is never decoded (safe by accident — it becomes a literal missing name),
and a proxy that decodes ahead of us reintroduces it. The prefix fallback has no canonicalize
guard, so a symlink at `root/p/.../index.html` escapes the export. `canonicalize().ok()?` on the
root means a broken root silently disables static serving.

F3. **Wrong fallback (CONFIRMED mechanics).** String-prefix denies miss `/apiary/x`,
`/internalfoo`; a no-`Accept` client gets the JSON 404 for a valid deep link, while an HTML-ish
client gets a shell for a typo'd non-document path. The implicit `or_else` order is easy to
invert. HEAD reads the full file and a large export asset is read with no size cap (memory/DoS).

F4. **404 double-prefix (GUESS mechanism).** `404.html` and prefix shells are served at deep
URLs; if the export uses relative assets or a client basePath, links resolve as
`/app/artifacts/_next/...` or `/app/app/...` → broken 404 page / blank shell. `web_fallback`
also returns 200 for already-doubled `/app/app/...`, masking the client bug. The shared-page CSP
has `base-uri 'none'` but the export HTML is not under that policy.

## Concrete hardening for v0.4.0 (GUESS)

P1. Add `WebRoot` (canonicalised absolute `PathBuf`) built in `validate_web_root` and stored in
`AppState`; one `safe_join(&self, rel) -> Option<PathBuf>` that percent-decodes, rejects
`.`/`..`/NUL/`\`/absolute segments, then canonicalizes and enforces `starts_with(root)`. Both
resolvers use it. *Risk:* decoding can change which URL serves a file — assert against goldens and
record any change as a DEVIATION.

P2. Health signal: on read error emit `tracing::warn!` (path/error only, no secrets), distinguish
"root unhealthy" from "not found", and wire root health to `/readyz`. Re-check the root rather
than trusting boot state. *Risk:* a transient FS blip could flap readiness and restart pods —
require two consecutive failures with hysteresis.

P3. Replace the `or_else` chain with `enum PageResolution { File, Shell, NotFoundHtml,
ApiNotFound }` and one tested precedence in `pages.rs`; make deny segment-aware (first path
segment in `api|rpc|internal|health|healthz|ready|readyz|metrics`). API-shaped paths never get
HTML. *Risk:* moves a module boundary; keep `router.rs` a thin caller so routes/behavior do not change.

P4. Keep the gate explicit: export only for GET/HEAD with an HTML-accepting header (retain `*/*`);
everything else keeps the REQ-040 JSON 404. For HEAD use `metadata().len()` instead of reading
the body. *Risk:* no-`Accept` deep links currently 404 in JSON — preserve that contract and pin
it with a regression test.

P5. Bound and cache static reads: cap file size via `metadata()` and add ETag/Last-Modified
conditional GET from (mtime, len), mirroring tower-http `ServeFile`. *Risk:* new headers are
externally visible → DEVIATION in `docs/PARITY.md`; make the cap configurable so a legitimate
large asset is not rejected.

P6. Kill double-prefix: require root-absolute export assets (`/_next/...`) or serve shells/404
through a minimal rewriter injecting `<base href="/">`, strip a known `BOMA_WEB_BASEPATH` before
resolution, and 404 (not 200) paths whose prefix repeats. *Risk:* HTML rewriting breaks byte
parity (DEVIATION) and basePath is new config surface — propose, do not ship without approval.

P7. Tests: `proptest`/fuzz `safe_join` with encoded `..`, double encoding, backslashes, NUL,
absolute paths and symlink fixtures; REQ-055 integration tests for deleted root, symlink escape,
doubled prefix, HEAD, deny_list near-misses and no-Accept; retain the REQ-039 island/CSP test.
*Risk:* symlink fixtures need Unix — gate with `#[cfg(unix)]`.

## Constraints / non-goals

- Preserve the REQ-040 stable JSON 404 envelope and the REQ-039 opaque sandbox/CSP; pages.rs
  gains no script execution or network. No new public wire field or route shape without a DEVIATION.
- No `unwrap`/`expect`/`panic` in non-test code; typed `thiserror` errors; no secrets in logs.
- Out of scope: un-embedding the runtime asset, changing the CSP, and client-side router fixes.

## Suggested rollout / acceptance (GUESS)

Wave A: P1, P3, P4 (invariants, no visible change). Wave B: P2, P5, P6 (config/caching/HTML
rewrite — needs approval and `docs/PARITY.md`). Acceptance: the symlink-escape test fails on the
old code; a Playwright deep-link test sees no doubled prefix in asset URLs; `/readyz` flips when
the export disappears.
