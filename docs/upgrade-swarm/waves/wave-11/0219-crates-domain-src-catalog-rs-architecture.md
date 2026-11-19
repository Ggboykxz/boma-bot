# Worker #219 — `crates/domain/src/catalog.rs` architecture proposal (v0.4.0)

Lens: architecture. Posture: READ-ONLY analysis; no source changed except this file.
Requirements touched: REQ-052 (model catalogue + credentials), REQ-018 (indicative prices feed
spending accounting), REQ-069 (context-window lookup).

## What I inspected (CONFIRMED)
- Full `crates/domain/src/catalog.rs` (259 lines): types `ModelPrice`, `CatalogModel`,
  `CatalogProvider`, `CredentialShape`, `ProviderEndpoint`; fns `provider_endpoint`,
  `provider_name`, `provider_models` (private), `catalogue_for`, `price_of`.
- `crates/domain/src/model.rs:59-102` — `DeploymentModels`, `resolve`, `reachable`, `split`;
  `split` duplicates `provider/model` parsing (catalog.rs:165 uses `split_once` too).
- `crates/app/src/worker.rs:306-312` — rebuilds the whole catalogue every run just to read one
  `context_window`, then matches `model.key == self.app.services.models.default`.
- `crates/app/src/usecases.rs:3542,3582-3596` — `catalogue_for` for the listing;
  `provider_endpoint` + a `CredentialShape` match drive live credential validation.
- `crates/api/src/rpc.rs:3218-3224` and `wire.rs:1341-1362` — domain types are manually projected
  to a 4-field `CatalogModelView` (`id`, `name`, `contextWindow`, `roles`); the domain-only `key`
  and `price` are deliberately excluded (golden `models-catalog.json:32` "no keys, no account ids").
- `crates/domain/src/lib.rs:60-61` — all catalog types are public API; `spec/01-requirements.md:423-429`
  (REQ-052) and `:146-153` (REQ-018).

## Current architecture (CONFIRMED)
- One module mixes three concerns: display metadata (name, roles, window), indicative accounting
  metadata (price, key), and provider-endpoint/credential-shape knowledge (hardcoded `match` tables).
- All data is rebuilt on demand: `provider_models` allocates and clones strings per reachable key;
  `price_of` fabricates a throwaway `DeploymentModels` and calls `catalogue_for` (194-203); the
  worker calls `catalogue_for` per run. No single catalogue object exists.
- Keys are plain `String`/`&str` and parsed in at least three places with slightly different rules
  (catalog.rs:165 `unwrap_or(("custom", key))`, model.rs:90-101, worker.rs:309 raw equality).
- `price == 0` is overloaded: it means "known free local", "unknown key fallback" (175-178), and
  "custom model" (test 249 asserts `price_of("custom/x").is_some()`). Spending accounting cannot
  tell "free" from "we have no price".
- `roles: Vec<String>` and `url: &'static str` allow invalid states (AGENTS.md: make invalid states
  unrepresentable); only `chat` is ever used.

## Gaps against the architecture bar
1. Key parsing and key identity are duplicated and inconsistent across three modules.
2. No aggregate: repeated materialization, no O(1) lookup, worker pays for a full build.
3. Domain record conflates internal accounting fields with the public display projection; the
   exclusion is enforced only by an API-layer hand-mapping, easy to bypass by re-export drift.
4. Overloaded zero price hides "unpriced" as "free" — weak REQ-018 foundation.
5. Stringly types where enums/newtypes are mandated.

## Upgrades (v0.4.0)

### U1 — `CatalogKey` newtype, single parser
Define `CatalogKey { provider: String, model: String }` with `parse(&str) -> Result<Self, DomainError>`
and `Display`; validate both halves non-empty. Use it in `catalogue_for`, `model.rs::split`, and the
worker lookup. Store `key: CatalogKey` on `CatalogModel`.
- Why: removes three copies of `split_once`, makes malformed keys unrepresentable, one identity rule.
- Risk: keys without `/` currently map to provider `custom` while the original string is kept as
  `key` (170) — the newtype must preserve that byte-for-byte or the golden and REQ-052 order test
  (221-230) change (DEVIATION; add a normal-key golden and check `docs/PARITY.md`).

### U2 — One pure `Catalog` aggregate built once
Replace table rebuilds with a `const` row table and a `Catalog` type offering `lookup(&CatalogKey)`,
`price_of`, `context_window_of`, and `endpoint_for(provider)`. Domain stays IO-free (no `OnceLock`
with IO); if caching is wanted, a `const`-backed binary search keeps it pure. Update `price_of` and
worker.rs:306 to call it directly.
- Why: single source of truth, deterministic declaration order, no per-run allocation, testable.
- Risk: `catalogue_for` must still yield providers in declaration order (REQ-052 test name); a
  `HashMap` would break that — use a sorted slice or preserve an order index. Public fn signatures
  in `lib.rs:60-61` change only if callers migrate together.

### U3 — Explicit internal-vs-public projection boundary
Split `CatalogModel` into the internal accounting row (key + price) and a display `CatalogEntry`
(id, name, window, roles) that the API maps from. Add `CatalogProvider::entries()` returning only
display fields, so `key`/`price` cannot leak through a re-export.
- Why: the golden's "no keys" rule becomes a domain-enforced invariant, not an API convention.
- Risk: duplicates field names with `wire.rs::CatalogModelView`; keep it a projection, not a second
  contract. No wire shape change — confirm the 4-field golden still matches (CONFIRMED target).

### U4 — Typed price states (REQ-018)
Replace `price: ModelPrice` (0 = everything) with `enum PriceKind { Free, Indicative(ModelPrice),
Unknown }`. Known Ollama rows become `Free`; unknown/custom keys become `Unknown`; `price_of`
returns `Option<ModelPrice>` = `None` for `Unknown` (so test 249's fabricated zero is removed).
- Why: the spending gate can refuse to account an unpriced model instead of silently treating it as
  free; makes the "indicative, not billing" doc (1-9) actionable.
- Risk: changes `price_of` semantics (public API) — DEVIATION candidate needing sign-off; worker's
  default-window fallback (310) must be decoupled from price; update the REQ-018 spending tests.

### U5 — Enum/newtypes for roles and endpoints
`roles` becomes a set of `ModelRole` enum (`Chat`, …) serialized back to `["chat"]`; `ProviderId`
newtype for provider keys; wrap `ProviderEndpoint.url` in a validated `Url` newtype (or `url::Url`).
- Why: make invalid states unrepresentable per AGENTS.md; `req_015_provider_endpoint_must_be_a_url`
  becomes cheap to assert; `CredentialShape`-driven validation in usecases.rs:3588-3596 stays typed.
- Risk: adding the `url` crate needs `cargo add` + `docs/DEPENDENCIES.md` (license check); newtype
  `Debug`/`Serialize` must not change the 4-field wire shape (golden lock). GUESS: defer the URL
  crate if a small validated wrapper suffices — decide by ADR.

## Suggested order (small steps, test-first)
U3 (parity-locked projection) → U1 → U2 (pure refactors) → U5 → U4 (semantics; operator sign-off).
Each step: `cargo fmt --check`, `clippy -D warnings`, REQ-named tests, keep `models-catalog` golden green.

## Verification for v0.4.0 (REQ-named)
- `fn req_052_catalogue_key_parsing_is_single_and_validated()`
- `fn req_052_projection_never_exposes_key_or_price()`
- `fn req_052_catalogue_lookup_preserves_declaration_order()`
- `fn req_018_unknown_price_is_unknown_not_free()`
- `fn req_069_context_window_lookup_needs_no_full_catalogue_build()`
- `fn req_015_provider_endpoint_url_is_validated()`
- Contract: replay `spec/golden/models-catalog.json`; `cargo test --locked -p boma-domain`.

## Provenance
CONFIRMED = read from the named files this session. GUESS = design judgment / parity uncertainty;
DEVIATION items require `docs/PARITY.md` entry and approval before shipping.
