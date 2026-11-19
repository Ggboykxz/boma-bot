# 0312 — `crates/infra/src/provider.rs` — reliability proposal (v0.4.0)

Worker #312 · lens: reliability · READ-ONLY review of `.worktrees/before-m5`. Proposals only; no code
changed. **CONFIRMED** = read in this tree this session; **GUESS** = inference needing a test. Anchors:
`provider.rs:10-70`, `lib.rs:11,35,57`, `commands.rs:317-335`, `config.rs:216-228,407-424,430-434`,
`openai.rs:41-60,231-274`, `worker.rs:436-467,442-453`, `ports.rs:200-205,4071-4115`, REQ-015/020/024,
AGENTS.md ("validated at startup, fail fast", "timeout on every outbound call", "make invalid states
unrepresentable", typed errors, no secrets in logs).

## Scope note

`provider.rs` is small: it defines `ScriptedProvider`, an immutable canned-string `ModelProvider`
whose `complete` returns `Ok(Chunks)` for every request (`provider.rs:47-52`). The reliability
questions named in the brief — selection drift, fallback chains, init failures — are *not* in this
file; they are decided by the boot wiring in `commands.rs`/`config.rs`, the BYOK adapter in
`openai.rs`, and the consumer in `worker.rs`. This proposal treats `ScriptedProvider` as the default
node in that chain and cites those anchors (all CONFIRMED).

## What is already right (keep)

- Config is all-or-nothing: half-configured `BOMA_PROVIDER_*` is a startup error, not a silent
  downgrade (`config.rs:407-424`, `optional` trims and drops empty values at `430-434`), tested by
  `req_015_the_provider_config_is_all_or_nothing` (`config.rs:739-768`).
- The key is `SecretString`, excluded from `Debug` (`config.rs:226,230-235`); the boot log carries
  endpoint/model only (`commands.rs:327`).
- BYOK adapter has a 60 s total timeout (`openai.rs:56`) and maps provider refusal to `Unavailable`,
  the one retryable variant (`error.rs:49-51`).
- `chunks()` round-trips its own answer: trailing spaces are carried so `concat() == answer`
  (`provider.rs:54-70`, test at `76-82`).

## Failure modes

**F1 — Provider-selection drift (high, CONFIRMED).** Each run resolves and *records* a provider,
model and pinned flag (`usecases.rs:2050-2052` etc.; `ports.rs:200-205` says a pinned decision "never
falls back silently"). The worker never reads `run.model_provider` / `run.model_id` — the only field
it touches from the run in the failure path is `error.is_retryable()` (`worker.rs:437`). A single
provider is bound at boot (`commands.rs:319-335`) and answers *every* run. So the recorded choice can
silently diverge from the adapter that actually answered, and the "pinned never falls back" contract
is unenforced. This is the core reliability gap.

**F2 — No fallback chain, and a false-healthy script fallback (high, CONFIRMED).** Wiring is a binary
either/or at boot: BYOK or `ScriptedProvider::default()` (`commands.rs:319-334`). There is no ordered
chain: if a configured BYOK endpoint answers `Unavailable` on every call, each run is marked failed
(`worker.rs:436-467`) with no alternative; conversely the scripted branch returns `Ok` for every
request (`provider.rs:49-51`), so a deployment that lost its config completes runs "successfully"
with a canned sentence. Both directions misrepresent health.

**F3 — Init validated too late (medium, CONFIRMED).** `OpenAiCompatibleAdapter::new` fails only when
the TLS client cannot be built (`openai.rs:54-60`); endpoint URL shape, model non-emptiness and
reachability are never checked at startup. A well-formed-but-wrong endpoint/model/key passes boot,
accepts queued work, then fails on the first `complete` (`openai.rs:239-272`). The `/models` probe
exists (`model_http.rs`, REQ-052) but is used for credential validation in the API path, not at
worker boot. Conflicts with AGENTS.md's fail-fast config rule.

**F4 — ScriptedProvider cannot fail or exercise tools (medium, CONFIRMED).** `complete(_request)`
ignores the `ProviderRequest` (`provider.rs:49`), never returns `Unavailable` or `ToolCalls`, and
adds no log/metric. As the "exercisable" default it hides failure and tool-loop paths and gives no
signal that a production run was answered by a script.

**F5 — Fragile chunk tokenization (medium, CONFIRMED).** `chunks()` splits only on ASCII `' '`
(`provider.rs:60-65`): `\n`, `\t` or non-breaking space produce one giant chunk; leading/repeated
spaces produce empty chunks that become empty `message-update` events (REQ-020); an empty answer
yields an empty chunk vector, so a run "answers" with no text.

**F6 — No per-run provider telemetry (low, CONFIRMED).** Only a boot-time `info!` names the provider
(`commands.rs:327,330-332`); `run.finished` logs status/duration but not provider/model/pinned
(`worker.rs:442-453`). Drift (F1) and script fallback (F2) are invisible in production.

## Concrete hardening for v0.4.0 (with risks)

- **H1 — Enforce the recorded choice (F1).** Give the provider an identity/descriptor and have the
  worker compare `run.model_provider`/`model_id` against it; when `model_pinned` and mismatched,
  fail the run legibly instead of answering with the wrong model. *Risk:* port change → DEVIATION;
  existing fixtures record `Some("openai")` while the scripted provider answers
  (`testkit/src/build.rs:2140`), so they must be updated first.
- **H2 — Explicit, opt-in fallback chain (F2).** Replace the boot either/or with an ordered chain
  that falls through only on `Unavailable`, with a bounded attempt budget and a log+metric on each
  fallthrough. Keep scripted as an explicit last resort behind `BOMA_ALLOW_SCRIPTED_FALLBACK`
  (default off), not an implicit fallback. *Risk:* serving canned text after a real outage is itself
  drift — gate on operator opt-in and DEVIATION approval, never silent.
- **H3 — Fail-fast boot validation (F3).** Validate endpoint as http(s) with a host and model/key
  non-empty at config parse, and run one bounded `/models` probe (reusing `HttpModelTransport`) with
  short retry/backoff before the worker loop; refuse to start on wrong config, and never log the key.
  *Risk:* a transient provider outage at boot could block startup — cap retries and allow a documented
  override.
- **H4 — Make the script path honest (F4).** Emit a per-completion log/metric identifying the answer
  as scripted, and add an erroring fake provider so failure/retry paths are exercised in tests; keep
  the `ModelProvider` signature unchanged. *Risk:* extra log volume; no error-format change.
- **H5 — Robust chunking (F5).** Use `split_inclusive(char::is_whitespace)`, drop empty chunks, and
  treat an empty answer as an explicit error (or one empty string); retain the `concat()==answer`
  property. *Risk:* changing chunk boundaries is observable streaming behavior → DEVIATION candidate;
  verify golden stream events before shipping.
- **H6 — Per-run provider telemetry (F6).** Add provider id, model, pinned and latency as bounded
  labels on `run.finished`, and alert on script-answered production runs. *Risk:* label cardinality —
  keep the set fixed.

## Verification (test-first, REQ-named)

- `req_015_*`: all-or-nothing config retained; a malformed endpoint/model fails at boot after H3;
  boot probe retried within budget; key absent from all logs.
- `req_020_*`: proptest `chunks(s).concat()==s` for arbitrary whitespace/unicode; no empty chunk;
  newline/tab answers split; empty answer handled after H5.
- `req_024_*`: BYOK `Unavailable` invokes the chain exactly once per configured policy; pinned
  mismatch fails the run (H1); scripted fallthrough is logged and metric-tagged (H2, H6).
- Check existing `wiremock`/`proptest` dev-deps before adding (GUESS: present). Wide timing margins;
  `cargo fmt --check`, `clippy -D warnings`, and REQ-015/020/024 tests after each step.

## Risks / non-goals

No public port, error-format or streaming-shape change without a DEVIATION in `docs/PARITY.md` and
sign-off (H1, H2, H5). Never log the API key, auth headers or answer content. Do not add live
inference or token accounting here. This is a read-only pass: no passing-test claims; line refs are
against `.worktrees/before-m5`.
