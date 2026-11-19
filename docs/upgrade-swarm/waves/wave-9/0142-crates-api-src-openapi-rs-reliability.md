# 0142 — `crates/api/src/openapi.rs` reliability proposal (v0.4.0)

Worker #142 · module `crates/api/src/openapi.rs` · lens: reliability · READ-ONLY review of
`before-m5`. Proposals only; no source edited.

- **Status:** PROPOSAL. CONFIRMED = read this session (`openapi.rs`, `conformance/tests/contract.rs`,
  `conformance/src/golden.rs`, `api/src/router.rs`, `api/src/pages.rs`, `Cargo.toml`,
  `spec/openapi.yaml`, `docs/adr/003-interface.md`, brief 0105). Everything else is GUESS.
- **Scope:** generation and the intended serving of the HTTP-surface OpenAPI document (REQ-053, ADR-003).

## Current behavior (CONFIRMED)

- `ApiDoc` is a compile-time `#[derive(OpenApi)]` manifest: a hand-listed `paths(...)` of five
  handlers, eight component schemas and three tag entries; `document()` calls
  `<ApiDoc as OpenApi>::openapi()` fresh on every call. It is pure, infallible, allocation-only and
  `#[must_use]` — no I/O, no cache, no failure path today.
- `info.version` is the literal `"1.0.0"` and `info.title` is "Boma Bot HTTP interface"; the
  workspace is `0.3.0-rc5` and the spec says "Bots platform HTTP interface". The contract test
  never inspects `info` or `tags`.
- Two `tags(...)` entries are both named `ops` (the second describes "The sealed screen stream");
  the spec's tag list is `ops/auth/public/webhooks/devices/admin` — no `screen` tag.
- `conformance/tests/contract.rs` checks only: generated paths exist in the spec, `operationId`
  matches, served status codes ⊆ spec status codes, plus two allow-list assertions. Its own header
  claims schemas are compared "by the fields the spec requires", but no schema comparison exists in
  the file (header/test mismatch). It reads `spec/openapi.yaml` via `repo_path()` =
  `CARGO_MANIFEST_DIR/../../<rel>` — correct in-tree, compile-time-fixed.
- No router route serves a document today (grep-confirmed); ADR-003 and brief 0105 intend a served
  `openapi.json` plus a generated TS client. `router.rs` already sets global `Cache-Control: no-store`
  and `pages.rs` has a `OnceLock` + SHA-256 ETag precedent for a generated asset.

## Failure modes

### F1 — Spec drift from the wire (HIGH)

- F1a. `paths(...)` is a manual list decoupled from `router.rs`. Adding a handler (the five
  `/api/auth/*` POST routes, SCIM, webhooks, automation) without updating `paths(...)` yields a
  document that silently omits served routes; the test fails only for generated-but-unspecified
  paths, never for served-but-undocumented ones. CONFIRMED (test direction + router contents).
- F1b. Schema drift is untested despite the header claim: a renamed/removed `ToSchema` field on
  `Liveness`, `AuthCapabilities`, etc. keeps path, `operationId` and statuses, so the contract test
  passes while generated clients (0105) and the wire disagree. CONFIRMED.
- F1c. Metadata drift is untested: version/title/tags can diverge — and already have (duplicate
  `ops`, `1.0.0` frozen against `0.3.0-rc5`) — with no gate. CONFIRMED.
- F1d. `repo_path` resolves through a compile-time-relative path; if the crate is vendored without
  `spec/`, the contract test panics on "spec/openapi.yaml is readable" rather than reporting
  cleanly. LOW, CONFIRMED mechanism.

### F2 — Serve failure (MED, prospective for v0.4.0)

- F2a. Adding an `openapi.json` handler creates a fallible boundary: serialise to bytes and map any
  failure to a structured 5xx through `ApiError`; do not `expect` in a request path. `to_value`
  cannot fail for this type today, so the risk appears only with custom/derived serializers.
- F2b. If served under `BOMA_WEB_ROOT`, `/openapi.json` must be added to `router.rs::deny_list`, or
  the static fallback can answer from a stale file in the export. CONFIRMED mechanism.
- F2c. Content type, ETag and `Cache-Control` must be deliberate: the global `no-store` layer is
  correct, but a long `max-age`/`immutable` would turn serving into caching (F3).

### F3 — Stale cached document (MED, prospective)

- F3a. Memoising with `OnceLock`, as `pages.rs` does, is safe *within one binary* because the derive
  output is compile-time fixed; a rebuild invalidates it automatically. Unsafe is persisting a
  generated JSON artifact (or its ETag) in the repo/edge: it drifts from the Rust source after a
  handler/schema edit until someone regenerates. CONFIRMED mechanism; GUESS that an artifact is planned.
- F3b. An ETag derived from a version string or `file!()`/line, not from the serialised bytes, will
  not change when a `ToSchema` field changes — clients keep a stale 304 body.
- F3c. 0105's build-time codegen must not fetch the document over HTTP from a running server: a down
  server breaks builds and a stale server yields stale types. Prefer a local CLI calling `document()`.

## Concrete hardening for v0.4.0 (GUESS)

- H1 (HIGH, closes F1a/F1b): extend `contract.rs` with (a) an explicit `DOCUMENTED_ROUTES` set in the
  api crate asserted against the router, (b) schema-field comparison (component → required set +
  property names/types, resolving `$ref`) between generated and spec, and (c) an `info`/`tags`
  assertion. Keep "generated ⊆ spec" and the alias exclusions intact. Risk: false positives from
  optional/nullable/default/`allOf` shapes — normalise structurally and triage each initial diff;
  axum exposes no route list, so the constant must be asserted at the router definition site.
- H2 (HIGH, F1c): set `doc.info.version = env!("CARGO_PKG_VERSION")` after `ApiDoc::openapi()` (or
  utoipa's `version = ...` if the pinned version expands `env!`), and fix the duplicate `ops` tag to
  its intended name; update `spec/openapi.yaml` `info`/`tags` in the same change. Risk: published
  metadata is externally visible — record in `docs/PARITY.md` and await approval.
- H3 (MED, F2): serve from a typed handler with a `Result`→`ApiError` 500 path, `application/json`,
  ETag over the serialised bytes with `If-None-Match`→304, global `no-store` retained, and add the
  route to `deny_list`. Risk: a new public route is a DEVIATION needing an ADR/spec entry — or keep
  it internal-only and leave 0105 on a local generator.
- H4 (MED, F3): if caching, use `OnceLock<OpenApiDocument>` built from `document()` on first use;
  compute any ETag from the bytes actually served; never add a runtime "reload" endpoint. Add a CI
  gate that regenerates any committed `openapi.json` and runs `git diff --exit-code` (PLAN.md row).
  Risk: small permanent allocation; a committed artifact still needs the diff gate to stay honest.
- H5 (LOW): make contract failures triage-friendly — distinct messages for "served but undocumented",
  "spec has no such path", "schema field X missing/typed differently", "tag/version drift". Risk: none.

## Risks & trade-offs

- H1's positive-direction checks must not fail on intentional milestone gaps (spec paths not yet
  served): fail on served-but-undocumented, report spec-but-unimplemented as a known gap, not red.
- Schema normalisation is the main false-positive source; start with required + property-name sets,
  add type/ref checks once green, and never weaken the existing gates to accommodate it.
- Caching is not a hot path: `document()` is microseconds. Add `OnceLock` only for the ETag/serve
  case; do not claim a performance win.
- DEVIATION exposure (H2/H3): metadata and any served route are public — ADR/PARITY approval first.

## Acceptance tests (REQ-053 names)

- `req_053_document_version_tracks_cargo_pkg_version`
- `req_053_no_duplicate_tags_in_generated_document`
- `req_053_every_served_route_is_documented_or_allowlisted`
- `req_053_contract_test_detects_a_removed_schema_field`
- `req_053_served_openapi_json_is_etag_validated_and_no_store` (if H3 lands)
- `req_053_openapi_route_is_denied_by_static_fallback` (if H3 lands)

## Constraints

- Keep "generated from handlers, never hand-written"; never copy spec text into the document.
- Keep `generated ⊆ spec`, the `/healthz`/`/readyz` exclusions and `document()`'s infallible,
  `#[must_use]` signature. No secrets/PII in diagnostics.
