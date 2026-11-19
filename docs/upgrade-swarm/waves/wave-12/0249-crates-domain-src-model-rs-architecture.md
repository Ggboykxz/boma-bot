# Upgrade proposal v0.4.0 — `crates/domain/src/model.rs` (architecture lens)

Worker: 249 | Lens: architecture | Module: `crates/domain/src/model.rs`
Status: proposal only — no code changed | Language: English
Scope: model-choice domain model structure in `boma-domain` (pure rules: no IO, no async, no clock).

## 1. Current structure (CONFIRMED — read this session)

- `ModelChoice` (model.rs:11-20): `provider: String`, `model: String`, `pinned: bool`. Serialize/Deserialize derived, so a wire value can carry an arbitrary `provider`/`model` pair with no validation.
- `DeploymentModels` (25-31): `reachable: Vec<String>` (catalogue keys `provider/model`), `default: String`. No constructor; every caller uses a public struct literal. `reachable()` (34-36) is a linear exact-string match. Vec order is a *display contract*: `catalog.rs::catalogue_for` (162-190) emits providers in declaration order (REQ-052).
- `ModelPreferences` (39-47): `bot: Option<(String, bool)>` — the pin flag is an unlabeled tuple field — plus `space`/`person: Option<String>`.
- `resolve()` (59-86): pure. Order = pinned bot → bot → space → person → deployment default; unpinned unreachable choices fall back, pinned unreachable is a visible failure. The fallback loop builds an `[Option<String>; 4]` and flattens it (69-77).
- Two subtleties (CONFIRMED):
  - At line 79 the loop recomputes `pinned` from `preferences.bot` for *every* candidate. Given the early returns at 63-68, the loop can only ever see an unpinned/absent bot, so that expression is **always false** — dead, misleading logic that would mislabel a result if resolution order ever changed.
  - Line 85 returns `DomainError::PinnedModelUnreachable` when the *deployment default* is unreachable. Nothing is pinned there; it is a deployment misconfiguration ("a refusal, not a fallback", per the comment), yet the error text says "the pinned model X is not reachable" (`error.rs:86-91`).
- The `provider/model` grammar and its `"custom"` fallback are parsed **three times**: `model.rs::split` (88-102), `catalog.rs:165`, `testkit/src/memory.rs:1343`; and formatted by hand in `app/src/usecases.rs:4209-4215` (`format!("{provider}/{model}")`). Four copies of one contract.
- `DeploymentModels` struct literals appear in ≥10 places (`infra/tests/*`, `api/tests/http_surface.rs`, `bin/src/commands.rs`, `testkit/*`, `catalog.rs` tests). Tests: 5 REQ-015 unit tests only; no proptest; the line-85 branch and multi-slash/empty keys are untested.

## 2. Target structure for v0.4.0

Keep `resolve()` pure and keep the public field names (compatibility). Strengthen the *representation* so the type system carries the invariants, and centralize the key grammar. No IO, no clock, no new runtime dependency in the domain crate.

```text
ModelKey (newtype, domain)   parse/format "provider/model" once; provider(), model();
                              split_once('/').unwrap_or(("custom", key)) — same rule everywhere
ModelChoice                  provider/model built from a ModelKey; pinned stays a named bool
BotPreference { key, pinned } named fields replace the Option<(String, bool)> tuple
DeploymentModels             reachable: Vec<String> kept (order contract); add try_new() that
                              checks default ∈ reachable; fields stay public for compatibility
PinnedModelUnreachable       pinned-bot case only; new error variant for bad deployment default
resolve()                    same order; fallback results are unpinned by construction (no recompute)
```

**Contract to preserve:** accepted key strings, fallback order, and the pinned-failure behavior. Any change to accepted wire values or to error text is a DEVIATION (docs/PARITY.md) and needs owner sign-off.

## 3. Concrete upgrades (ranked: value / risk)

### U1 — Replace the unlabeled pin tuple with a named type (high value, low risk)
`bot: Option<(String, bool)>` hides a security-relevant flag behind `.1`; `resolve` matches `(key, true)` positionally. Proposal: `pub struct BotPreference { pub key: String, pub pinned: bool }` (or a `Pin` enum `Pinned | Flexible`) in domain, and `ModelPreferences.bot: Option<BotPreference>`. Risk: source-level API change at the ~4 `ModelPreferences` literals (`usecases.rs:4208,7932,8157,9997`) plus tests; internal, no wire impact. Mitigation: update call sites in the same wave; add a REQ-named test that a pinned bot can never yield a fallback choice.

### U2 — One `ModelKey` parser/former (high value, medium risk)
Four hand-rolled copies of `provider/model` + `"custom"` (see §1). Proposal: a domain `ModelKey` newtype with `parse(&str)`, `provider()`, `model()`, `Display`; use it in `split`, `catalogue_for`, and expose it for the app/testkit sites. Risk: behavior drift if the grammar is interpreted differently (multi-slash keys, keys starting with `/`, empty provider). Mitigation: pin current behavior exactly in tests *before* refactoring, including `split_once` semantics and the `"custom"` default; record any wire change as a DEVIATION.

### U3 — Separate the deployment-default error (medium value, low risk)
Reusing `PinnedModelUnreachable` at line 85 produces a false "pinned" message for a config fault. Proposal: add `DomainError::UnreachableDeploymentDefault(String)` with an accurate Display text, keep `PinnedModelUnreachable` for the genuinely pinned case. Risk: a new public error variant flows into the API error mapping; externally visible message change → needs owner approval (docs/PARITY.md). Mitigation: add a REQ-named test for the default-unreachable branch first; if sign-off is withheld, keep the variant but preserve the existing Display text.

### U4 — Validate the default at construction (medium value, medium risk)
`DeploymentModels` can hold a default that is not in `reachable`; `resolve` only discovers this at the end. Proposal: add `DeploymentModels::try_new(reachable, default) -> Result<Self, DomainError>` (returns a config-style error when default ∉ reachable), keep fields public so existing literals still compile, and keep the line-85 guard as defense-in-depth. Risk: two construction paths (literal vs `try_new`) mean the invariant is *encouraged*, not enforced; making fields private would break ≥10 call sites and change the public API. GUESS: the spec does not mandate a constructor. Decision needed before privatizing.

### U5 — Remove dead pin logic + property tests (medium value, low risk)
Delete the always-false `matches!` at line 79 and set `pinned: false` explicitly in the fallback return, with a comment proving why (pinned+reachable returned, pinned+unreachable errored). Add proptest invariants: (a) a successful `resolve` always returns a key in `reachable`; (b) a reachable pinned bot always wins; (c) `ModelKey` round-trips `parse → Display → parse`. Risk: none to behavior; the only care is to keep the audit-trail meaning of `pinned` documented.

## 4. Decisions needed / open questions

- D1: Are U2/U3 externally visible (wire strings, error text)? Owner sign-off / PARITY entry required if so.
- D2: Should `DeploymentModels` fields be privatized for a real invariant (U4), accepting ≥10 call-site changes and a public API break? GUESS until answered.
- D3: Keep `reachable` as `Vec` (order = catalogue display contract, REQ-052) — CONFIRMED; do not "optimize" to a `HashSet`.

## 5. Evidence and limits

CONFIRMED from files read this session: `model.rs` (full), `error.rs` (full), `catalog.rs` (full), `app/src/usecases.rs:4200-4220`, and grep results for `DeploymentModels {`, `ModelPreferences {`, `split_once('/')`, `PinnedModelUnreachable` across `crates/`. GUESS items are labelled inline. No spec/golden files or commands were run; the wire-impact of U2/U3 is unverified and must be checked by the implementing wave before changing behavior.
