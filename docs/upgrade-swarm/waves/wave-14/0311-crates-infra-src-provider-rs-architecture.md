# v0.4.0 architecture proposal — `crates/infra/src/provider.rs` (worker #311, lens: architecture)

Status: proposal only (read-only review). Claims are CONFIRMED from the files cited unless marked
GUESS. This module is the M1 scripted provider; the interesting architecture question is no longer
"how to chunk a fixed string" but "who owns provider selection now that five providers exist".

## Current structure (CONFIRMED)

- `provider.rs` is 83 lines: `ScriptedProvider { answer: String }`, a hand-rolled `Default`, an
  `ModelProvider::complete` that ignores its request (`_request`) and returns
  `ProviderTurn::Chunks(chunks(&answer))`, a private `chunks()` splitting on spaces, and one test
  (`req_020_the_scripted_answer_is_split_into_ordered_chunks`).
- It is still public (`infra/src/lib.rs:57` re-exports it) and still the **production fallback**:
  `bin/src/commands.rs:319-334` builds `OpenAiCompatibleAdapter` when `config.provider` is set, else
  `ScriptedProvider::default()`. There is no other selection at startup.
- The port has outgrown it. `ProviderRequest` now carries bot, thread, prompt, history, tools,
  memory, skills and `run_id`; `ProviderTurn` is `Chunks | ToolCalls` (`app/src/ports.rs:4071-4115`).
  This provider can produce neither tool calls nor contextual answers — it is a stub that happens
  to satisfy the trait.
- Provider selection is **scattered and ad-hoc**, not a registry:
  - `app/src/worker.rs:594-650` (`provider_for`) does an imperative chain per run: ACP attachment →
    `AcpProvider`; external-agent row → `AgUiProvider`; else the single injected provider. Unknown
    executor or a failed store read logs a `warn!` and silently falls back to the internal model.
  - `infra/src/openai.rs` builds the BYOK adapter; `infra/src/provider.rs` builds the scripted one;
    `app/src/acp_provider.rs` and `app/src/agui_provider.rs` build theirs; `testkit/src/scripts.rs:347`
    defines a **second, richer type also named `ScriptedProvider`** (turn queue, fallback, gate,
    captured requests). Two same-named providers with different capabilities is a live hazard.
- Runs already record a resolution decision — `model_provider`, `model_id`, `model_pinned` on
  `NewRun` (`app/src/ports.rs:200-205`), written in `app/src/usecases.rs` — but `provider_for` never
  reads those fields; it re-derives the provider from attachment/config rows. The "pinned never
  falls back silently" invariant is therefore not enforced at dispatch time (CONFIRMED from the
  cited call sites; whether another path consumes the fields is GUESS).

## Upgrade 1 — Turn `provider.rs` into a dispatch module, not a one-adapter file (CONFIRMED gap)

Add a `ProviderRegistry` in `infra` holding `Vec<(ProviderId, Arc<dyn ModelProvider>)>` plus an
explicit fallback head. `ModelProvider::complete` selection becomes one pure resolver
`resolve(run, registry) -> ProviderChoice` that the worker calls instead of its private `match`.
Keep `ScriptedProvider` as one registered adapter (the `scripted` id), not as the module's whole
identity; move `chunks()` beside it with its `req_020_*` test name unchanged.

- **Why:** there are five `ModelProvider` impls across three crates and no single place that can
  answer "which provider serves this run, in what order, and why?".
- **Risk:** high blast radius (infra + app worker + bin). Keep the `Arc<dyn ModelProvider>` signature
  and `Worker::new` signature stable so testkit builders keep compiling; land the resolver first as
  an additive function, then migrate `provider_for` behind a test.

## Upgrade 2 — Reconcile the two `ScriptedProvider` types (CONFIRMED gap)

Decide one production `ScriptedProvider` in `infra` that honours the full port (uses `request` — at
minimum echoes a contextual answer and can emit a scripted `ProviderTurn::ToolCalls`), and rename
the testkit one to `ScriptedAnswers` (it is a test double with a queue, fallback and request
capture). Re-export the test double from `testkit` under both the old and new names for one release.

- **Why:** `testkit` double is what tests actually exercise; the `infra` stub is what ships. A
  behavior change tested against one and deployed as the other is a parity trap.
- **Risk:** medium — many `testkit` call sites (seen across `testkit/src/build.rs`) and trace_check
  names; do the rename as a mechanical, separate commit. Do NOT delete the M1 `req_020_*` test.

## Upgrade 3 — Drive selection from the resolved run decision (CONFIRMED gap)

Feed `run.model_provider` / `run.model_id` / `run.model_pinned` into `resolve()` as the primary key.
An explicit, pinned choice must win over attachment/config inference and, if its adapter is
missing, fail loudly rather than silently drop to the scripted fallback.

- **Why:** the domain already models resolution and pinning; dispatch ignoring it makes `model_pinned`
  decorative and can answer a pinned run with the wrong model.
- **Risk:** high — externally visible behavior change for existing deployments (a run that used to
  silently fall back now fails), so it needs a DEVIATION/PARITY entry and owner approval before
  shipping. Keep the `warn!`-and-fallback path only for unpinned runs, and test both with named
  REQ-carrying tests.

## Upgrade 4 — Make the fallback chain legible and observable (CONFIRMED gap)

Replace implicit fallbacks with a typed decision (`Selected { provider, reason }` / `FellBack {
from, to, reason }`) emitted as structured `tracing` fields (`run`, `provider`, `fallback_from`,
`reason`). Log provider ids and error kinds only — never `base_url`, bearer names or config values
(the `AgUi` path already handles a `secrecy::SecretString` bearer).

- **Why:** today the only trace of a downgraded provider is a free-text `warn!` in `worker.rs`; an
  operator cannot tell from logs which model answered a run or why.
- **Risk:** low-to-medium — logging changes, not behavior. Guard against leaking provider config or
  prompt/history into fields; keep the existing stable API error envelope untouched.

## Upgrade 5 — One construction boundary, validated at startup (CONFIRMED gap)

Add a `ProviderFactory::build(config, secrets) -> Result<ProviderRegistry, AppError>` in `infra` that
owns all adapter construction (BYOK, scripted, and the executors' specs), so `commands.rs` no longer
builds one provider while `worker.rs` builds others per run. Validate ids are unique and that the
fallback head exists; fail fast at startup.

- **Why:** construction is split across three crates; adding a provider currently means editing the
  registry, the worker chain, the bin wiring and the test builder separately.
- **Risk:** medium — startup now fails on a bad provider config instead of degrading to scripted,
  which is a behavior change to declare in PARITY. Keep secret handling with `secrecy`, keep the
  builder usable by `testkit` without a live database, and preserve the `Arc<dyn ModelProvider>`
  type so no use case changes.

## Out of scope / deferred (GUESS)

SSE streaming, per-run token budgets, and folding local-model selection (`local_models.rs`) into the
registry are sensible follow-ups, but each changes wire behavior or needs new config; keep them out
of the first registry cut. Keeping `AppError` and `model_http.rs` unchanged is deliberate — this
proposal touches selection, not transport.
