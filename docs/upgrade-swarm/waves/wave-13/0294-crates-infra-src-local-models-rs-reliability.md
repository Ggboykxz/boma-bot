# Worker #294 — `crates/infra/src/local_models.rs` — Reliability (v0.4.0)

Scope: reliability lens on REQ-075 loopback discovery. Evidence is static inspection of this file,
`crates/app/src/local_models.rs`, `crates/api/src/rpc.rs:3192-3207`, `crates/infra/src/lib.rs` and
`spec/01-requirements.md:603-608` this session. Claims are CONFIRMED unless marked GUESS; no test
was run in this read-only pass. Behaviour changes need a DEVIATION entry in `docs/PARITY.md`.

## Current status (CONFIRMED)

- This file is **orphaned**: `crates/infra/src/lib.rs` does not declare `mod local_models`, and no
  caller references `boma_infra::local_models`. The live RPC path calls the app copy. So the gaps
  below are *latent* today; they become production-facing the moment worker #293 wires this adapter
  in as the single implementation. Reliability work here is prerequisite to that consolidation.
- Scope: this module is discovery only. It probes Ollama `/api/tags` and LM Studio `/v1/models`,
  parses JSON, returns `LocalModels`. It never loads a model and never streams tokens.

## Failure modes

**FM1 — Discovery failure is completely silent.** Both helpers map transport and `.json()` decode
errors to `AppError::Unavailable` (l.51-54, 77-80); `discover()` swallows both with `if let Ok`
(l.37, 40). No `tracing` at all. Connection refused, 2 s timeout, HTTP 500 with HTML body, malformed
JSON, and a machine with no server are indistinguishable: all yield an empty `LocalModels` with no
kind, source, or elapsed time. Offline fallback works, but a downstream "no model" has no traceable
cause.

**FM2 — No HTTP status gate (CONFIRMED).** There is no `error_for_status()`. A non-2xx reply whose
JSON happens to match `{"models":[{"name":...}]}` or `{"data":[{"id":...}]}` is accepted as real
models. Those phantom ids flow to the picker and then fail at model-load/stream-start time — the
exact confusing failure this lens cares about. Garbage ids are also not filtered: empty or
whitespace `name`/`id` strings are pushed downstream.

**FM3 — Offline fallback has no stability signal.** An empty result correctly triggers the spec's
fallback ("picker shows what the configured provider exposes"), but there is no last-known-good or
staleness notion. A brief loopback restart makes the picker flicker empty; a selection made just
before the restart can disappear, surfacing as a sporadic stream-start error that looks like an
inference bug but is actually a discovery gap (impact GUESS — not reproduced in this pass).

**FM4 — Worst-case latency is 2x the stated budget.** `discover()` awaits Ollama then LM Studio
sequentially; with `TIMEOUT_MS = 2_000` a dead pair costs ~4 s, contradicting "never blocks startup"
under poor conditions. Because the RPC handler awaits discovery inline, that latency is on the
`models.discovered` request path.

**FM5 — Unbounded body buffering.** `.json().await` reads the whole response with no size cap. A
broken or hostile loopback server can force a large allocation; AGENTS.md's body-limit discipline
should apply to these outbound responses too.

**FM6 — No test seam and no tests.** This copy has hardcoded ports and no `discover_with_urls`
equivalent; parsing is inline in the two helpers, with no public parser functions and no `#[cfg(test)]`
module at all. It cannot be exercised without a live server, so none of the failure modes above are
regression-protected (the app copy, by contrast, has two parser unit tests).

**FM7 — Per-call client construction.** `Client::builder().build()` runs on every `discover()` call:
no connection reuse and repeated setup if discovery is re-run on picker refresh.

## Concrete hardening for v0.4.0 (with risks)

- **H1 — Per-source observability.** Keep the infallible public API, but use a small typed internal
  error (transport / status / decode / oversize) and emit `tracing::warn!` for unexpected causes,
  `tracing::debug!` for expected absence (connection refused). Log `source`, status/error kind and
  elapsed. *Risk:* log volume on a hot path. No URLs from env here (ports are hardcoded), so no
  credential-redaction concern unlike the app copy — but add redaction if #293 makes URLs configurable.
- **H2 — Concurrent probes.** Replace the two sequential awaits with `tokio::join!`, collecting in
  fixed order so Ollama-first ordering and public data are unchanged. One 2 s budget instead of 4 s.
  *Risk:* ties the helper to Tokio (already required by reqwest); no public behavior change.
- **H3 — Status gate + id hygiene.** Call `.error_for_status()` before `.json()` and feed the status
  into H1; filter empty/whitespace ids. *Risk:* a non-2xx server that currently "works" by returning
  parseable JSON would stop contributing models — DEVIATION candidate if ever observed; unlikely for
  Ollama/LM Studio.
- **H4 — Body cap.** Stream with a hard limit (proposed 4 MiB) then `serde_json::from_slice`, with an
  `oversize` variant feeding H1. *Risk:* new internal error path; the 4 MiB ceiling is a GUESS —
  validate against a server with many tags before fixing it.
- **H5 — Test seam + property tests.** Extract the two JSON walks into pure parser functions and add
  `proptest` cases (arbitrary `Value` never panics, no empty ids) plus `wiremock` tests for non-2xx,
  oversize, timeout and concurrency. *Risk:* adding dev-deps; check workspace dev-deps first (GUESS:
  `proptest`/`wiremock` present elsewhere). Timeout tests are timing-sensitive — use wide margins.
- **H6 — Reuse a client.** Build the `reqwest::Client` once behind `OnceLock` inside `discover()`.
  *Risk:* freezes config at first use; acceptable since ports are constant here, document it.
- **H7 — Offline stability (DEVIATION — needs approval).** Do not cache by default. Propose in an ADR
  either (a) a short stale-on-error cache in the caller (serve last-known-good ~30-60 s when both
  probes fail) or (b) an additive `DiscoveryStatus` so the picker can say "local servers offline".
  *Risk:* (a) can advertise removed models (reintroducing load/stream failures) and changes picker
  contents; (b) changes a public struct shape. Both need operator sign-off before shipping.

## Coordination

H1-H4 must land before or with #293's wire-up, otherwise the newly-live adapter inherits every gap.
Do not duplicate #176's app-copy edits; share the parser/error design, keep this file the single
adapter per #293, and delete the app-side HTTP path in the same change to avoid two live rules.

## Verification (test-first, REQ-075 names)

`req_075_discovery_survives_dead_servers`, `req_075_non_2xx_contributes_no_models`,
`req_075_parser_never_yields_empty_ids`, `req_075_probes_run_concurrently`, and a `proptest` over
arbitrary JSON. Only mark CONFIRMED after `cargo fmt --check`, `clippy -D warnings` and these tests
actually run.
