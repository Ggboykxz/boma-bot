# 0421 — `migrations/0004_cards.sql` — architecture (v0.4.0)

- Worker #421 · Lens: architecture (structure, boundaries, coupling, debt) · Target: v0.4.0.
- Status: PROPOSAL (read-only review; no product code or migration changed). CONFIRMED = verified by
  direct read of this worktree; GUESS = needs a test/oracle run.
- Module: `migrations/0004_cards.sql` (49 lines), the M2 schema for approval/question cards and the
  exactly-once effects they authorise (REQ-007, REQ-017, REQ-018).
- Read for context (CONFIRMED): `migrations/0001_init.sql`, `0055_signin_requests.sql` (later-house
  convention), `crates/infra/src/stores/cards.rs`, `crates/infra/src/stores/rows.rs`,
  `crates/app/src/ports.rs`, `crates/app/src/usecases.rs:7659–7764` (`answer_card`), and the
  wave-15/0333 and 0334 proposals for house style and cross-layer seams.

## Current structure (CONFIRMED)

One file owns two unrelated aggregates: `cards` (mutable question/approval lifecycle; status text,
`answered_at`) and `external_effects` (append-only idempotency ledger; PK `(run_id, effect_key)`,
`applied_at`). Three architectural traits stand out:

1. **Vocabulary lives outside the DB.** `kind` and `status` are unconstrained `text` (:12–13); the
   only validators are the Rust codecs in `rows::card` / `cards::create`. The DB cannot reject an
   invalid state, and the wire error reuses `UnknownEventType`.
2. **The exactly-once invariant is half in the schema.** `cards_thread_run_key` is a partial unique
   index gated on `run_id IS NOT NULL AND status = 'pending'` (:19–21), so the dedup boundary simply
   does not exist for run-less cards; the ledger's key is sound but its "recorded vs applied" state
   is a single nullable timestamp (:29).
3. **Tenant isolation is hidden in dynamic SQL.** A `DO $$ … FOREACH … EXECUTE format(...)` loop
   (:34–49) enables/forces RLS and names policies at runtime. Unlike the static
   `ENABLE`/`FORCE`/`CREATE POLICY` in `0055_signin_requests.sql:23–29`, the effective policy is not
   greppable and is invisible to SQLx offline metadata and to reviewers.

Also: `created_at` has no DB default (:15), so ordering relies on the caller's clock, while
`answered_at = now()` uses the DB clock — two time sources in one aggregate.

## Guiding principle for v0.4.0

`0004` is immutable: every change ships as a NEW, additive migration (e.g. `0060_cards_vocabulary.sql`,
`0061_effects_outbox.sql`) that only adds objects, nullable/defaulted columns or indexes, never
drops or rewrites an existing row. Keep all wire-visible behavior, error text, effect keys
(`approve:<card>`, `tool:<run>:<call>`) and RLS semantics byte-compatible; anything externally
visible is a DEVIATION for `docs/PARITY.md`.

## Concrete upgrades (5), each with a risk

1. **Give the two aggregates separate ownership.** Add a header comment block to the *new* migrations
   that states the boundary (cards = conversational state machine; effects = idempotency ledger) and
   put each future change in its own numbered file; leave `0004` byte-identical. *Risk:* a shared
   `FOREACH tenant_table IN ARRAY['cards','external_effects']` still couples both tables' RLS in
   `0004`. *Mitigation:* document the loop as legacy, use explicit per-table policies for every new
   tenant table, and add a migration-lint check that no new migration reintroduces the array loop.

2. **DB-enforced vocabulary via additive CHECK constraints.** In a new migration, add
   `CONSTRAINT cards_kind_valid CHECK (kind IN ('approval','question','purchase','takeover'))` and
   `cards_status_valid CHECK (status IN ('pending','answered','declined'))`, mirroring
   `signin_requests_status_valid` (`0055:16`). *Risk:* any existing row with an out-of-set value makes
   the migration fail, and a new Rust enum variant then needs a schema change (coupling). *Mitigation:*
   run a pre-flight `SELECT kind,status FROM cards WHERE … NOT IN (...)` audit first (GUESS: none today),
   keep the constraint names explicit, and make "add a kind" a documented paired Rust+SQL step.

3. **Complete the idempotency boundary in the schema, additively.** Add a second partial unique index
   closing the run-less gap, scoped to approval-like kinds so concurrent question cards still work,
   e.g. `UNIQUE (thread_id, kind) WHERE run_id IS NULL AND status='pending' AND kind IN ('approval','purchase')`.
   *Risk:* it can reject legitimate concurrent approvals if the kind set is wrong, and REQ-007
   deliberately allows multiple chat cards. *Mitigation:* restrict to the two effect-bearing kinds,
   add an ADR, and prove with a two-card concurrency test plus a replay of `thread-answer.json`.

4. **Model effect state instead of a single nullable `applied_at`.** Add an additive sibling
   `effect_outbox` (or nullable `state`/`attempts` columns) so "intent recorded" and "effect executed"
   are distinct, post-commit states; the existing `external_effects` row and its PK stay as the
   dedup key. *Risk:* duplicate ledger/outbox concepts and coordination with `EffectStore` (another
   worker's scope); a wrong backfill can mark old effects unapplied and re-execute them.
   *Mitigation:* keep the old key authoritative, default new state to "applied" for existing rows,
   land port/mock changes together via ADR, and never hold the DB transaction across the tool call.

5. **Make RLS governance introspectable without removing the loop.** Add a migration-time assertion
   (a `DO $$` that raises unless `pg_policies` shows `cards_tenant_isolation` and
   `external_effects_tenant_isolation` with `using = with check`) plus a test that queries
   `pg_policies` / `pg_class.relrowsecurity`, and document the effective policies in a companion
   comment. *Risk:* `pg_policies` columns vary across Postgres versions and a test that mirrors the
   loop can drift from it. *Mitigation:* pin to the embedded PG 17 used by the packs, assert only
   `policy name + enforced RLS`, and derive both checks from one shared expected-policy constant.

## Cross-cutting risks / sequencing

- Sequence 1 → 2 → 3 → 5 → 4: settle ownership and vocabulary first, then the dedup boundary and RLS
  assertions, then the outbox (needs port coordination). All ship as separate additive migrations.
- Regenerate SQLx offline metadata when columns/indexes are added; CI must run `migrate` against a
  fresh DB and replay the cards goldens. Freeze effect keys, conflict semantics and RLS expressions.
- No new dependency; the work is SQL + migration-level tests. Mark every claim CONFIRMED/GUESS per
  `AGENTS.md`, and gate each step with `cargo fmt --check`, clippy `-D warnings`, golden and contract suites.

## Out of scope

`answer_card` orchestration and pending-tool application (`usecases.rs`), the `Card`/`CardKind`/
`CardStatus` domain types, the `CardStore`/`EffectStore` ports and their in-memory doubles, and any
in-place edit of `0004` are left to their owners. Each belongs to a different brief in this swarm.
