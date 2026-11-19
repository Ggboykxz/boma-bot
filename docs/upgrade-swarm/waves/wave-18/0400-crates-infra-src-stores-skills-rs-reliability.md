# v0.4.0 reliability proposal — `crates/infra/src/stores/skills.rs`

- Worker #400 · Lens: reliability · Module: `PostgresSkillStore` (142 lines, `before-m5`, REQ-035).
- Status: PROPOSAL ONLY. Code facts are CONFIRMED by reading this file plus `ports.rs:2098–2196`, `tenant.rs:41–163`, `usecases.rs:458–555,1328–1365`, `domain/skill.rs`, `testkit/memory.rs:5124–5220`, `migrations/0028_skills.sql`, `0035_plugin_installs.sql` and `infra/tests/m1.rs:1890–1969`. GUESS = needs a runtime/differential check. No tests were run; nothing outside this document was written.

## 1. Current behaviour (CONFIRMED)

- `list`/`get`/`remove` use `tenant::with_tenant` + RLS only; only `put` extracts `space_id` and rejects `TenantContext::System` (:93–97). A System `remove(name)` therefore deletes that name in **every** space; System `list`/`get` read across spaces.
- `put` is `INSERT … ON CONFLICT (space_id,name) DO UPDATE` (:102–109): it silently overwrites any existing row and flips `origin`/`plugin_source`, and never returns `AppError::Conflict` although the port promises it (`ports.rs:2186–2188`). `created_at` is absent from the `SET` list, so a re-teach keeps the first value while the caller reports a new one; `id` is likewise immutable after first insert.
- `remove` is `DELETE FROM skills WHERE name = $1` (:132): no explicit `space_id` beyond RLS, no origin check, so it can delete a plugin skill behind the install ledger's back; `rows_affected()>0` is the only signal.
- Every SQL error, including PK/unique violations and the `$8::timestamptz` cast of caller-supplied `created_at` (:103), goes through `tenant::unavailable` → `Unavailable` (503). `tenant::is_unique_violation` exists (:159) but is unused here.
- The only test (`m1.rs:1892`) covers round trip, cross-space RLS and taught-over-taught replacement; it does not cover collisions, orphans, invalid input or System context. No `tracing` span, no `LIMIT`, no statement timeout in the adapter.

## 2. Failure modes

### Name collisions
- **C1 — Cross-origin silent overwrite (HIGH, CONFIRMED).** `put` upserts regardless of `origin`. A plugin install whose skill name matches a member's taught skill overwrites it (`usecases.rs:1351`); a later uninstall deletes it via `remove_plugin_skills` — user data lost. The reverse (teach over a plugin row) flips provenance and drops `plugin_source`.
- **C2 — Two plugins, one name (HIGH, CONFIRMED).** No plugin-vs-plugin check on any write path; the second install wins, the first install's ledger still lists the skill, and uninstalling the winner deletes the row the loser expects. The port's Conflict contract is unenforced in **both** adapters (correction to sibling 0399: `MemorySkillStore::put` at `memory.rs:5185–5196` is also an unconditional overwrite).
- **C3 — Built-in shadowing is app-layer only (MED, CONFIRMED).** The reserved-name check lives in `teach_skill` (`usecases.rs:534–539`), not in the store or DB. A future direct `SkillStore::put` can persist `interrogate`; `skills_list` (:467–479) then advertises two entries of that name while `skill_get` always returns the built-in (:492–504), leaving an unreachable row in the tool loop.
- **C4 — `id` collision misreported (MED, CONFIRMED).** Two names sharing one `id` violate the PK; `unavailable` turns it into a 503, inviting retries instead of a Conflict.

### Orphan skills
- **O1 — Source-keyed revoke leaves orphans (HIGH, CONFIRMED).** `remove_plugin_skills` deletes only `WHERE origin='plugin' AND plugin_source=$1` (`plugin_installs.rs:104`). A row with origin=plugin and NULL/non-canonical `plugin_source` (reachable via direct `put` or canonicalisation drift) is never revoked and keeps feeding instructions to the model, with no ledger row to name it.
- **O2 — Ledger/skill divergence (MED, CONFIRMED).** Each install step is its own transaction (sibling 0374/F1); a mid-loop failure leaves a partial skill set and a stale `parts` count. Nothing reconciles `skills` against `plugin_installs` at read time.
- **O3 — `remove` has no ownership check (MED, CONFIRMED).** It deletes any name, including plugin skills, without touching `plugin_installs`; under a System context the delete is global, leaving orphan ledger rows.

### Invalid definitions
- **I1 — Store persists unvalidated fields (HIGH, CONFIRMED).** The DB CHECK covers only `origin`; name/description/instructions have no constraint. Validation is only in `teach_skill` (`usecases.rs:531–533`), so a direct `put` can store an empty/oversized/control-char name or multi-MB instructions that later reach the model prompt (bloat / injection surface). Empty instructions make `skill.load` useless.
- **I2 — Untrusted timestamp cast (MED, CONFIRMED).** `$8::timestamptz` (:103) raises a driver error on malformed `created_at`, mapped to 503: a client input bug looks like an outage. Reads also depend on the DB function `boma.rfc3339` existing in every environment.
- **I3 — Origin/source inconsistency (MED, CONFIRMED).** Nothing enforces `origin='plugin' ⇔ plugin_source IS NOT NULL` at the store; `put` writes plugin-without-source or taught-with-source, breaking O1's revoke key.

## 3. Concrete hardening for v0.4.0 (ordered by value/risk)

- **H1 — Scope check in every method + classify errors (LOW–MED).** Extract `space_id` once; add `AND space_id = $n` to `list`/`get`/`remove`; return `NotFound`/empty for System as `put` and the memory adapter do. Map unique/PK violations to `Conflict` via `tenant::is_unique_violation`, cast/CHECK failures to `Rule`, keep driver detail at debug (REQ-054). *Risk:* the redundant predicate must use the same id; a wrong branch turns an outage into a client fault. Covers C4, I2, O3.
- **H2 — Origin-aware conditional upsert (MED).** Inside the existing transaction, add `… DO UPDATE SET … WHERE skills.origin = EXCLUDED.origin OR skills.origin <> 'plugin'` (a same-source reinstall still replaces, because the row was revoked first) and map `rows_affected()==0` to `Conflict`. Prefer the SQL condition over `SELECT … FOR UPDATE` to avoid a racy check-then-insert. *Risk:* externally visible error change → `docs/PARITY.md` DEVIATION and owner approval; add two-adapter conformance tests first. Covers C1/C2.
- **H3 — Revalidate at the store boundary (LOW–MED).** Call the domain validators on `NewSkill` inside `put`, reject built-in names there too, and assert the origin/source pairing; keep `teach_skill`'s checks as the friendly API layer. *Risk:* duplicate checks can drift — prefer one shared pure `validate_skill`-style entry point from `domain/skill.rs` (coordinate with worker 0264). Covers C3, I1, I3.
- **H4 — Guard the revoke key (MED).** For origin=plugin require a non-empty canonical `plugin_source`; add a backfill + metric for existing NULL/non-canonical rows before enforcing so legacy orphans are surfaced, not silently rejected. *Risk:* rejects previously accepted writes → DEVIATION-gated; the backfill must not touch unrelated rows. Covers O1/O3.
- **H5 — Restore timestamps, bound reads, add spans (LOW).** Put `created_at = EXCLUDED.created_at` in the `SET` list (or document immutability), select raw `timestamptz` and format with one Rust helper, add a generous `LIMIT`, and add one `tracing::instrument` per method logging only `space_id`/`name`/outcome — never `instructions` or credential-bearing sources. *Risk:* formatting must be byte-identical to keep wire/golden tests stable; a `LIMIT` that shrinks results is a DEVIATION. Covers I2 + observability.

## 4. Tests and gates

- New REQ-035-named infra tests (both adapters where possible): `req_035_put_plugin_over_taught_is_conflict`, `req_035_two_plugins_share_a_name_is_conflict`, `req_035_builtin_name_cannot_be_put_directly`, `req_035_null_plugin_source_orphan_is_refused`, `req_035_malformed_created_at_is_rule_not_unavailable`, `req_035_system_remove_does_not_cross_spaces`, `req_035_invalid_definition_is_rejected_at_the_store`.
- Sequence H1 → H3 → H5 → H4 → H2 (contract/error changes last, DEVIATION-gated). Each step: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test -p boma-infra`, `tools/trace_check.py`.
- Freeze the public port signature, JSON names, ORDER BY and error text until approval; coordinate with 0399 (architecture), 0264 (domain validators) and 0374 (install atomicity), which own adjacent changes. This proposal adds no product feature.

Recommendation: ship H1 + H5 first (no visible change); treat H2–H4 as DEVIATION-gated, with H2 the highest-value fix against silent data loss.
