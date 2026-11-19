# Skill domain model — architecture proposal (v0.4.0)

Module: `crates/domain/src/skill.rs` (REQ-035 skills).
Lens: architecture. Scope: proposal only — no code was changed.

## Current shape (CONFIRMED — read this session)
- `SkillOrigin` enum (`Builtin`, `Taught`, `Plugin`), `Serialize`/`Deserialize`, `as_str()` but no inverse parse.
- `Skill` { `name`, `description`, `instructions`, `origin: SkillOrigin`, `plugin_source: Option<String>` } — all public fields, no `id`/timestamps, no `TryFrom`/constructor, no `Serialize`.
- Private limits: name 64, description 200 *chars*, instructions 32_768 *bytes*.
- Three free validators all return `DomainError::InvalidName`, even for description and instructions; callers must remember to invoke them.
- `builtin_skills() -> Vec<Skill>` rebuilds one owned `interrogate` skill on every call.
- App layer restates the model as `SkillContext`, `NewSkill`, `SkillSummary`, `SkillRecord` (`ports.rs`); there `origin` is a bare `String`, and stores return raw DB text without re-parsing it (`infra/stores/skills.rs`).
- Callers: `usecases.rs` (list/get/teach, plus skill loading in the worker path), `api/rpc.rs`, `api/wire.rs`, `infra/stores/skills.rs`, `testkit`. Four unit tests; no property tests.

## Guiding principles
Keep the domain pure (no IO/async); make invalid states unrepresentable; keep the wire/DB
contract byte-stable (`builtin`/`taught`/`plugin` strings, lowercase-kebab names, the three
limits, and the `interrogate` built-in). Any externally visible change is a DEVIATION needing
`docs/PARITY.md` and owner approval before shipping.

## Upgrade 1 — Validated `SkillName` newtype at construction
- Current: names are bare `String`; `validate_skill_name` is a free function called at three
  separate sites (`teach_skill`, plugin paths, filter in `usecases.rs`), so a `Skill` can exist
  with an invalid name.
- Proposal: a `SkillName` newtype following `ids.rs`'s `opaque_id!` convention with `try_new`
  that runs the kebab check once; store it in `Skill`. Keep `validate_skill_name` as a thin
  compatibility wrapper and implement `Display`/`AsRef<str>` so DB/wire text is unchanged.
- Why: validation moves into the type; eliminates "validated, then stored as String" drift.
- Risks: touches every literal constructor and store mapping (CONFIRMED sites above). Mitigate
  with staged accessors and an atomic commit; `Deref` is deliberately avoided to keep invariants.
  Effort: medium; highest blast radius of the five.

## Upgrade 2 — Symmetric `SkillOrigin` wire contract
- Current: `as_str()` exists; the inverse is nowhere in the domain — `infra/stores/skills.rs`
  passes DB `origin` text straight into `SkillSummary`/`SkillRecord` as `String`, and
  `usecases.rs` converts back with `as_str()`. A corrupted row is indistinguishable from a
  valid origin.
- Proposal: `impl TryFrom<&str> for SkillOrigin` (reusing `DomainError`) plus `Display`, with a
  round-trip test over all variants; have the store decode through it and have `ports.rs`
  hold `SkillOrigin` instead of `String`.
- Why: one home for the origin vocabulary as new origins are added; typed at every boundary.
- Risks: unknown future values must be rejected, not defaulted; needs a new parse error variant
  (or a documented reuse of `InvalidName`). Store signature changes are internal but ripple
  into `testkit/memory.rs`. Keep serialized spellings identical.

## Upgrade 3 — Make origin/source combinations unrepresentable
- Current: `plugin_source: Option<String>` is independent of `origin`, so `Builtin` with a
  source and `Plugin` with `None` both compile. `parse_plugin_source` produces a validated
  `PluginSourceRef` (domain) but the skill stores raw text and only re-parses at the API edge.
- Proposal: a `SkillSource` enum { `Builtin`, `Taught`, `Plugin(PluginSourceRef)` } replacing the
  `origin` + `plugin_source` pair inside `Skill`, with accessors preserving today's fields;
  or, minimally, a checked `Skill::try_new` that enforces the pairing for all constructors.
- Why: the invariant "only plugin skills have a source, and that source is valid" becomes a
  compile-time/one-place guarantee; removes double parsing.
- Risks: adding/removing public fields breaks every struct literal (CONFIRMED in `skill.rs`,
  `testkit`, and indirectly `ports.rs`); do it atomically. `PluginSourceRef` must round-trip
  to exactly the stored string or existing rows/queries change behavior — a DEVIATION risk.

## Upgrade 4 — Built-in catalogue with one lookup path
- Current: `builtin_skills()` allocates a fresh `Vec<Skill>` with owned strings on every call;
  name lookup and the "name reserved by a built-in" check are separate linear scans in
  `usecases.rs` (`skill_get`, `teach_skill`, plugin install), so the rule is re-implemented by
  each caller.
- Proposal: a `BuiltinSkills` catalogue over a `&'static`/`LazyLock` slice with `get(name)`,
  `names()`, and `is_reserved(name)`; keep `builtin_skills()` as a cloning shim for callers
  that need owned values.
- Why: removes repeated allocation on the load path and centralizes the shadow rule.
- Risks: `LazyLock` requires `std` (GUESS available — unconfirmed); keep strings `&'static` so
  no lazy init is needed. A plugin/taught skill named like a *future* built-in becomes silently
  unreachable while still stored — either reserve a prefix for plugin names or re-check at read
  time; both are behavior changes needing owner sign-off.

## Upgrade 5 — One validated constructor and honest errors
- Current: validators accept trimmed values but `teach_skill` stores `description.trim()` and
  untrimmed `instructions`; limits mix chars (description) and bytes (instructions); all three
  errors are `InvalidName`.
- Proposal: `Skill::try_new(name, description, instructions, source)` performing all checks with
  one consistent policy (trim once, store the trimmed value; document the unit of each limit),
  and dedicated `DomainError` variants for description/instruction violations. Add `proptest`
  coverage for the name grammar and byte/char boundaries.
- Why: a single guaranteed-valid entry point; errors that describe the failing field.
- Risks: changing stored text or error strings is externally visible — DEVIATION gate; confirm
  which errors clients assert on before renaming. New `DomainError` variants touch `error.rs`
  and its exhaustive `Display` match (CONFIRMED).

## Suggested sequencing
1 → 2 → 3 (types and wire parity), then 4, then 5. Each step: tests first with REQ-035 in the
name, then `cargo fmt --check`, clippy `-D warnings`, and the unit/integration suites.

## Open questions / decisions needed (GUESS where noted)
- Is `DomainError::InvalidName` acceptable for new parse/length variants, or add typed ones? Ask owner.
- Should `plugin_source` become a validated `PluginSourceRef` in v0.4.0 (affects stored text) or
  stay a string for parity? GUESS: keep text, add typed access.
- Are `SkillContext`/`NewSkill`/`SkillSummary`/`SkillRecord` in `ports.rs` in scope for this
  upgrade, or should this wave stop at the domain boundary? GUESS: domain only, shims for callers.
- Confirm whether the future-built-in shadow case is a v0.4.0 requirement or deferred.
