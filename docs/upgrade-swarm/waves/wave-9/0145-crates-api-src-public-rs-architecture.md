# Worker 0145 — `crates/api/src/public.rs` (architecture) — v0.4.0 proposal

Scope: the **public share endpoint** structure (`GET /api/v1/pages/shared/{token}`, REQ-037 /
REQ-039) as it lives in this module, plus the module shape around it. Read-only review; nothing
in `crates/api/src/` was changed. Claims are tagged CONFIRMED (read from the tree) or GUESS.

## Current structure (CONFIRMED)

- One route, `router.rs:67-70` → `public::shared_page`, no body limit, no auth layer.
- `public.rs` is a 1579-line grab-bag of ~10 unrelated public handlers: plugin submissions,
  shared page, bot webhook, email relay, messaging, billing, CSP, SCIM, account export/import,
  invite preview. The share code occupies `public.rs:156-260` (constants + handler + two response
  builders); HTML rendering and CSP live in a separate `pages.rs:19-48`; lookup in
  `app/usecases.rs:1590-1602`; token hash in `infra/crypto.rs:110-114` (plain SHA-256).
- The handler does five jobs in sequence: per-IP rate limit → hash+lookup usecase → content
  negotiation (`accept.contains("text/html")`) → JSON projection or HTML render → response build.
- Dual representation from one path: JSON is the golden contract (`golden/shared-page.json`),
  HTML is the sandboxed document (`pages.rs`, strict CSP). 404 is uniform JSON or uniform HTML
  by media type. Every exit carries `no-store`, `x-robots-tag: noindex, nofollow`,
  `referrer-policy: no-referrer`; HTML adds the sandbox CSP.
- Token is 32 random bytes → 43 base64url chars (`crypto.rs:19,101-108`); OpenAPI declares
  `minLength: 16`, the usecase accepts `1..=128`.
- `public.rs` unit tests cover only REQ-054 host parsing; share behavior is tested in
  `crates/api/tests/{http_surface,rpc_surface}.rs`, `crates/testkit/src/build.rs`,
  `crates/conformance` goldens. No unit test for `accept_prefers_html`.

## Concrete upgrades for v0.4.0

1. **Extract a `public/` module; move share code to `public/shared_pages.rs`.** Keep the single
   route path and the handler names so traceability and goldens are untouched. This is the main
   structural win: the share flow (config → negotiation → render → response) becomes one
   reviewable unit instead of lines 156-260 of a mixed-purpose file.
   *Risk:* mechanical/import churn across `router.rs`; re-run all share goldens and
   `cargo fmt/clippy` after. Pure refactor — must produce byte-identical responses.

2. **Centralize the public response contract in one builder**, e.g.
   `PublicResponse::json/uniform_404/rate_limited/html`, reused by `shared_page` and the other
   public handlers. Concretely fix the two failure exits `public.rs:183` and `:216`, where
   `ApiError::from(error).into_response()` currently returns **without** `noindex` /
   `no-referrer` (CONFIRMED): a rate-limit or store outage can serve a page-path error that is
   indexable/referrer-bearing, breaking the privacy invariant the rest of the handler enforces.
   *Risk:* changes failure-path headers only — record as a DEVIATION in `docs/PARITY.md` per
   AGENTS.md, even though it is a hardening. Ensure the builder does not double-insert headers or
   clobber the HTML CSP, and keep the golden 404 body exactly `{"error":"Not found."}`.

3. **Replace substring content negotiation with real `Accept` parsing.** `accept_prefers_html`
   (`public.rs:221-226`) treats any `Accept` containing `text/html` as HTML, so
   `application/json, text/html;q=0.1` wrongly gets HTML. Use a small pure function (or the
   `headers` crate, added via `cargo add` + docs check) with q-values and media-type precedence;
   add unit tests in the new `shared_pages.rs`. Default with no `Accept` and `*/*` stays JSON
   (CONFIRMED: invalid golden sends no `Accept` and expects JSON).
   *Risk:* any externally visible change for mixed `Accept` headers is a DEVIATION; decide the
   precedence rule first and keep the two golden cases (JSON for `application/json`, HTML for
   browser `text/html`) stable.

4. **Close the uniform-404 timing channel.** `usecases::pages_shared` short-circuits empty and
   `>128` tokens **before** hashing (`usecases.rs:1594-1596`), so a malformed token returns
   measurably faster than an unknown/revoked one; `golden/shared-page-invalid.json` explicitly
   requires malformed, unknown and revoked to be indistinguishable in *timing* (CONFIRMED).
   Do constant work for every bounded token (always hash, then look up), letting the shared rate
   limit bound junk traffic.
   *Risk:* a little extra SHA-256 + DB work for junk tokens; still bounded. Keep a hard ceiling
   at the boundary to avoid unbounded hashing of a giant path segment. Verify with a timing
   comparison test, not an assertion.

5. **One source of truth for token shape.** Introduce a validated `ShareToken` newtype at the
   boundary and derive the OpenAPI `minLength` from the same constant used by `TokenGenerator`,
   instead of the hand-typed `16` in `openapi.yaml:211` that disagrees with the 43-char minted
   token (CONFIRMED). Note GUESS: keyed lookup (HMAC with a deployment secret) would harden the
   store against a read-only DB leak; it is low priority because tokens carry 256 bits of
   entropy, and it would require migrating stored hashes — treat as a follow-up ADR, not a
   v0.4.0 change.
   *Risk:* tightening to 43-chars-only would reject the conformance token `golden-shared-token`
   (18 chars, CONFIRMED `conformance/src/golden_world.rs:116`). Keep validation at `16..=128`
   for compatibility and update only the documented contract unless a migration is planned.

## Verification for v0.4.0

`cargo fmt --check`, `cargo clippy -D warnings`, replay `spec/golden/shared-page*.json` and
`spec/golden/ui/REQ-037-*.json`, run `crates/api/tests` share cases, and the OpenAPI contract test.
Any header/negotiation/validation change visible to callers goes to `docs/PARITY.md` with a
justification before shipping.
