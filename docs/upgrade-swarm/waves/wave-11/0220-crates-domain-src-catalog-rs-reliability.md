# Worker #220 — `crates/domain/src/catalog.rs` reliability proposal (v0.4.0)

Lens: reliability. Posture: READ-ONLY analysis; no source changes, no tests run.
Evidence (CONFIRMED): read `crates/domain/src/catalog.rs` (259 lines), `crates/domain/src/model.rs`,
`spec/01-requirements.md` (REQ-052/018/069), `spec/golden/models-catalog.json`, and the downstream
call sites in `crates/app/src/{usecases.rs,worker.rs}`, `crates/bin/src/commands.rs` and
`crates/api/src/wire.rs`. Findings below are source-inspected; forward-looking items are RISK/GUESS.

## Role of this module
Pure domain catalogue for REQ-052: it maps `DeploymentModels.reachable` keys to display metadata
(name, context window, roles) and supplies indicative prices for REQ-018 spend accounting. Two
downstream paths make its reliability visible: `models_catalog` (usecases.rs:3542) returns providers
in **declaration order** and is golden-tested; `worker.rs:306-312` reads the default model's window
to size the REQ-069 history budget. `provider_endpoint` gates credential connects (usecases.rs:3582),
but there the provider name is already trimmed/lowercased/charset-checked, so catalog validation is
not that path's boundary. In production `reachable` is currently two hardcoded literals
(commands.rs:232-238) — so the modes below are **latent today** but become live if keys ever come
from config/DB/API, and the module is a public API used by tests/testkit.

## Failure modes

### 1. Invalid entries
- FM-1 **CONFIRMED** `catalogue_for` (165) uses `split_once('/').unwrap_or(("custom", key))` with no
  trimming or shape check. `""` → provider `custom`, empty model; `"anthropic/"` → empty model row;
  `"/x"` → provider `""` shown as "Custom"; `"openai/gpt-5/extra"` → model `gpt-5/extra`. Every one
  is silently synthesized (169-179) with a 128_000 window and zero price instead of being flagged.
- FM-2 **CONFIRMED** a typo or extra slash on a built-in id loses its *known* metadata silently:
  `anthropic/claude-sonnnet` or `openai/gpt-5/x` never match `provider_models`, so price becomes
  0/0 and window 128_000. `price_of` (194-203) **never returns `None`** (test 249 asserts
  `custom/x` is `Some`), so callers cannot tell "free local model" from "unknown/typo'd model".
  REQ-018 spend accounting then undercounts to zero with no error.
- FM-3 **CONFIRMED (latent)** `CatalogModel`/`CatalogProvider`/`ModelPrice` derive `Deserialize`
  (16, 25, 42) with no `deny_unknown_fields` and no invariant checks; empty ids/names, a zero
  context window or arbitrary roles would deserialize unchecked. No deserialization site exists
  in `crates/` today — this is a boundary gap if a catalogue file is ever introduced (AGENTS.md
  requires strict, validated boundaries).
- FM-4 **CONFIRMED** `worker.rs:309-312` finds the row whose `key == default` and falls back to
  128_000. Because FM-1/FM-2 synthesize a row for any key, the lookup always "succeeds" but with a
  fabricated window — a typo'd default quietly shrinks/inflates the REQ-069 compaction budget.
- FM-5 **CONFIRMED (minor)** `price_of` builds a whole `DeploymentModels` + `Vec` per lookup; needless
  allocation if used in a hot accounting path, and it duplicates the row-building logic of
  `catalogue_for` (the source of the FM-2 divergence risk).

### 2. Stale data
- FM-6 **CONFIRMED** `provider_models` (99-142) is a hardcoded table the docs call *illustrative*
  with fictional ids/prices. No version, timestamp or refresh path exists. Stale prices feed the
  REQ-018 spend limit; a stale window mis-sizes REQ-069 compaction; nothing detects the drift.
- FM-7 **CONFIRMED** `provider_endpoint` URLs (79-94) are hardcoded; if a provider moves its models
  path, credential validation hits a dead URL and surfaces only as "unavailable" at connect time.
  Fails closed for unknown providers (correct), but a stale known URL is silent until used.
- FM-8 **CONFIRMED (latent)** no guard against duplicate ids inside a built-in table; `find` (167)
  takes the first, so a future duplicate row would be silently shadowed.

### 3. Dedup
- FM-9 **CONFIRMED** `catalogue_for` never dedups `reachable`. A repeated key pushes a second
  `CatalogModel` into the same provider: the REQ-052 reply lists it twice and any per-model
  accounting double-counts. Declaration order is preserved, only multiplicity is wrong.
- FM-10 **CONFIRMED** distinct keys can normalize to the same provider/model pair (a trailing space,
  or a custom key naming a built-in provider), yielding two rows with the same `id` under one
  provider; consumers keyed by `id` may collide.
- FM-11 **CONFIRMED (constraint)** hardening must keep first-occurrence order and the documented
  "unknown key is never dropped" behavior — the golden `models-catalog.json` asserts a custom
  provider/model with window 128000 in order. A naive `HashMap` dedup would reorder and break it.

## Concrete hardening (with risks)
| # | Change | Risk / cost |
|---|--------|-------------|
| H1 | Dedup by exact key with a `HashSet` of seen keys while pushing to the existing `Vec` (first occurrence wins). | Low; preserves order and unknown-key listing (FM-11). Do this first. |
| H2 | Extract a shared `row_for_key(provider, model) -> CatalogModel` used by both `catalogue_for` and `price_of`, removing the duplicated synth logic. | Pure refactor; fixes FM-5 allocation and the FM-2 divergence source. Must keep byte-identical row values. |
| H3 | Add a `parse_key` that trims and rejects empty halves / extra `/`, with a typed `DomainError::InvalidModelKey`. | Deciding to drop/error an invalid key conflicts with "never silently dropped" (REQ-052) and the golden — DEVIATION; owner decision needed. GUESS safe default: keep listing but mark invalid, never drop. |
| H4 | Add `builtin_price_of` returning `Option<ModelPrice>` that is `None` for unknown keys, so accounting can fail loudly on typos; leave `price_of` intact. | Additive, non-breaking; prefer over changing `price_of` semantics (test 249 would fail — DEVIATION, needs `docs/PARITY.md`). |
| H5 | Add a `CATALOG_VERSION` const and rustdoc "illustrative — review each release"; a test asserts every built-in key round-trips through `catalogue_for` with its exact price/window. | Zero behavior change; detects FM-6 drift/typos at build time, not a runtime refresh feed (none in spec — do not invent one). |
| H6 | Add a test that `provider_models` has no duplicate ids (or `debug_assert!` in the table builder). | Test-only; closes FM-8. |
| H7 | If a catalogue file is ever loaded, add `#[serde(deny_unknown_fields)]` and validate on deserialize. | Defer until a real site exists (FM-3 is latent); premature schema work is speculative. |
| H8 | Signal (log/telemetry) when the REQ-069 default key is not a known built-in instead of silently using 128_000. | Touches `crates/app` — outside this module's write scope; proposal only, needs REQ-069 owner. |

## Verification (test-first; REQ id in the test name)
- `req_052_catalogue_deduplicates_keys_preserving_declaration_order` — repeated key appears once, order stable (H1/FM-9).
- `req_052_invalid_model_keys_are_visible_not_silently_defaulted` — empty/trailing-slash/multi-slash keys (FM-1).
- `req_018_builtin_price_of_distinguishes_unknown_from_free_local_model` (FM-2/H4).
- `req_052_builtin_rows_round_trip_through_catalogue` — every built-in price/window preserved (FM-6).
- `req_052_provider_models_has_no_duplicate_ids` (FM-8).
- `req_069_unknown_default_key_is_signalled` (FM-4/H8).
- `req_052_multi_slash_key_does_not_lose_known_price` (FM-2).

## Open decisions (GUESS where unresolved)
- H3 (invalid-key handling) and any change to `price_of` return semantics are externally visible:
  check `docs/PARITY.md` and `spec/05-known-defects.md` and obtain approval before shipping.
- All findings are CONFIRMED by source inspection only; no tests were run in this read-only pass —
  re-run `cargo fmt --check`, `clippy -D warnings` and the domain tests in the implementing session.
