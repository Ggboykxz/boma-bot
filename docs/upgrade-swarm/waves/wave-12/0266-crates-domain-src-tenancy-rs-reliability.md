# tenancy.rs — reliability hardening proposal (v0.4.0)

Worker #266 · MODULE `crates/domain/src/tenancy.rs` · LENS reliability · READ-ONLY audit.
No source was modified. Evidence: this file, `ids.rs`, `error.rs`, `infra/src/tenant.rs`,
`migrations/0002_m1.sql`, `infra/tests/policy.rs` and store call sites, all read this session.
"Current state" items are CONFIRMED; proposed behaviour/impact is marked GUESS where inferred.

## Current state (CONFIRMED)

- `SystemReason` is a closed 8-variant enum; `ALL: [Self; 8]` is a hand-maintained fixed array;
  `as_str()`/`parse()` use exhaustive matches. `parse` is exact and case-sensitive (no trim),
  returning `DomainError::UnknownSystemReason(other)` — D-09's free-text hatch is closed.
- `TenantContext::Member { space_id: SpaceId, user_id: UserId }` is fully scoped.
  `TenantContext::System { reason: SystemReason, user_id: Option<UserId> }` carries **no space**.
- Accessors: `space_id()` is `Some` only for `Member`, `None` for `System`; `is_system()` is
  `matches!(System)`; `system_reason()` is `Some` only for `System`.
- At the DB layer (`infra/src/tenant.rs::scope`) every transaction sets `boma.space_id` and
  `boma.system_reason` with `is_local = true` and rolls back on error. RLS in `0002_m1.sql` is
  `USING (space_id = boma.current_space_id() OR boma.is_system())`, where `is_system()` is just
  `current_setting('boma.system_reason') <> ''`.
- Many stores reject system contexts defensively (`space_id().ok_or(NotFound)` or an explicit
  `System => Err(NotFound)` match), and `infra/tests/policy.rs` greps that every `SystemReason::`
  call site names a known variant. Enforcement is therefore per-call-site, not in the type.
- `SpaceId`/`UserId` (`ids.rs`) reject only blank strings; `#[serde(transparent)]` lets a JSON id
  bypass `new()` entirely (see sibling #236 audit).

## Failure modes

### FM-1 — System context has no tenant bound: cross-tenant leakage (CONFIRMED mechanism)

Several reasons are semantically **space-scoped** — `WebhookDelivery` ("scoped to its space"),
`ScheduledJob` ("the space it belongs to"), `SharedPageRead` (one shared page, REQ-037),
`ScimDirectorySync` (REQ-045), `AccountExport` (REQ-044) — yet `TenantContext::System` stores no
`space_id`. Because RLS `is_system()` lifts isolation on **every** table, a handler that builds a
`System` context for one space and reaches a generic store can read or modify any other tenant.
The invalid state (a "webhook for space A" unbound to A) is representable today. GUESS: impact is
tenant-data breach, not just a lookup miss, since writes use the same blanket bypass.

### FM-2 — Reason is audited but not enforced (CONFIRMED)

`boma.system_reason` is checked only for non-emptiness; nothing ties the variant to the rows or
tables touched. `SharedPageRead` is an anonymous, least-privilege path, yet at the DB layer it has
the same power as `DataMigration`. The enum stops arbitrary strings but not the **wrong valid
variant**: any code that can construct `TenantContext::System` can pick the most privileged reason.
Only a CI grep (policy.rs) checks call-site spelling, not reason↔operation correctness.

### FM-3 — Unguarded construction and test-drift on `ALL` (CONFIRMED)

`System` is a `pub` enum variant; nothing forces a reason to match a scope. `ALL: [Self; 8]` is
fixed-size: adding a variant forces `parse`/`as_str` updates (exhaustive match) but **not** `ALL`,
so the `req_043` round-trip test that iterates `ALL` would silently skip the new reason — any
telemetry/allow-list derived from `ALL` would miss it too.

### FM-4 — Invalid ids can become the tenant scope (CONFIRMED mechanism)

`space_id()` returns the inner `SpaceId` verbatim and `new()`/serde accept any nonblank string.
A malformed or swapped id that reaches `Member.space_id` yields an empty view (fail-closed reads,
REQ-043) but a *wrong-but-existing* id writes into another tenant. No `Member` invariant checks
that `user_id` belongs to `space_id` in the domain; that relies entirely on the DB membership table.

## Concrete hardening (with risks)

- **H1 (highest, GUESS effort high) — bind scoped reasons to a space.** Add a space to
  `TenantContext::System` (or named builders such as `system_for_webhook(space, user)`), set
  `boma.space_id` to it even on system paths, and restrict the RLS bypass to genuinely global
  reasons (`Boot`, `Authentication`, `DataMigration`). Risk: touches every store signature and
  RLS policy; existing tests construct `System` with no space and must migrate. Changing visible
  data access is a DEVIATION requiring approval — mitigate by phasing: add constrained constructors
  first, make the raw variant non-public only once call sites are converted.
- **H2 (high, risk medium) — enforce reason at the policy layer.** Replace the blanket
  `OR is_system()` with a per-reason capability (allowed reasons per table, or a distinct bypass
  setting only migrations set), so `SharedPageRead` cannot reach `bots`/`runs`. Risk: SQL/migration
  complexity; each REQ path must be re-tested or a legitimate job breaks.
- **H3 (medium, risk medium) — make raw `System` construction private** behind named builders that
  couple reason to scope and required `user_id` (e.g. `AccountExport` must carry a user). Update the
  policy.rs grep to assert reason↔scope, not just reason spelling. Risk: broad call-site churn.
- **H4 (low, risk low) — kill `ALL` drift.** Add a test asserting `ALL.len()` equals the variant
  count via an exhaustive `match` (or adopt `strum::EnumIter` with an ADR/dependency record), plus
  a property test `parse(r.as_str()) == r` for every `r` in `ALL`.
- **H5 (low, risk medium) — validate ids at trust boundaries.** Add a strict `try_new`
  (length ceiling, no control chars, prefix) used when an id becomes a tenant scope, keeping
  permissive `new` for storage-shaped values so opaque-format parity is preserved. Risk: any shape
  rule could reject a future legitimate format → DEVIATION; prefer a boundary-only check.
- **H6 (medium, GUESS) — add `TenantContext::validate() -> Result<_, DomainError>`** asserting
  scoped reasons carry a space and `user_id` consistency, called once in `with_tenant`. Risk: the
  exact invariants are only partly specified; mark unspecified ones GUESS/ADR before enforcing.

## Suggested tests (traceability)

Extend `infra/tests/tenancy.rs` with `REQ-043` integration cases proving a scoped system context
cannot read another space's rows (FM-1/H1). Add a `D-09` property test that no untrusted reason
string or wrong valid variant enters a system context. Add an `ALL`-exhaustiveness test (H4) and a
`REQ-037/REQ-044` test that shared-page/export contexts cannot perform member writes.

## Priority

H4 + H5 first (cheap, no behaviour change), then H3 → H1 → H2 (isolation payoff, DEVIATION gates),
H6 as the supporting invariant layer.
