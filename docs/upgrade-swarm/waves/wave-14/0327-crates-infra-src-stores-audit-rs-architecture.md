# 0327 — `crates/infra/src/stores/audit.rs` — architecture (v0.4.0)

- Worker #327 · Lens: architecture · Target: v0.4.0 · Read-only review; no code was changed.
- Module: `crates/infra/src/stores/audit.rs` (123 lines) at `before-m5` (v0.3.0-rc5): the Postgres
  adapter for the `AuditStore` port.
- Read for context (CONFIRMED by direct reads): `spec/01-requirements.md` REQ-045 / REQ-058,
  `crates/app/src/ports.rs` :630–649 (`AuditEventView`) and :2239–2286 (`NewAuditEvent`,
  `AuditStore`), `crates/domain/src/audit.rs` (`AuditKind`, redaction), `crates/infra/src/tenant.rs`
  (`with_tenant` / `in_space` / `as_system`), migrations `0030`, `0045`, `0049`, `0052`,
  `crates/app/src/usecases.rs` :8477–8549 (`security_events_list`, `audit_export`), `AGENTS.md`.

## Current structure (CONFIRMED)

One struct, `PostgresAuditStore { pool }` (:16–26), with a `const fn new`. The `AuditStore` impl has
exactly two methods. `list_for_space` (:30–72) builds a `LIKE "{prefix}%"` string, runs one
`sqlx::query_as` with a 7-element tuple `(String, Option<String>, Option<String>, String, String,
Value, String)`, and maps positionally into `AuditEventView` (:56–71). `append` (:74–122) converts
newtypes to owned strings, **derives its own tenant context from the event's fields** (:88–101:
member when both space and actor are present, otherwise `System(ScheduledJob)`), then runs an INSERT
through `with_tenant`. Both paths map every driver error to `AppError::Unavailable` via
`tenant::unavailable`; the module logs nothing itself.

Two structural facts drive this proposal:
- `list_for_space` **ignores its `space_id` argument** (`_space_id`, :33). Scope comes solely from
  `context` plus RLS. Reading `tenant.rs` :95–103 and migration `0045` :38–45, a `System` context
  sets `is_system()` and therefore sees **every** space's rows; with `space_id` unused, a
  system-context call is an unbounded cross-space read (GUESS: reasoned from code, not executed).
  This is the REQ-058 "ordinary members cannot reach admin data" boundary at the store level.
- The allowed `kind` set has two sources of truth: `AuditKind::as_str()` in `domain` and a literal
  SQL `CHECK` that migrations `0045`, `0049`, `0052` repeatedly drop and rewrite by hand.

## Guiding principle for v0.4.0

Make the two boundaries explicit and single-sourced: which **space** a read may see, which **actor or
reason** a write carries, and which **kinds** exist. Keep the port signatures, the `security.`
prefix token, the `limit` caps (200 / 4_000), the `ORDER BY created_at DESC, id DESC` tiebreaker and
all error text stable — these are load-bearing for REQ-045 goldens; any externally visible change is
a DEVIATION for `docs/PARITY.md` (AGENTS.md).

## Concrete upgrades (5), each with a risk

1. **Make reads space-bound, not context-bound.** Use `tenant::in_space(&pool, space_id, …)` with the
   argument currently discarded (:33), and add a debug assertion that a `Member` context's space
   equals `space_id`. A `System` caller then cannot enumerate other spaces through this method.
   *Risk:* a caller that legitimately passes a `System` context to read deployment rows would start
   seeing nothing (deployment rows have `space_id IS NULL`). *Mitigation:* CONFIRMED the two call
   sites (`usecases.rs` :8514, :8539) pass a member context and pass the same space as the context;
   add a REQ-058 integration test asserting a system-context `list_for_space` cannot return another
   space's row, and keep the read path on member contexts only.

2. **Replace the 7-tuple row with a typed DTO.** Add a private `#[derive(sqlx::FromRow)]`
   `AuditEventRow` (here, or in `stores/rows.rs` — coordinate with that worker) with named fields
   and `impl From<AuditEventRow> for AuditEventView`; keep `boma.rfc3339(created_at) AS created_at`.
   *Risk:* `FromRow` requires exact column aliases, so a typo silently breaks every read at runtime
   rather than at compile time (the current tuple is positional but checked). *Mitigation:* alias all
   seven columns explicitly in the SELECT, add one row-mapping unit test and replay the REQ-045
   export goldens before and after.

3. **Make write scope an explicit value instead of inferring it from absent fields.** Extract the
   match at :88–101 into a private `fn context_for(&NewAuditEvent) -> TenantContext` with unit tests
   now; propose that v0.4.0 move the choice onto `NewAuditEvent` (e.g.
   `AuditScope::Space { actor } | AuditScope::Deployment { reason }`) so an actorless event no longer
   silently becomes `ScheduledJob`. *Risk:* `NewAuditEvent` lives in the `app` crate (another
   worker's scope) and has ~20 call sites; changing it is a cross-crate API change. *Mitigation:*
   land the extraction + tests in this module only, then hand the typed-scope change to the port
   worker as a coordinated item; preserve `ScheduledJob` as the compatibility default.

4. **Make `append` idempotent and distinguish a duplicate from an outage.** Today a repeated event
   id violates the PK and becomes a generic `AppError::Unavailable` (:104–118), so a retry after a
   timeout looks like a database failure. Use `ON CONFLICT (id) DO NOTHING` (or map the unique
   violation to a distinct typed error) using the existing `tenant::is_unique_violation` helper.
   *Risk:* `DO NOTHING` could hide two genuinely different events that collide on id. *Mitigation:*
   ids are opaque caller-generated values (GUESS from the ~20 call sites); add tests that a repeated
   append leaves exactly one row and returns `Ok`, and that two distinct ids both persist.

5. **Remove the kind-set drift between `AuditKind` and the SQL `CHECK`.** Add a test that derives
   every `AuditKind::as_str()` and asserts it is accepted by the live table (or generate the CHECK
   from the enum in a migration), and centralise the `security.` prefix token beside the enum so
   the filter and the kinds cannot diverge. *Risk:* rewriting the CHECK touches a constraint shared
   by every append path; a missed literal would reject a currently valid event. *Mitigation:* keep
   the existing literal list verbatim in v0.4.0 and only add the verification test first; note that
   `kind LIKE $1` treats `%`/`_` as wildcards, but CONFIRMED both callers pass fixed constants.

## Cross-cutting risks / sequencing

- Sequence 1 → 2 → 5 → 4 → 3: fix the read boundary first (security), then the mapping safety net,
  then the kind contract, then idempotency, then the cross-crate scope type last.
- The `AuditStore` port, `NewAuditEvent`, `AuditEventView` and the RLS policies are other workers'
  scope: coordinate items 1/3/5 rather than duplicating edits. No new dependency is needed.
- Gate each step with `cargo fmt --check`, `clippy -D warnings`, REQ-045/REQ-058 integration tests,
  the export goldens and `tools/trace_check.py` (REQ id in test names).

## Out of scope for this module

The export/JSONL and redaction logic (`usecases.rs`, `domain/src/audit.rs`), the admin role and grant
checks that produce the REQ-058 events, the migration files themselves, and the RLS policies are
other modules. This proposal only restructures the audit store adapter.
