# Worker #345 — External agents store architecture proposal (v0.4.0)

- Module: `crates/infra/src/stores/external_agents.rs` · Lens: architecture
- Status: proposal. CONFIRMED claims were verified this session by reading this file, `app/src/ports.rs`
  (trait at :4343), `domain/src/external.rs`, `infra/src/tenant.rs`, `migrations/0054_external_agents.sql`,
  `docs/adr/026-external-agents.md`, `app/src/usecases.rs` (:9538), `app/src/worker.rs` (:623) and
  `testkit/src/memory.rs` (:8783). GUESS items need owner confirmation.
- REQs in scope: REQ-059 (external agents), REQ-043 (tenant isolation shape), REQ-054 (error shape).

## Current structure (CONFIRMED)

`PostgresExternalAgentStore { pool: PgPool }` implements the `ExternalAgentStore` port with three
methods over one table, `bot_external_agents` (migration 0054). One row per bot, `bot_id` a global
PRIMARY KEY with FK to `bots`; `base_url` NOT NULL, optional `agent_name`/`secret_name` (a vault
reference, never a bearer), RLS FORCE with `space_id = boma.current_space_id() OR is_system()`.
Every method wraps one `tenant::in_space`/`with_tenant` transaction, so RLS is correctly driven.
`connect` is an upsert; `disconnect`/`find` take a `TenantContext` and derive `space_id` from it,
but **`connect` does not** — it takes `space_id` from `NewExternalAgent`. Timestamps cross the
boundary as `String` cast with `$6::timestamptz` and rendered by `boma.rfc3339`.

## Structure target

Keep one file (~140 lines; no submodule split — match the repo's one-store-per-file convention).
Add two private helpers so upgrades stay local: `fn space_of(context) -> Result<&SpaceId, AppError>`
(single place that rejects `TenantContext::System`), and `fn map_write_error(sqlx::Error)
-> AppError` (constraint/FK → stable domain error, connection/timeout → `Unavailable`, reusing
`tenant::is_unique_violation`). No new dependency; `chrono` only if upgrade 5 is taken.

## Concrete upgrades

### 1. Tenant-scope `connect` through the port, not through the config (REQ-059/043) — do first
CONFIRMED: `connect(config: NewExternalAgent)` is the only port method without a `TenantContext`;
`app/src/usecases.rs:9568` fills `config.space_id` from `context.space_id()`, so the current call path
is safe, but the port itself lets any caller name any space, and `tenant::in_space` will happily set
RLS to that space. `AcpAgentStore` shares the flaw. Change the port to take `&TenantContext` and
derive `space_id` from it inside `space_of`; drop `space_id` from `NewExternalAgent` if it becomes
redundant. Risks: ripples to `usecases.rs`, `testkit/src/memory.rs`, `api` tests and the in-memory
implementor (all must be updated in one commit); internal port change, not HTTP-visible, but record
in `docs/PARITY.md` if any wire DTO shifts; add `req_059_connect_uses_caller_tenant` test.

### 2. Stop re-parenting rows in the upsert (REQ-059/043)
CONFIRMED: `ON CONFLICT (bot_id) DO UPDATE SET space_id = EXCLUDED.space_id`. Because `bot_id` is a
global PRIMARY KEY, a second connect for the same bot from another space would rewrite `space_id`
and move the row across tenants (RLS permits it, since the setting equals the new space). Remove
`space_id` from the `SET` list; assert the existing row's space matches, or scope conflicts with a
`(bot_id, space_id)` unique key. Risks: GUESS — spec/ADR-026 are silent on moving a bot between
spaces; if re-parenting was intentional, the owner must bless it and an ADR + migration are needed.
Needs `req_059_connect_cannot_move_bot_between_spaces` test.

### 3. Align Postgres and in-memory disconnect/find semantics (test parity)
CONFIRMED: memory `disconnect` returns `NotFound` for an absent row and `Forbidden` when the context
has no space; Postgres `disconnect` returns `Ok(())` after deleting zero rows and `NotFound` for a
missing space. `find` likewise differs on the no-space case. Tests driven through the memory store
therefore pass where production behaves differently. Pick one contract (recommend: idempotent delete
for both; `NotFound` for no space in both) and implement it in both adapters. Risks: changing an
error variant is externally observable through the API mapping — log any shift in `docs/PARITY.md`;
grep all callers of `external_agents.disconnect` before choosing; add paired REQ-named tests against
both stores.

### 4. Classify errors instead of blanket `Unavailable` (REQ-054)
CONFIRMED: `into_domain_row` maps an invalid stored `BotId`/`SpaceId` to `AppError::Unavailable`, and
all SQL errors map through `tenant::unavailable`. `AppError::is_retryable()` is true only for
`Unavailable`, so a corrupt row is reported as a transient failure that retries can never fix, and a
CHECK/FK violation is indistinguishable from a dead database. Route writes through `map_write_error`
and give corrupt rows a non-retryable server-fault error. Risks: GUESS — no internal/corrupt variant
exists today (`error.rs`), so adding one changes the stable error envelope and its HTTP mapping
(needs an ADR and owner approval); never log `base_url` or `secret_name` in the error path; add a
unit test per variant.

### 5. Make the row boundary type-safe and dedupe context handling (low priority)
CONFIRMED: `ExternalAgentRow` stores timestamps as `String`, relying on a `::timestamptz` cast on
input and `boma.rfc3339(...)` on output; `disconnect`/`find` each repeat
`context.space_id().cloned().ok_or(AppError::NotFound)?` plus an extra clone (`for_delete`,
`for_find`). Use `chrono::DateTime<Utc>` fields in the row and render RFC 3339 only when building
the domain value; use `space_of` in both methods. Risks: adds a `chrono` dependency (per AGENTS.md,
`cargo add` + docs.rs check + `docs/DEPENDENCIES.md` entry first); confirm `boma.rfc3339`'s exact
format before changing it, or golden/API tests will drift.

## Cross-cutting risks

- Upgrades 1–3 change observable error/tenant behavior: record every shift in `docs/PARITY.md` and
  wait for approval before shipping (AGENTS.md); they are candidate DEVIATIONS, not silent fixes.
- Keep `sqlx::query` (runtime-checked) as used repo-wide; a move to `query!` belongs in a workspace
  ADR, not this wave.
- RLS already enforces isolation, so these are defense-in-depth/consistency fixes — do not weaken
  the `boma.space_id` transaction discipline in `tenant.rs` while refactoring.
