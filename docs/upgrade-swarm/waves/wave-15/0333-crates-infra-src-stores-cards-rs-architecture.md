# 0333 — `crates/infra/src/stores/cards.rs` — architecture (v0.4.0)

- Worker #333 · Lens: architecture · Target: v0.4.0 · Read-only review; no code was changed.
- Module: `crates/infra/src/stores/cards.rs` (206 lines) at `before-m5` (v0.3.0-rc5): the Postgres
  adapters for `CardStore` and `EffectStore`, re-exported from `crates/infra/src/lib.rs:67`.
- REQ-017 (`spec/01-requirements.md:137–144`): a classified tool call opens a pending card and no
  effect runs until a member approves; a replayed approval must not execute the effect twice.
- Read for context (CONFIRMED by direct reads): `crates/infra/src/stores/rows.rs` (`CardRow`,
  `EffectRow`, `card()`, `effect()`), `crates/infra/src/tenant.rs` (`with_tenant`, RLS settings,
  `is_unique_violation`), `migrations/0004_cards.sql`, `crates/domain/src/card.rs`,
  `crates/domain/src/tenancy.rs:105–113`, `crates/app/src/ports.rs:1107–1151`, :1849–1862,
  :1865–1880, :4167–4188, `crates/app/src/usecases.rs:7659–7764` (`answer_card`), and the
  wave-14/0309 proposal for house style.

## Current structure (CONFIRMED)

`PostgresCardStore` and `PostgresEffectStore` each wrap a `PgPool`; every method opens its own
transaction via `tenant::with_tenant`, so RLS (`boma.space_id`) is set from the `TenantContext` in
one place. Cards are written with a literal `'pending'` status (:94) and a client-supplied
`created_at`; `answer` is a single conditional `UPDATE ... WHERE id = $1 AND status = 'pending'`
(:133–135), so a double answer returns `None` → `AppError::Conflict` without a read-modify-write
race. `create` relies on the partial unique index `cards_thread_run_key` (migration 0004:19–21) and
maps a unique violation to `Conflict` (:108–115). `EffectStore::record` inserts into
`external_effects` with PK `(run_id, effect_key)` and maps a unique violation to `Conflict`
(:194–200), which is the entire idempotency mechanism. Both timestamps are read back through the
`boma.rfc3339()` SQL function into `String` fields, consistent with the rest of `rows.rs`.

The REQ-017 contract is assembled *across* three transactions in `answer_card` (usecases.rs:7666,
:7677, :7687–7709): `find` → `answer` → `record`. Each step is individually correct; their
composition is not atomic. The store layer exposes no combined primitive.

## Guiding principle for v0.4.0

Keep every error variant, SQL-visible field, conflict semantic and public signature byte-compatible:
`thread-answer.json` and the `tests/` suites pin this behavior, and any externally visible delta is
a DEVIATION for `docs/PARITY.md`. Improve structure by removing duplicated encodings and by giving
the store one transactionally honest primitive — not by loosening the approval gate.

## Concrete upgrades (5), each with a risk

1. **One card projection, written once.** The same nine-column SELECT with two `boma.rfc3339()`
   calls appears four times (:34–37, :55–57, :95–97, :136–138). Introduce a single
   `const CARD_COLUMNS: &str` (or a small projection macro) and use it in all four queries, so a new
   column is added in one place. *Risk:* `query_as::<_, CardRow>` matches by position, so an
   ordering mistake silently swaps fields; string-splicing SQL can break placeholder numbering.
   *Mitigation:* keep the projection as the only source of column order, add a seeded-row test that
   decodes each of the four query shapes, and run `cargo test --locked` with the cards goldens.

2. **Centralise kind/status encoding.** `create` maps `CardKind` → four string literals (:77–82)
   and `rows::card` maps them back (rows.rs:410–420); status literals are repeated in SQL (:38, :94,
   :134) and in the row mapper (rows.rs:421–430). Prefer typed `CardKind::as_str()/parse` and
   `CardStatus::as_str()/parse`, following the existing `ToolKind::parse` and
   `PendingToolCallStatus` pattern. *Risk:* `crates/domain` is another worker's scope, and the parse
   error currently reuses `UnknownEventType` — changing text or adding a variant is wire-visible.
   *Mitigation:* if the domain crate cannot be touched, keep a private codec inside `cards.rs`; add a
   round-trip test over all 4 kinds × 3 statuses and keep the existing error mapping verbatim.

3. **Offer one atomic `answer + record` store primitive.** Add a method that, in a single
   `with_tenant` transaction, performs the conditional card `UPDATE` and the effect `INSERT ... ON
   CONFLICT DO NOTHING`, returning whether the effect was newly recorded (`AnswerCardOutcome`
   already has the field). Keep `CardStore::answer`/`EffectStore::record` as delegating wrappers.
   *Risk:* port and `answer_card` changes are cross-worker; a naive `ON CONFLICT DO NOTHING` cannot
   distinguish "effect replayed" from "card missing", and mixing the two statements could weaken
   the "closed card never answers twice" rule if the order is changed. *Mitigation:* gate the UPDATE
   on `status = 'pending'` first, use `RETURNING`/`xmax` (or a `FOR UPDATE` read) to report the insert
   outcome, delegate the old methods, and replay `thread-answer.json` plus a fault-injection test
   that aborts between the two statements and asserts no half-written state.

4. **Remove the member-only space restriction on effect recording.** `record` takes its `space_id`
   from `context.space_id().ok_or(NotFound)?` (:177), so a `TenantContext::System` path — which RLS
   explicitly allows (`boma.is_system()`, migration 0004:42) — cannot record an approval effect at
   all. Resolve the space from the referenced run inside the same transaction, or add a
   `record_in_space` method taking an explicit `SpaceId`. *Risk:* a system path holding a run id from
   another tenant could record an effect in the wrong space; the run FK proves existence, not
   caller intent. *Mitigation:* keep the member fast path exactly; for system contexts require an
   explicit `SpaceId` and assert it equals the run's `space_id`; add an integration test for both
   accept and reject cases.

5. **Make `list_pending` stable before considering a cap.** It orders only by `created_at ASC`
   (:38–39), so equal timestamps yield a nondeterministic order, and the result is unbounded.
   First add `, id ASC` as a deterministic tiebreaker; treat any `LIMIT`/cursor as a separate
   decision. *Risk:* a cap can hide a pending approval from the UI (an externally visible behavior
   change / DEVIATION), and changing the port signature breaks all callers. *Mitigation:* keep the
   port signature unchanged in v0.4.0, ship only the tiebreaker now, and put any cap behind an ADR
   plus `docs/PARITY.md` and a UI "more pending" affordance.

## Cross-cutting risks / sequencing

- Sequence 1 → 2 → 5 (pure structure and determinism) → 3 → 4: build the shared projection and
  codec first, then the ordering fix, then the atomic primitive, then the system-context path.
- Freeze the port signatures, the `'pending'` → `Conflict` semantics, the effect key `approve:<card>`
  (usecases.rs:7686) and the four encoding rules; the usecase orchestration, domain types and schema
  are other workers' scope — coordinate, do not duplicate.
- `cards.rs` itself contains no tests; coverage currently lives in `crates/infra/tests/{m1,computers}.rs`
  and `tests/common/mod.rs`. Add focused store tests beside the adapter where the new seams land,
  with REQ ids in the test names, and gate each step with `cargo fmt --check`, `clippy -D warnings`,
  integration, golden and OpenAPI contract suites. No new dependency is needed for items 1–5.

## Out of scope for this module

`answer_card` orchestration and parked-tool-call application (usecases.rs), the `Card`/`CardKind`/
`CardStatus` domain model, the `migrations/0004_cards.sql` schema (including the run-less-card case
where `cards_thread_run_key` does not apply), and splitting `PostgresEffectStore` into its own
`stores/effects.rs` module (a lib.rs/mod.rs change) are left to their owners.
