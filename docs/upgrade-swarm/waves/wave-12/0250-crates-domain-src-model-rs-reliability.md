# Worker #250 — `crates/domain/src/model.rs` — reliability lens — v0.4.0 proposal

## Scope and status

Read-only review of `crates/domain/src/model.rs` (186 lines) and its direct neighbours:
`error.rs`, `catalog.rs`, `app/src/usecases.rs::model_choice_for` (4191-4220), `service.rs`
config assembly, and spec REQ-015 / REQ-052 / REQ-018 / REQ-075. No code was changed.
All claims below are CONFIRMED from those files unless marked GUESS.

## What the module does today

`resolve()` (59-86) checks a pinned bot key against `DeploymentModels.reachable` by exact string
equality, then walks bot → space → person → deployment default and returns the first reachable key.
Any failure is `DomainError::PinnedModelUnreachable(String)` (error.rs:48). `reachable()` (34-36) is
`==` over a static `Vec<String>` assembled at startup; `split()` (89-102) splits on the first `/`
and silently assigns `provider = "custom"` when there is no slash. `ModelChoice` carries only
provider / model / pinned. No record of which preference was rejected or why is returned, though
the module docs (3-5, 51-53) promise the fallback is "part of the audit trail".

## Failure modes

1. **Invalid model ids.** `split()` accepts empty halves (`/gpt-5` → provider `""`; `openai/` → model
   `""`), extra segments (`openai/gpt-5/extra` → model `gpt-5/extra`), and whitespace/case variants.
   A key lacking a slash is coerced to `custom` rather than refused, so a typo such as `gpts-5`
   becomes a legitimate-looking custom model. Because `reachable()` is exact and unnormalized, an
   unpinned typo silently falls back and the bot's configured model is ignored with no audit event;
   a pinned typo produces a visible error. Neither case identifies *which* preference (bot/space/
   person) was rejected. `usecases.rs:4209-4215` builds the bot key with `format!("{provider}/{model}")`
   and defaults a missing provider to `"custom"`, so a bot with `model_id` but no `model_provider`
   usually resolves to a silent fallback rather than to the model the operator set.
2. **Capability drift.** `reachable` is a startup snapshot: a provider renaming or retiring a model
   leaves a stale key that still resolves and fails later at call time (REQ-075 discovery is
   best-effort, but nothing marks a key stale). `resolve()` checks only key membership — never the
   catalogue's `roles`, `context_window`, or price (catalog.rs:26-39). A reachable key with no `chat`
   role or a tiny window is selected and fails downstream. `catalogue_for()` (catalog.rs:162-190)
   materializes unknown keys with `roles = ["chat"]`, `context_window = 128_000`, `price = 0` via
   `unwrap_or_else`, so drift is hidden *and* spend accounting under-counts (REQ-018 caps can be
   bypassed by a misspelled model priced at zero).
3. **Provider mismatch.** Provider is derived from the key prefix, but reachability treats the key as
   opaque, so nothing checks that the prefix names a provider this deployment can actually route to.
   `catalog::provider_endpoint()` (77-97) knows only `openai`/`anthropic`/`openrouter`/`ollama`;
   a reachable `mystery/qwen-99` resolves successfully although no credential shape exists. A
   cross-provider key such as `openai/claude-sonnet` passes membership and can be dispatched with
   the wrong auth header (`CredentialShape`, catalog.rs:54-61) → runtime 401. The `custom`
   catch-all collapses "configured custom adapter" and "malformed key with no slash" into one
   identity, so mismatch is undetectable at this layer.
4. **Configuration and latent-code faults.** `models.default ∉ reachable` falls through the loop to
   the same `PinnedModelUnreachable(default)` error (83-85), mislabelling a deployment config fault
   as a user's pinned choice; there is no startup validation of the invariant. The `pinned` flag
   computed inside the loop (79) is dead: a pinned bot key either returns at 65 or errors at 67, so
   the `matches!(.., Some((_, true)))` branch is always false — harmless today, a trap on refactor.

## Concrete hardening for v0.4.0

- **H1 — validate keys at the boundary.** Add `parse_key(&str) -> Result<ModelKey, DomainError>`:
  reject empty/whitespace halves, extra `/`, and leading/trailing whitespace; keep an explicit,
  allow-listed `custom` form instead of the implicit no-slash default. Call it from `split()` and
  from a `DeploymentModels::validated()` constructor that rejects malformed `reachable` entries and
  asserts `default ∈ reachable` at startup (fail fast, AGENTS.md config rule). New typed errors
  `InvalidModelKey(String)` / `InvalidDeploymentModels(String)` with safe static Display; keep
  `PinnedModelUnreachable` unchanged for REQ-015 parity.
- **H2 — carry the resolution trace.** Return a `Resolution { choice, from: PreferenceSource,
  skipped: Vec<(String, SkipReason)> }` (or `resolve_traced` alongside `resolve`) naming bot/space/
  person/default and why each candidate was rejected. The app layer emits one audit event per run;
  this restores the doc's promise and makes silent fallback observable. Keep `resolve()` as a
  wrapper so existing callers compile.
- **H3 — check capability, not just membership.** In `DeploymentModels`, accept an optional
  capability map (`key → { roles, context_window, provider_route }`) sourced from `catalog.rs`.
  `resolve()` refuses a key whose `roles` lack `chat` (new `ModelLacksChat(String)`) and records a
  `Stale`/`Unknown` flag so `price_of` cannot silently become zero for a reachable key. Constrain
  `catalogue_for()`'s default row to keys in `reachable`, with a test that an unknown key never
  silently gets a free `chat` row for spend accounting.
- **H4 — verify provider route.** Use `catalog::provider_endpoint()` to classify the split provider
  as `BuiltIn` or `Custom`; refuse a built-in provider key whose model id is known to belong to a
  different built-in provider (new `ProviderMismatch { key, found, expected }`). For `Custom`,
  require an explicit deployment-declared adapter id — never infer it from a missing slash. Keep the
  domain IO-free: accept a precomputed route set, do no network calls here.
- **H5 — tests.** Replace the dead-flag branch with an explicit `from` so `pinned` is set from the
  winning preference, not inferred. Add REQ-id tests: `req_015_rejects_empty_and_multislash_keys`,
  `req_015_default_must_be_reachable`, `req_015_resolution_trace_names_skipped_preferences`,
  `req_052_reachable_key_without_chat_role_is_refused`, `req_018_unknown_key_is_not_priced_zero`,
  `req_015_provider_mismatch_is_refused`. Add a `proptest` property: for any generated preferences
  and models, `resolve()` never returns a key absent from `reachable`, and a pinned bot key absent
  from `reachable` always errors (AGENTS.md: proptest for critical rules).

## Risks and mitigations

- **Source/wire change:** H1-H4 add error variants and may change which keys are accepted. Externally
  visible behavior changes are DEVIATIONs — record them in `docs/PARITY.md` and wait for approval.
  Mitigation: land H1/H5 (validation, tests, no wire change) first; gate H3/H4 on the parity entry.
- **Startup strictness:** `validated()` can break a deployment whose `reachable` contains a legacy
  odd key (e.g. no slash). Mitigation: ship a one-version warning path (log + metric) before making
  it a hard failure, and keep an explicit `custom/<id>` escape hatch for legitimate local models.
- **Cross-module coupling:** H3/H4 read `catalog.rs`; a circular import would break the pure-domain
  rule. Mitigation: pass a `ProviderRoute`/`Capabilities` value into `resolve()` from the caller;
  `model.rs` depends only on plain data, matching the existing `DeploymentModels` pattern.
- **Over-strict capability checks:** refusing a `chat`-less key could reject a model that actually
  works via an alias. Mitigation: treat unknown capabilities as `Unknown` and fall back (unpinned)
  or error (pinned), mirroring REQ-015; never silently select. GUESS: confirm alias behavior against
  the original before enforcing.
- **Error taxonomy alignment:** `PinnedModelUnreachable` is currently mapped to HTTP 400 and never
  retried (worker #228, `error.rs` proposal). H1's new errors inherit that bug until #228's
  `ErrorClass::Unavailable` lands. Mitigation: coordinate H1 error variants with #228 so they are
  classified as 503/retryable from day one; do not duplicate the taxonomy here.
- **Test churn:** `catalogue_for`'s default-row behavior is asserted in `req_052_an_unknown_key...`;
  tightening it changes that test. Mitigation: update the assertion deliberately and record the
  parity decision rather than weakening it.
