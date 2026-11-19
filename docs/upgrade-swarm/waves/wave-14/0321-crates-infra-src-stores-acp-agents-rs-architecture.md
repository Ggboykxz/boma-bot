# Worker #321 — `crates/infra/src/stores/acp_agents.rs` architecture proposal (v0.4.0)

Lens: architecture (structure, layering, boundaries, error model, invariants). Read-only review; no
source was changed. Layout claims are **CONFIRMED** from a source read of `acp_agents.rs`,
`app/src/ports.rs`, `app/src/usecases.rs`, `domain/src/acp.rs`, `infra/src/tenant.rs`, the near-identical
`infra/src/stores/external_agents.rs`, and `migrations/0059_acp_agents.sql`. Impact notes are **GUESS**
until a build/test run verifies them.

## Current structure (as built)

- A single ~125-line file: `PostgresAcpAgentStore { pool: PgPool }`, an `AcpAgentRow` DTO with all
  timestamp/id fields as `String`, one `into_domain_row` converter, and three trait methods
  (`attach`, `detach`, `find`) wrapped in `tenant::in_space`. CONFIRMED.
- Layering is correct: the port lives in `app`, the domain types are pure, the executor allow-list is
  enforced in `app/usecases.rs::attach_acp_agent` before the store is called, and no credential is
  stored. CONFIRMED.
- `tenant::in_space` opens a transaction, sets transaction-local `boma.space_id`, runs one statement,
  and commits/rolls back. All three methods are single-statement and cancellation-safe; no lock is
  held across `.await`. CONFIRMED.
- The migration gives `bot_id` as PRIMARY KEY, `space_id` FK, a `CHECK` on `executor`, a `(space_id)`
  index, and FORCE RLS with a tenant policy. Parameterized SQL everywhere. CONFIRMED.
- Weak boundaries: (a) `attach` trusts the `space_id` carried inside `NewAcpAgent` and performs no
  bot↔space check at the store layer; (b) all failures collapse to `AppError::Unavailable`, and
  `into_domain_row` maps unparsable ids to `Unavailable` too; (c) `detach` ignores rows affected;
  (d) the file is a near-copy of `external_agents.rs` with subtly different upsert semantics.

## Proposed upgrades for v0.4.0

**U1 — Enforce the bot↔space invariant inside `attach`.** The FK only references `bots(id)` globally;
RLS checks the *written* `space_id` column, not the bot's real tenant. A caller passing a valid
`bot_id` from space A with `space_id = B` would write a row visible to B (the app-layer `bots.find`
in the usecase is the only guard today). Add a guard to the upsert, e.g.
`... INSERT ... SELECT $1,$2,$3,... WHERE EXISTS (SELECT 1 FROM bots WHERE id = $1 AND space_id = $2)`
and map zero affected rows to `AppError::NotFound`. Keep the allow-list check in `app`; this is
defense in depth, not a layer move.
*Risk:* one extra index lookup per write; needs a decision on error code (`NotFound` vs a typed
`Rule`). Rejecting a cross-tenant write is a defect fix, not an external behavior change for
legitimate callers, but the new error branch must be covered by a test. **GUESS** on perf impact.

**U2 — Align upsert conflict semantics with the sibling store, deliberately.** `external_agents.rs`
does `SET space_id = EXCLUDED.space_id`; this file updates only `executor`/`updated_at`. With
`bot_id` as PRIMARY KEY, the former can *move* an attachment between tenants — a latent isolation
bug — while the latter silently ignores a changed `space_id`. Write the intended ownership rule
into both stores (bot_id-keyed row must never change tenant; a re-attach only rotates executor and
`updated_at`) and make the divergence a tested, documented choice.
*Risk:* touching `external_agents.rs` widens the change beyond this module; if a real move-on-reconnect
behavior exists in the spec, the rule must accommodate it. Check `spec/01-requirements.md` before
editing. **GUESS** on spec coverage.

**U3 — Give `detach` the result semantics its contract advertises.** The `detach_acp_agent` usecase
doc promises `AppError::NotFound` when no attachment exists, but the store ignores `rows_affected`
and always returns `Ok(())`. Either return `NotFound` on zero rows or soften the usecase doc to
"idempotent no-op" — pick one and make the code, port doc, and test agree.
*Risk:* returning `NotFound` is visible at the API (`AcpAgentDisconnectedReply`) and is a DEVIATION
if callers currently rely on idempotent success; record it in `docs/PARITY.md` or choose the
no-op option for v0.4.0. **CONFIRMED** doc/code mismatch; **GUESS** on external reliance.

**U4 — Improve error fidelity without changing the public error surface.** Introduce a private
adapter that distinguishes unique/FK/RLS failures and data-corruption failures (bad stored id)
from a database outage, logging detail at `tracing` debug and mapping to the existing `AppError`
variants at the boundary. Today `tenant::unavailable` and both `map_err(|_| AppError::Unavailable)`
calls erase that distinction, so a corrupt row is indistinguishable from a down database.
*Risk:* richer mapping must never leak driver text to clients (REQ-054); keep the stable API error
format. Adds branches that need integration tests. **CONFIRMED** current collapse.

**U5 — Reduce duplication and add the missing composite index + isolation tests.** Deduplicate the
`bot_id` clone / `space_id` clone / `in_space` boilerplate shared with `external_agents.rs` behind a
small `infra` helper; consider making `attach` take `&TenantContext` like `find`/`detach` instead of
embedding `space_id` in `NewAcpAgent`. Replace `CREATE INDEX ... (space_id)` with `(space_id, bot_id)`
in a new versioned migration to serve both the RLS predicate and the point lookup. Add integration
tests (testcontainers) for: cross-tenant read blocked, cross-tenant attach rejected (U1), reattach
preserves `created_at` and rotates `executor`/`updated_at`, detach result semantics (U3), and
out-of-order `saved_at` does not move `updated_at` backwards.
*Risk:* changing the port signature ripples into mocks and usecases and is a DEVIATION if the wire
shape changes — stage the helper/index/tests first and treat the signature change as a separate ADR.
The index adds one write cost. **GUESS** on index benefit; **CONFIRMED** test gap in this review.

## Priority / verification

Order U1 → U3 → U4 → U2 → U5. Run `cargo fmt --check`, `cargo clippy -- -D warnings`, and the
infra integration tests after each step; mark each claim **CONFIRMED** only with command output.
Record any API/visible change (notably U3) in `docs/PARITY.md` before shipping; open a decision in
`docs/STATE.md` if the spec is silent on detach semantics.
