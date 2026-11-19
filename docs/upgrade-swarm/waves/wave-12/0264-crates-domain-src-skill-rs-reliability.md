# Worker 0264 — `crates/domain/src/skill.rs` reliability proposal (v0.4.0)

Module: `crates/domain/src/skill.rs` (182 lines). Lens: reliability (invalid definitions, name
collisions, install failures). Evidence: `skill.rs`, `error.rs`, `plugin.rs`, `app/ports.rs`,
`app/usecases.rs`, `infra/stores/skills.rs` and `infra/stores/plugin_installs.rs` were read in
`before-m5` on 2026-11-19. CONFIRMED = read here; GUESS = needs a test/runtime check. Read-only
task; this document is the only write.

## 1. What the module does (CONFIRMED)

- Pure data types only: `SkillOrigin` (Builtin/Taught/Plugin, `as_str`), `Skill` (5 fields, all
  `pub`, no constructor/invariants, :36-49), and three free validators (:65-111) that all return
  `DomainError::InvalidName` (error.rs:31,77 prints the reason verbatim).
- `builtin_skills()` allocates a `Vec` with one long instruction body on every call (:119-130);
  callers use it as the reserved-name set (`usecases.rs:492,534,1169`).
- Four happy-path tests carry `req_035_*` (:139-181). No proptest and no whole-`Skill` validation.

## 2. Failure modes

### 2.1 Validators are advisory, not enforced (CONFIRMED)

`Skill` fields are public with no constructor, so an invalid name, empty instructions, or an
impossible origin/source pair is fully constructible. The builtin is validated only in a test
(:173-181). "Make invalid states unrepresentable" is not met; correctness depends on every caller
remembering to validate first.

### 2.2 Raw name echoed into errors/logs (CONFIRMED code, impact GUESS)

`validate_skill_name` formats the *raw, untruncated* `name` into its error (:77-80); `InvalidName`
`Display` prints it verbatim (error.rs:77). An invalid name may be arbitrarily long (the length
check is only part of the `valid` flag) and may contain newlines/control characters: a 10 KiB
CRLF-laden name becomes a 10 KiB error and log line — log/forging injection. `req_040`
(error.rs:138-144) already establishes the opposite rule for personal data.

### 2.3 Validation/store mismatch — whitespace bypass (CONFIRMED)

`validate_skill_instructions` checks `trimmed.len()` (:104-105) but `teach_skill` stores the
**untrimmed** body (`instructions.to_owned()`, usecases.rs:549). Description is trimmed on store
(usecases.rs:547) but instructions are not: ~32 KiB of padding plus one character passes and is
stored in full — storage/prompt bloat. The two validators also mix units (description `chars()`,
name/instructions bytes), a latent drift as limits evolve.

### 2.4 Collision enforcement lives outside the domain (CONFIRMED)

The doc promises a skill "can never shadow a built-in" (:5) but the check is duplicated in the app
layer (`usecases.rs:534-539` and `:1169-1175`) via `builtin_skills().any(...)`. Any new caller using
`SkillStore::put` directly bypasses it; the two copies can drift. Worse, `put` is an UPSERT
(`ON CONFLICT (space_id, name) DO UPDATE`, skills.rs:104-109), so teaching over an existing
non-builtin name silently overwrites it and can flip its `origin` (Taught↔Plugin) — silent data
loss / provenance confusion, not a conflict.

### 2.5 Origin ↔ `plugin_source` not tied (CONFIRMED)

Nothing enforces `origin == Plugin ⇔ plugin_source = Some(canonical source)`. A `Skill` can carry a
Plugin origin with no source, or a Taught origin with one. `remove_plugin_skills` deletes by
`plugin_source = $1` (plugin_installs.rs:115), so a missing/malformed source orphans skills on
reinstall/uninstall, leaving stale instructions in the model loop.

### 2.6 Install replace is non-atomic (CONFIRMED at call sites; fix outside scope)

`marketplace_install` calls `remove_plugin_skills` **before** the per-skill `teach_skill` loop
(usecases.rs:1343-1355), with no transaction. A validation failure or store error mid-loop leaves
old skills removed and only part of the new set installed. The domain cannot make this atomic, but
it can offer one pure "validate the whole definition set" entry point used before any mutation.

### 2.7 Thin test surface (CONFIRMED)

No proptest; no tests for padding bypass (2.3), control-char echo (2.2), origin/source pairing,
64-byte name boundary, or duplicate names in a growing `builtin_skills()` list.

## 3. Concrete hardening for v0.4.0 (ordered by value/risk)

- **H1 — Whole-skill validator (low risk, highest value).** Add pure `validate_skill(&Skill)` calling
  all three field validators plus origin/source consistency (2.5) and reserved-name check (2.4).
  Additive; call it at the adapter boundary first. Tests `req_035_*` for each inconsistent pair.
- **H2 — Sanitise the name error (low-medium).** Cap the echoed name (~64 bytes), strip control
  characters before building `InvalidName`, keeping the variant/wire shape. Add a test that a 10 KiB
  CRLF name cannot echo into a log line. A full dedicated error variant is a separate DEVIATION
  (below).
- **H3 — Close the whitespace bypass (low).** Make the validator check the exact bytes that are
  stored: trim before the length check *and* store the trimmed body (or reject padding outright).
  Align description/instructions units. Requires an app-layer edit (out of this write scope) —
  propose, do not edit.
- **H4 — Reserved names in the domain (low).** Add `const BUILTIN_SKILL_NAMES: &[&str]` and pure
  `is_reserved_skill_name(&str)`/`is_builtin_skill(&str)`; make app call sites use it and make
  `builtin_skills()` derive from the same const. Removes duplicated logic and the per-call alloc.
- **H5 — Constructor / newtype (medium).** Add `Skill::new(...) -> Result<Skill, DomainError>` so
  valid construction is the default and 2.5/2.4 are enforced once. Keep fields `pub` initially and
  migrate callers (domain/app/infra/testkit) in a later step to limit blast radius.
- **H6 — Validate `plugin_source` field (medium).** Require `origin == Plugin ⇔ plugin_source =
  Some(canonical)`, checked against `parse_plugin_source(...).canonical()` (plugin.rs:29-41) so
  removal-by-source matches. Roll out as validator + metric first; stored non-canonical values may
  otherwise be rejected.
- **H7 — Proptests (low).** Name grammar never yields `/`, `.`, control chars or traversal; bounded
  lengths; validator idempotence; `builtin_skills()` names are unique and all reserved. Dev-only
  dependency (proptest), `req_035_*` names.

## 4. Risks and gates

- H2's fuller error taxonomy and H3's trimming change externally visible strings/stored bytes:
  record any change in `docs/PARITY.md` as a DEVIATION and wait for owner approval (AGENTS.md).
- H5/H6 touch construction and stored provenance across four crates — stage additively; never break
  `pub` fields or reject existing rows silently. H6 may surface orphaned legacy skills: metric first.
- Domain is pure (no IO, clock, async): all hardening stays pure. No new runtime deps; if proptest
  is added use `cargo add --dev` and read docs.rs. Keep `#![deny(missing_docs)]`, `clippy -D warnings`,
  and no `unwrap`/`expect`/`panic` outside tests.
- `error.rs`, app and infra fixes are outside this write scope: propose only. Collision/atomicity
  (2.4, 2.6) ultimately need a store/usecase transaction and an explicit replace-vs-conflict policy.
- Traceability: `tools/trace_check.py` counts declarations, not comments — put `REQ-035` in test
  names. Mark log-injection impact (2.2) GUESS until reproduced against the running original; never
  point verification at third-party hosts.

## 5. Verification for this wave

1. `cargo fmt --check` and `cargo clippy -p boma-domain --all-targets -- -D warnings`.
2. `cargo test -p boma-domain` with new `req_035_*` tests and H7 proptests.
3. Differential: assert a 10 KiB CRLF-laden invalid name yields a bounded, control-free error and
   that a padded instruction body is either rejected or stored trimmed; record as CONFIRMED only
   after running them.
