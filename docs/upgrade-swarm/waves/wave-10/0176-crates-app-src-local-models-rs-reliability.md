# Worker #176 — `crates/app/src/local_models.rs` — Reliability lens — v0.4.0 proposal

Scope note (CONFIRMED by reading the file and `spec/01-requirements.md:603-608`): this module is
**discovery only**. It probes Ollama `/api/tags` and LM Studio `/v1/models`, parses the JSON, and
returns `LocalModels`. It never loads a model and never streams tokens. Model-load failures and
stream errors live in downstream provider/inference code. Reliability here matters because this
module is the *input* to those flows: a silent, slow, or wrong discovery answer turns a healthy
local model into a downstream "no model" / "stream failed" error with no traceable cause. REQ-075
is SHOULD, "best-effort and never blocks startup"; the empty result correctly triggers the spec's
fallback ("picker shows what the configured provider exposes"). Preserve that contract.

## Failure modes

1. **Total silence on failure (CONFIRMED).** `get_json` maps transport *and* JSON-decode errors to
   `AppError::Unavailable`; both call sites are `if let Ok(...)` and discard the error. No `tracing`
   at all, though `tracing` is already a dependency. A refused connection, a 2 s timeout, an HTTP
   500 with an HTML body, a malformed URL, and a genuinely absent server are indistinguishable: all
   yield the same empty `LocalModels`. Offline fallback works, but diagnosis of a downstream
   model-load failure is impossible.
2. **Worst-case latency is 2x the stated budget (CONFIRMED).** `discover_with_urls` awaits Ollama,
   then LM Studio, sequentially. With `TIMEOUT_MS = 2_000` a dead pair costs ~4 s, contradicting
   "never blocks startup" under poor conditions.
3. **No HTTP status check (CONFIRMED).** `get_json` does not call `error_for_status()`. A non-2xx
   response whose JSON happens to match `{"data":[{"id":...}]}` is accepted as real models. Those
   phantom ids are advertised in the picker and then fail at model-load/stream-start time — the
   exact confusing failure this lens cares about.
4. **Unbounded body buffering (CONFIRMED).** `.json().await` reads the whole response with no size
   cap. A broken or hostile loopback server can force a large allocation. AGENTS.md requires
   request/body limits on inbound HTTP; the same discipline applies to these outbound responses.
5. **Per-call client construction (CONFIRMED).** `Client::builder().build()` runs on every call;
   no connection reuse, repeated setup. If discovery is re-run on picker refresh this churns.
6. **Fragile URL construction (CONFIRMED).** `format!("{ollama}/api/tags")` produces `//api/tags`
   for a trailing-slash env value, and never validates the scheme; such a failure is silently
   swallowed as "offline". Parsers also accept empty-string ids (CONFIRMED), which get passed
   downstream as blank model ids.
7. **Offline fallback has no stability (CONFIRMED behavior, GUESS impact).** Empty result correctly
   falls back, but there is no last-known-good or staleness signal. A brief server restart makes
   the picker flicker empty and can break an in-flight selection, surfacing as sporadic stream
   errors that look like inference bugs.

## Concrete hardening for v0.4.0 (with risks)

- **H1 — Per-source observability.** Keep the infallible public API, but inside `get_json` use a
  small typed internal error (transport / status / decode / oversize) and emit `tracing::warn!`
  for unexpected causes and `tracing::debug!` for expected absence (connection refused). Log
  `source`, redacted URL (strip userinfo), status/error kind, and elapsed. *Risk:* log volume if
  called on a hot path; URLs from env may embed credentials — redact before logging.
- **H2 — Concurrent probes.** Replace the two sequential awaits with `tokio::join!`, collecting
  results in fixed order so Ollama-first ordering (and public data) is unchanged. One 2 s budget
  instead of 4 s. *Risk:* ties the helper to a Tokio runtime (already required by reqwest);
  no behavioral change.
- **H3 — Status gate.** Call `.error_for_status()` before `.json()` and feed the status into H1.
  *Risk:* a non-2xx server that currently "works" by returning parseable JSON would stop
  contributing models — record in `docs/PARITY.md` as a DEVIATION candidate if such a server is
  ever observed; unlikely for Ollama/LM Studio.
- **H4 — Body cap.** Stream the body with a hard limit (proposed 4 MiB, generous for large tag
  lists) then `serde_json::from_slice`, with an `oversize` internal variant. *Risk:* needs a new
  internal error path and a dev-dep-free streaming loop; the 4 MiB ceiling is a GUESS — validate
  against a server with many models before fixing it.
- **H5 — Reuse a client.** Build the `reqwest::Client` once (e.g. `OnceLock<Client>` inside
  `discover()`) and keep `discover_with_urls(&str, &str)` intact for tests by having it use the
  shared client internally. *Risk:* `OnceLock` freezes timeout/config at first use — acceptable
  because env URLs are read once too, but document it.
- **H6 — URL hygiene.** Use `trim_end_matches('/')` before appending the path and reject/ log
  non-http(s) schemes; filter empty/whitespace model ids in both parsers. *Risk:* trivial, no API
  change. Guard against trimming an origin down to empty.
- **H7 — Offline stability (DEVIATION — needs approval).** Do **not** change discovery to cache by
  default. Instead propose, in an ADR, either (a) a short stale-on-error cache in the client layer
  (serves last-known-good for ~30-60 s when both probes fail) or (b) an additive `DiscoveryStatus`
  so the picker can say "local servers offline — showing configured provider". *Risk:* (a) can
  advertise models that no longer exist (reintroduces load failures) and changes externally
  visible picker contents — a DEVIATION per AGENTS.md; (b) changes a public struct shape. Both
  require operator sign-off before shipping.

## Verification (test-first, REQ-075 names)

- Extend parser tests with `proptest`: arbitrary `serde_json::Value` never panics and never yields
  empty/whitespace ids (satisfies "property tests for every parser of an external format").
- Wiremock-based tests for: non-2xx status, oversized body, timeout, trailing-slash URL, and an
  assertion that both probes run concurrently (elapsed < sum of two delays). *Risk:* timeout tests
  are timing-sensitive — use wide margins. Check workspace dev-deps for `proptest`/`wiremock`
  before adding them (GUESS: present elsewhere in the workspace).

All claims above are CONFIRMED by source inspection unless marked GUESS; no tests were run in this
read-only pass.
