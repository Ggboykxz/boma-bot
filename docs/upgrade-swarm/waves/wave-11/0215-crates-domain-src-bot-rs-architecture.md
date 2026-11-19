# Worker 0215 — `crates/domain/src/bot.rs` architecture proposal (v0.4.0)

Lens: architecture. Posture: READ-ONLY review of the 92-line file plus targeted reads of
`ids.rs`, `model.rs`, `error.rs`, `group.rs`, `routine.rs`, `computer.rs`, `lib.rs`,
`crates/infra/src/stores/bots.rs`, `crates/api/src/wire.rs`, `crates/app/src/ports.rs` and
`spec/02-data-model.md`. No code changed, no tests run. All findings are **CONFIRMED by source
inspection**; forward-looking items marked RISK or GUESS.

## Current structure (as read)

- One flat `Bot` struct (15–52) of 12 public fields: identity (`id`, `space_id`, `user_id`),
  presentation (`name`, `description`, `color`), one relationship (`thread_id`), lifecycle
  (`archived_at`, `created_at`), and two policy clusters: tool grant (`enabled_tools`) and
  model choice (`model_provider`, `model_id`, `model_pinned`).
- All fields are mutable `pub`; there is no constructor and no `new()`. The only logic is a
  static `validate_name(&str) -> Result<(), DomainError>` (61–72) and `is_archived()` (76–78).
  The struct therefore cannot enforce its own invariants: any caller can build a `Bot` with an
  empty name, a pinned model with no model id, or an archived bot with `archived_at = None`.
- No `#[serde(deny_unknown_fields)]` (unlike `Group` in `group.rs:65`), so unknown JSON keys are
  silently accepted on deserialisation.
- Timestamps are RFC-3339 `String`, although `chrono` is already a domain dependency
  (`routine.rs:9`). Date validity and ordering are unchecked.
- `model_provider` / `model_id` / `model_pinned` **duplicate** the model concept already typed in
  `model.rs`: `ModelChoice { provider, model, pinned }` and `ModelPreferences.bot:
  Option<(String, bool)>`. `model.rs::split` even re-parses the `provider/model` key that `Bot`
  stores as two fields. The two representations can drift.
- `enabled_tools: Vec<String>` overloads emptiness as a sentinel: empty = "all tools in the
  space" (REQ-016), non-empty = explicit grant. The rule exists only in a doc comment (39–40).
- The spec data model (`spec/02-data-model.md:38`) lists more Bot fields (`instructions`,
  `instructionHistory`, `avatarSet`, `sectionId`, `parentBotId`, `spawnKey`, `memoryScope`,
  `computerId`, `voiceId`, `position`) than the struct carries; several clearly belong to sibling
  aggregates (`Computer`, `MemoryDocument`) referenced by `BotId`, not to `Bot` itself.
- `Bot` is re-exported (`lib.rs:54`) and built in at least 12 sites across infra/testkit/tests.

## Target structure for v0.4.0

Keep the public field names and every `req_0xx_...` test name (compatibility + traceability
gates). Introduce value objects and one constructor, then let the store map to/from them:

```
Bot { id, space_id, user_id, name, description, color, thread_id,
      archived_at, created_at,                       // typed instants at the boundary
      tools:      ToolGrant,                         // All | Only(Vec<String>)
      model:      Option<BotModel> }                 // one object, not three loose fields
impl Bot { fn new(...) -> Result<Self, DomainError>; fn model_preferences(&self) -> ModelPreferences }
```

## Concrete upgrades

### 1. Consolidate the three model fields into one typed value object (cohesion)
CONFIRMED: `model_provider`, `model_id`, `model_pinned` (43–51) express the same choice as
`model.rs::ModelChoice`/`ModelPreferences::bot`. Change: add `BotModel { key: String, pinned: bool }`
(or reuse `ModelChoice`), give `Bot` one `model: Option<BotModel>` and a
`fn model_preferences(&self) -> ModelPreferences` that feeds `model::resolve` directly; delete the
duplicate splitting logic. Benefit: a pin can no longer exist without a model, and REQ-015
resolution has one source of truth.
RISK: touches `BotRow` (`infra/src/stores/rows.rs:65–70, 260–292`), store INSERT/SELECT
(`stores/bots.rs:40–55, 116`), `NewBot` port and many testkit builders. Keep the three DB columns
and their names — map at the adapter, do not change persisted schema. Public struct fields change
type; none are wire DTOs (`api/src/wire.rs:622` exposes only id/name/thread/dates), so no API
DEVIATION, but verify no golden fixture serialises `Bot` directly.

### 2. Make the tool grant explicit instead of an empty-vec sentinel (correctness)
CONFIRMED: REQ-016's "empty means every tool" lives only in a doc comment (39–40); a caller who
forgets to populate `enabled_tools` silently grants every installed tool. Change: introduce
`enum ToolGrant { All, Only(Vec<String>) }` with `Default = All` and a custom serde impl that keeps
the wire/DB shape as a JSON array (`[]` ⇄ `All`, non-empty ⇄ `Only`). Add a `Bot::may_use(&self,
key)` query so the rule is enforced in one place.
RISK: custom serde must round-trip the existing array format exactly or stored grants change
meaning — a security-relevant DEVIATION. Keep `infra` `enabled_tools: Vec<String>` columns and
translate in `rows::bot`; add a golden test asserting `[]` deserialises to `All` and back.

### 3. Replace stringly lifecycle state with typed instants + strict deserialisation (validity)
CONFIRMED: `archived_at`/`created_at` are `String` (34–36); `chrono::DateTime<Utc>` is already
available. CONFIRMED: `Bot` lacks `#[serde(deny_unknown_fields)]` while `Group` has it. Change:
type the two timestamps as `Option<DateTime<Utc>>` / `DateTime<Utc>` and add
`#[serde(deny_unknown_fields)]`; keep wire output RFC-3339 via serde. Keep `is_archived()` as the
single query and add `archived_at` invariants (no archive before create).
RISK: cascades through `rows.rs`, the `boma.rfc3339()` SQL helper and all `Bot { ... }` literals;
chrono serde feature must be confirmed before coding. Rejecting unknown JSON keys is an
acceptance change — record in `docs/PARITY.md` as a DEVIATION if any fixture relies on it.

### 4. Add a canonicalising constructor and real validation (invariants)
CONFIRMED: `validate_name` returns `()` and does not store the trimmed value (61–72), unlike
`group.rs::validate_name` which returns the trimmed `String`. A name `"  Analyst  "` passes and is
persisted verbatim. There is no validation of `description`, `color`, or ties between `thread_id`
and `id`. Change: `Bot::new(...)` canonicalises the name (trim + limit, matching `Group`), validates
optional fields, and is the only way infra/testkit construct a `Bot`; add a proptest for name
length/unicode and one for "constructor output always satisfies `validate_name`".
RISK: trimming changes stored/displayed names for new bots — a visible behaviour change; gate to
the creation path, add a DEVIATION note, and leave existing rows untouched. Field visibility can
only be tightened after all builders are migrated.

### 5. Document the aggregate boundary and align it with the spec (clarity + parity)
GUESS: some spec fields in `02-data-model.md:38` are intentionally modelled as separate
aggregates (`Computer` in `computer.rs`, `MemoryDocument` in `memory.rs`, `Routine` in `routine.rs`)
referenced by `BotId`, while `instructions`, `avatarSet`, `sectionId`, `parentBotId` and `position`
have no domain home. Change: add a module-level "Aggregate boundary" doc block listing exactly
which fields `Bot` owns and which are sibling aggregates keyed by `BotId`; file each missing field
as a parity question in `docs/STATE.md` rather than guessing. Keep `Bot` free of IO/async per
`AGENTS.md`.
RISK: adding fields to the domain struct is a schema/port/wire change and must not be done
before confirming the field is a MUST and where it is persisted; this upgrade is documentation
and a decision record first, code second.

## Sequencing / cross-cutting risks

1. Start with #5 (docs + decisions), then #1 and #2 (mechanical value objects, no wire change);
   run `cargo fmt --check`, `clippy -D warnings` and domain tests after each step.
2. #3 and #4 last: both can alter accepted data or stored values. Land only after `docs/PARITY.md`
   DEVIATION entries and owner approval, per `AGENTS.md`.
3. Preserve serde field names and `skip_serializing_if` behaviour on every field; the RPC `BotReply`
   is a separate DTO and must stay byte-stable.
4. Do not silence lints or weaken a test to make a refactor compile; every claim here is from
   source inspection, so re-run the full suite in the implementing session before flipping status.
