# 0175 — `crates/app/src/local_models.rs` architecture proposal (v0.4.0)

Module: `crates/app/src/local_models.rs`
Lens: architecture
Spec anchor: REQ-075 (SHOULD — loopback discovery, best effort, never blocks startup) plus the
AGENTS.md rules that `crates/app` is "use cases and ports without a concrete adapter", that config
is typed and validated at startup, that ids/units use newtypes, and that every parser of an external
format gets property/fuzz tests.

Status of claims: "Current shape" is CONFIRMED by reading this file, `crates/app/src/lib.rs`,
`crates/app/Cargo.toml`, `crates/app/src/ports.rs`, `crates/app/src/error.rs`,
`crates/infra/src/local_models.rs`, `crates/api/src/{wire,rpc}.rs` and `spec/01-requirements.md`
this session. Target structure and upgrades are GUESS (design inference; none implemented, none
tested, no command run).

## Current shape (CONFIRMED)

- `lib.rs` promises the app crate "never names a concrete adapter", and `#![deny(missing_docs)]`
  applies. This module breaks that promise: it imports `reqwest` (a concrete HTTP adapter pinned as a
  direct app dependency in `Cargo.toml`) and `serde_json::Value`, builds URLs from `std::env::var`
  on every call, and routes transport/parse failures through the use-case error `AppError::Unavailable`.
- Discovery is not a port: there is no trait, no fake, no injection via `Services`/`Stores`. The API
  (`rpc.rs::models_discovered`) calls `boma_app::local_models::discover()` directly, then hand-maps
  each model into `LocalDiscoveredModelView`. `ModelProvider` in `ports.rs` shows the port style this
  module does not follow.
- A near-duplicate lives in `crates/infra/src/local_models.rs`: same `LocalModels`/
  `LocalDiscoveredModel` shapes and `TIMEOUT_MS`, hardcoded ports, different helper names
  (`ollama_tags`/`lmstudio_models`) and no env override. Two sources of truth for one REQ-075
  behaviour; workers #293/#294 own that copy, so consolidation needs coordination, not a silent edit.
- `source` is a free-form `String` with magic literals `"ollama"`/`"lm-studio"` repeated in app,
  infra and the API view. `id` is a raw `String`. "Ollama first" is a comment and a probe order, not
  an enforced invariant; duplicates between servers are not resolved anywhere.
- Parsing is two hand-rolled `Value` walks with two inline unit tests; no proptest/fuzz despite the
  AGENTS.md parser rule. The two `BOMA_*_URL` variables are read only here, not in the typed
  `crates/bin/src/config.rs` (GUESS: verified by grep, config not read in full).

## Target structure (GUESS)

One vocabulary type in `boma-domain`, one port trait in `crates/app/src/ports.rs`, one HTTP adapter
in `boma-infra`, one wire DTO in `crates/api/src/wire.rs`. This module becomes either the thin
best-effort use case that composes the port, or folds into `usecases.rs`; it stops owning `reqwest`.

## Concrete upgrades for v0.4.0 (GUESS)

1. **Typed vocabulary.** Replace `source: String` with `enum LocalProvider { Ollama, LmStudio }`
   (`as_str` for the wire) and `id: String` with a `LocalModelId` newtype; derive `Ord` and dedup by
   `(provider, id)` so "Ollama first" becomes a testable invariant. *Risk:* the RPC reply must keep
   the exact strings `"ollama"`/`"lm-studio"` and camelCase keys — any change is a DEVIATION in
   `docs/PARITY.md` awaiting approval. Migration is mechanical but touches three crates at once.
2. **Add a port, move the IO out.** Define `#[async_trait] trait LocalModelDiscovery` in `ports.rs`
   returning `LocalModels` (best effort stays inside the adapter), implement it in `boma-infra` with
   `reqwest`, and inject it through `Services` the way `ModelProvider` is. Keep
   `discover_with_urls` as the fake/test seam. *Risk:* touches the service wiring and API call site;
   if injection is incomplete, the API can silently keep calling the old function, leaving two live
   paths. Add one fake-backed test to prove the use case needs no network.
3. **Collapse the duplicate.** Make `crates/infra/src/local_models.rs` the single adapter and delete
   the app-side HTTP code, or vice versa — one implementation, one parse rule, one type vocabulary.
   *Risk:* workers #293/#294 are editing the infra copy this same wave; edit sequencing is required
   or their diffs and this one will conflict. Do not merge both proposals blindly.
4. **Config at startup, not per call.** Add the two endpoints to the typed bin `Config`, validate them
   there (fail fast), and pass them into the adapter at construction; drop `std::env::var` from this
   module and keep `discover_with_urls` only for tests. *Risk:* an invalid override that was formerly
   swallowed (garbage URL -> empty list) would now fail startup — an externally visible behaviour
   change that must be recorded as a DEVIATION or approved before shipping.
5. **Typed parsers plus property tests.** Parse `/api/tags` and `/v1/models` into small
   `#[derive(Deserialize)] #[serde(default)]` structs (permissive: unknown fields allowed, bad rows
   dropped) instead of `Value.get().as_array()` chains, and add proptest cases for arbitrary JSON
   including non-object rows, huge arrays and duplicate ids. *Risk:* a stricter typed shape could
   reject payloads the original tolerated; since discovery feeds only a best-effort list, keep the
   parse layer forgiving and prove equivalence against the current two unit tests before swapping.

## Constraints and non-goals

- Wire contract is fixed: `models.discovered` reply stays `{ models: [{ source, id }] }`, camelCase,
  Ollama first, empty list on failure (REQ-075). No new fields, no error on an unreachable server.
- Caching, concurrent probing and TTL are reliability concerns (workers #176 and #294) — not added
  here; best-effort, never-blocks-startup semantics must survive the refactor.
- No claim above is backed by a command or test in this session; mark items CONFIRMED only after
  `cargo fmt --check`, `clippy -D warnings` and the REQ-075 tests actually run.
