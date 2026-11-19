# Wave 11 — worker 0216 — `crates/domain/src/bot.rs` — reliability (v0.4.0)

Target: the `Bot` aggregate and `Bot::validate_name`, the pure domain record that carries a bot's
identity, tool grant (REQ-016) and model choice/pin (REQ-015). Scope: name validation, the
bot-carried "executor config" (model + tool fields), and the serde defaults that backfill older
rows and payloads. This is a proposal only; no code was changed.

All findings are **CONFIRMED by reading** `bot.rs`, `error.rs`, `ids.rs`, `model.rs`,
`crates/infra/src/stores/tools.rs`, `crates/app/src/usecases.rs` (create_bot l.352-391 and
resolve_at l.4204-4219) and `spec/01-requirements.md` (REQ-001/015/016), unless marked **GUESS**.

## Failure modes

**F0 — The aggregate has no invariant enforcement.** Every field of `Bot` (l.15-52) is public and
the type has no constructor: any caller can build a `Bot` with an empty name, a contradictory
model config or a blank timestamp. `validate_name` is a free function called only in
`create_bot` (usecases.rs:359); rename/update paths and every `Bot` produced by deserialization
(store rows, API DTOs) bypass it. CONFIRMED. This is the root cause: reliability here cannot be
fixed by hardening `validate_name` alone, because nothing forces it to run.

**F1 — Invalid name: too weak (l.61-72).** The check rejects only empty/whitespace and counts
`chars()` against `MAX_BOT_NAME_LENGTH`. It accepts NUL/newlines/ANSI escapes, zero-width
characters and bidi overrides (U+202E) used for display spoofing, and unnormalised Unicode.
`chars().count()` counts code points, not grapheme clusters or display columns, so 120 combining
marks or a few multi-code-point emoji render unpredictably. CONFIRMED. The stored value is
trimmed by the *caller* (usecases.rs:360), not by the domain, so another caller can persist an
over-limit or untrimmed name. No uniqueness rule exists (GUESS whether the spec implies one).

**F2 — Invalid executor config: representable contradictions.** `model_provider`, `model_id`
and `model_pinned` (l.43-51) are three independent values with no cross-field invariant, then
joined as `format!("{provider}/{model}")` with a `"custom"` provider when absent
(usecases.rs:4209-4215). Consequences, all CONFIRMED:
- `model_pinned: true` + `model_id: None` → `ModelPreferences.bot == None`, so `resolve` silently
  ignores the pin and falls back — a direct violation of REQ-015's "never execute something else
  silently".
- `model_id: Some` + `model_provider: None` → silently becomes `custom/<model>`; a typo'd
  provider is indistinguishable from an intentional custom model.
- Free-form whitespace keys reach `resolve` and fail as `PinnedModelUnreachable` only at run
  time, not at write time.

**F3 — Invalid executor config: fail-open tool grant (l.38-42).** `enabled_tools: Vec<String>`
is unconstrained (no cap, no key shape, duplicates, empty strings) and `[]` means *every*
installed tool (REQ-016, tools.rs:74 `grant.is_empty() || ...`). CONFIRMED. An operator who clears
the list to revoke tools grants all of them instead; a typo'd tool key matches nothing and yields
a silent zero-tool run. There is no domain-level check against the catalogue (that belongs to
the app/infra layer), but the domain lets invalid values through unchecked.

**F4 — Missing defaults are fail-open and silent.** All four new fields use bare `#[serde(default)]`
(l.41,44,47,50) with no `#[serde(deny_unknown_fields)]`. An older row, a partial migration or a
hand-written DTO that omits them gets: empty tool grant → **all tools** (least-privilege inverted),
no pin, no model choice. CONFIRMED. A misspelled key (`model_pin`, `enabled_tool`) is dropped and
replaced by its default, so a pin request silently becomes unpinned. CONFIRMED.

**F5 — Unvalidated timestamps defeat `is_archived` (l.32-36, 75-78).** `created_at` /
`archived_at` are plain `String`; `is_archived` is `archived_at.is_some()`. A corrupted
`Some("")` marks a healthy bot archived (REQ-001 not-found class) or an archived bot reachable.
CONFIRMED that the code path does not check the value.

## Hardening plan (with risks)

1. **Give the aggregate a validating boundary (F0).** Add `Bot::validate(&self) ->
   Result<(), DomainError>` and a `Bot::new(...)` constructor; call them in `create_bot`, on
   rename, and in the DTO→domain conversion at the API/store boundary so no `Bot` exists that
   violates its rules. Keep `validate_name` as the name helper. Risk: validating on deserialize
   can reject legacy rows — land a migration/backfill (see 6) and keep every rejection observable
   via `tracing` + a metric rather than a silent fallback.

2. **Strengthen name validation (F1).** Trim first, reject control characters (Unicode Cc) and
   bidi/zero-width formatting ( Cf, especially U+202E), normalise to NFC, and measure length in
   grapheme clusters (add a small dependency after checking docs.rs + license) or, for v0.4.0,
   in `chars()` with an explicit note on the limitation. Return `DomainError::InvalidName` with a
   reason that never echoes the name (privacy). Risk: stricter rules may reject names already in
   the database; grapheme counting changes the 120-char boundary — record any externally visible
   change as a DEVIATION in `docs/PARITY.md`.

3. **Make the model config a typed invariant (F2).** Replace the three loose fields' semantics with
   an enum such as `enum ModelConfig { Inherit, Preferred(ProviderModel), Pinned(ProviderModel) }`
   (serde-compatible with the existing wire shape via an adapter), or minimally enforce in
   `validate()`: reject `model_pinned` without `model_id`, reject whitespace/empty parts, and
   require `model_provider` whenever `model_id` is set. Keep reachability checks in the app layer
   (`DeploymentModels`), domain validates shape only. Risk: rejects currently persisted
   contradictory rows — backfill first; any wire-shape change is a DEVIATION needing approval.

4. **Replace the fail-open grant with an explicit type (F3).** Model `enabled_tools` as
   `enum ToolGrant { All, Only(Vec<String>) }` internally, cap the list, dedupe, and reject blank
   keys at the boundary; validate keys against the catalogue in the app layer. Keep wire
   compatibility: `[]` still deserialises to `All` (REQ-016), but add a typed internal marker so
   "no tools" cannot be expressed as an empty list. Risk: persistence/API shape is externally
   visible — a DEVIATION entry and owner approval before shipping.

5. **Close the default/unknown-field gap (F4).** Add `#[serde(deny_unknown_fields)]` on the wire
   DTO (or on `Bot` if no additive clients exist) and make defaults explicit and documented: a
   missing config field is `Inherit` + `All` by compat, but log a warning so partial payloads are
   detectable. Risk: `deny_unknown_fields` rejects additive/forward-compatible clients — apply at
   the boundary DTO rather than the stored struct if compatibility requires it.

6. **Validate timestamps / archive state (F5).** Parse `created_at` and `archived_at` as RFC 3339
   at the boundary (reuse the store's existing parser) and make `is_archived` require a
   well-formed, non-empty value. Risk: a one-off backfill is needed for malformed legacy rows;
   a stricter archived check could temporarily make an archived bot reachable if bad data is not
   cleaned first.

7. **Tests (test-first; REQ ids in names).** Proptest names over control chars, bidi overrides,
   combining marks, whitespace, empty and 120/121 boundaries (`req_001_...`); table tests for every
   model-config contradiction and the `"custom"` fallback (`req_015_...`); grant tests for `[]`,
   duplicates and blank keys (`req_016_...`); serde round-trips with omitted fields, unknown keys
   and `Some("")` timestamps. Risk: property tests may surface legacy violations — fix via 1/6,
   never by weakening the assertions.

Priority: F0, F2, F4 → F1, F3, F5 → tests (7) alongside each step.
