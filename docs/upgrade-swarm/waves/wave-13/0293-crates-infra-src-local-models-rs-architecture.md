# Worker #293 — `crates/infra/src/local_models.rs` — Architecture (v0.4.0)

Scope: adapter structure for loopback model-server discovery (REQ-075). Evidence is static
inspection of this module and its call sites (CONFIRMED unless marked GUESS); no runtime test
was run by this worker. Externally visible changes need a DEVIATION entry in `docs/PARITY.md`.

## Current structure (CONFIRMED)

- `crates/infra/src/local_models.rs` defines `LocalDiscoveredModel`, `LocalModels`, `discover()`,
  `ollama_tags()` and `lmstudio_models()`. It is **not declared** in `crates/infra/src/lib.rs`
  (no `pub mod local_models;`), has no tests, hardcodes both URLs, and imports
  `boma_app::AppError` at the bottom. It is dead code: nothing references it.
- The live implementation is a near-duplicate in `crates/app/src/local_models.rs`: it adds env
  overrides (`BOMA_OLLAMA_URL`, `BOMA_LMSTUDIO_URL`), `discover_with_urls()`, serde derives,
  pure `parse_*` functions and two `req_075_*` unit tests. `crates/api/src/rpc.rs:3196` calls
  `boma_app::local_models::discover()` directly.
- AGENTS.md assigns IO adapters to `infra` and ports/use cases to `app`. The current split is
  inverted: HTTP probing (IO) lives in `app`, while `infra` holds an unwired copy.

## Upgrade 1 — One adapter in `infra`, a port in `app`

Evidence: two divergent copies of the same types and logic; the `infra` one is unreachable.
Two `LocalDiscoveredModel` structs with different derives already exist (infra lacks serde),
so any future consumer must convert between them.

- Declare a pure port in `app`, e.g. `trait LocalModelDiscovery { async fn discover(&self) -> LocalModels; }`
  with the result types, no reqwest, no async runtime calls in `domain`.
- Move the richer `app` implementation into this `infra` module, implement the port for a
  `HttpLocalModelDiscovery` struct, add `pub mod local_models;` + `pub use` to `infra/src/lib.rs`,
  and remove the IO copy from `app`. Keep `app`'s port signatures and the wire DTOs unchanged.
- Inject the adapter at composition/AppState construction; RPC calls the port, not a free function.

Risks: touches AppState wiring (internal, no wire change → no DEVIATION expected). Keep the
`boma_app::local_models` path as a thin re-export shim for one release if other crates need it.
Adding the module activates `#![deny(missing_docs)]` and the strict clippy lints: rustdoc every
public item and avoid `unwrap`/`expect`.

## Upgrade 2 — Typed config and a loopback-only guard (SSRF / correctness)

Evidence: the infra copy hardcodes `127.0.0.1:11434` / `:1234`; the app copy reads env vars with
`std::env::var` on **every call**, unvalidated and outside typed startup config. REQ-075 and the
RPC doc promise "nothing reaches the network beyond the loopback" — an env override to a LAN or
public host silently breaks that promise.

- Put `BOMA_OLLAMA_URL` / `BOMA_LMSTUDIO_URL` in the typed config struct, validated at startup,
  with the loopback defaults preserved. Validate scheme `http`, no userinfo/query/fragment, and
  host in `127.0.0.0/8` / `::1` / `localhost`.
- Fail fast on an invalid override; log the resolved *host* (never credentials) at startup.

Risks: an operator deliberately using a LAN address would break → externally visible, record a
DEVIATION or add an explicit, documented opt-in flag. A `localhost` that resolves via a hostile
resolver is a residual risk; prefer literal loopback IPs.

## Upgrade 3 — Bounded, concurrent, cached probing

Evidence: `discover()` awaits Ollama then LM Studio serially, each with a 2 s client timeout —
up to ~4 s on the synchronous `models.discovered` call. Every picker open re-probes. Neither copy
checks response size; `.json()` can buffer an arbitrarily large body from a broken server.

- Probe both servers concurrently (`join!` of two `Option`-returning futures) so one dead server
  cannot delay the other, with a single overall `tokio::time::timeout` deadline.
- Cap the response body (content-length check + `bytes().await` limit) before parsing.
- Add a short TTL cache (~5–10 s) behind the port so repeated listing stays cheap; preserve
  "Ollama first" by sorting explicitly, not by probe order.
- Add `tracing` spans on failed probes and a probe-outcome metric; no secrets in logs.

Risks: concurrency changes result order unless sorted (sort explicitly). Caching delays seeing a
just-started server by up to the TTL → behaviour change, document as DEVIATION and make TTL
configurable. Timeout values need tuning to avoid slow-CI flakiness.

## Upgrade 4 — Typed results and honest failure handling

Evidence: `source: String` and a free-form `id: String` make invalid states representable; the
infra copy maps every failure to `boma_app::AppError::Unavailable` and discards it silently,
leaking the API error type into an adapter with no diagnostic. Duplicate ids across sources are
not deduplicated.

- Model `source` as an enum (`Ollama`, `LmStudio`) serialized exactly as `ollama` / `lm-studio`
  to keep the wire contract; dedupe by `(source, id)`.
- Replace `AppError` with an adapter-local `thiserror` type (or plain `Option`) and log the
  cause at `debug`/`trace`; discovery still never fails, per REQ-075.

Risks: enum serialization must produce byte-identical JSON — cover it with a wire/contract test
against `spec/openapi.yaml` and the golden, or it becomes a DEVIATION.

## Upgrade 5 — REQ-tagged test coverage for the adapter

Evidence: the infra module has zero tests; the app copy tests only JSON parsing, not orchestration,
env overrides, timeout, SSRF rejection, ordering or dedup. AGENTS.md requires property/fuzz tests
for external-format parsers.

- Keep the `req_075_*` parser tests (property-test them with `proptest`), and add `wiremock`
  tests: one server down, garbage JSON, oversized body, timeout, env override honoured,
  non-loopback override rejected, Ollama-first ordering, dedup, TTL cache behaviour.

Risks: timing tests can be flaky — use deterministic server delays well outside the threshold.

## Priority for v0.4.0

1. Upgrade 1 (remove the orphan, establish the port/adapter boundary) — foundational.
2. Upgrade 2 (typed config + loopback guard) — security, no wire change if defaults hold.
3. Upgrade 4 (typed results) then 3 (concurrency/cache) — keep wire contract under a contract test.
4. Upgrade 5 alongside each step; any externally visible change → `docs/PARITY.md` DEVIATION.
