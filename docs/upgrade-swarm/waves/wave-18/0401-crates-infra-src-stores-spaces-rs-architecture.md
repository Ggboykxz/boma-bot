# v0.4.0 architecture proposal — `crates/infra/src/stores/spaces.rs` (spaces store)

Worker #401 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no code changed by this worker).

Status of claims: "current" statements are CONFIRMED by direct reads of `spaces.rs` (all 259
lines), `stores/rows.rs`, `tenant.rs`, `crates/app/src/ports.rs` (the `SpaceStore` trait, 550-628),
plus symbol searches for `is_unique_violation`, `as_str` and `PostgresSpaceStore`. Forward-looking
items are proposals (GUESS until implemented and tested).

## Current structure (CONFIRMED)

- `PostgresSpaceStore` is a stateless adapter: one `PgPool` field, `const fn new`, `#[derive(Debug,
  Clone)]`. All eight `SpaceStore` methods are thin SQL wrappers over `tenant::{in_space,
  as_system}`; each transaction sets `boma.space_id` / `boma.system_reason` (REQ-043) and maps
  every driver error to `AppError::Unavailable` via `tenant::unavailable` (REQ-054).
- **Tenant split is asymmetric.** Only `find`, `membership` and `create` are member-scoped
  (`in_space`). Five methods — `name_of`, `add_member`, `spaces_of`, `member_ids_of`, `default_for`
  — cross tenants with `as_system` and a `SystemReason`. This contradicts the module doc (lines
  3-10), which claims only `create` and `default_for` "need more than the tenant they are given".
- **System reasons are inconsistent.** `Authentication` is used for `name_of` (anonymous invite
  preview), `add_member` and `default_for`; `AccountExport` for `spaces_of` and `member_ids_of`.
  The choice is not derived from the caller, so one reason may be too broad or too narrow.
- **Owned-value boilerplate.** The boxed HRTB futures (`TenantFuture`) force ids/roles/timestamps to
  be cloned to `String` before the closure. `member_ids_of` clones `space_id` twice (lines 120 and
  125); the inner clone is redundant because the closure is `FnOnce` and can move the owned value.
- **Role stringification is duplicated.** `add_member` matches `SpaceRole` to `"owner"/"admin"/
  "member"` inline (76-80), `create` hard-codes `'owner'` (171), and `rows::role` parses the reverse
  with `SpaceRole::parse`. Domain enums elsewhere expose `as_str()`; `SpaceRole` has no such helper.
- **Conflict handling diverges from the crate idiom.** `add_member` uses `ON CONFLICT DO NOTHING`
  then `rows_affected() == 0 -> AppError::Conflict`. The rest of infra (`cards.rs`, `messages.rs`,
  `runs.rs`, `accounts.rs`, `mcp.rs`) maps unique violations with `tenant::is_unique_violation`.
  `create` has no duplicate-id handling and reports a duplicate space as `Unavailable`.
- **No unit tests in this file.** Space coverage is indirect: `crates/infra/tests/m1.rs`,
  `crates/testkit/src/memory.rs` (a parallel in-memory `SpaceStore`), and heavy use of `default_for`
  across `testkit`. There is no focused test pinning the two cross-tenant paths or the
  Postgres/in-memory parity.

## Guiding principle for v0.4.0

Keep the adapter stateless and the public trait byte-identical, but make the tenant contract the
load-bearing, visible part of the module: one canonical helper per tenant scope, one canonical
role ↔ DB mapping, one conflict idiom, and focused integration tests for the paths that cross
tenants. Restructure internals only; no query result, status, error or REQ behavior changes.

## Upgrades (5 concrete items, each with a risk)

1. **Unify the tenant entry points and reconcile the doc.** Add two private helpers,
   `system_read(reason, |conn| ...)` and `member_write(space_id, |conn| ...)`, thin wrappers over
   `as_system` / `in_space`, and route all eight methods through them. Rewrite the module doc to
   state the real split (5 of 8 methods are system-scoped) and the reason each one needs.
   *Risk:* a wrong reason passed to a helper is a silent RLS bypass, not a compile error.
   *Mitigation:* keep `SystemReason` closed, add a comment at every call site, and lean on the
   existing ADR-002 CI check that greps for raw `boma.space_id` outside `tenant.rs`.

2. **Tighten system reasons.** Give the anonymous invite-preview read (`name_of`) a dedicated
   `SystemReason` variant rather than reusing `Authentication`, and check that `AccountExport` is
   really the narrowest reason for `spaces_of` / `member_ids_of` (both serve account delete,
   REQ-044).
   *Risk:* a new enum variant plus an RLS policy change is a code + migration change; a policy that
   does not allow the new reason breaks the invite page at runtime.
   *Mitigation:* add the variant and its policy in one migration, cover it with an integration test
   that calls `name_of` with no tenant, and record any behavior change as a DEVIATION in
   `docs/PARITY.md` before shipping.

3. **One role ↔ DB mapping.** Add `SpaceRole::as_str()` in the domain crate, mirroring the other
   enums, and a matching `rows::role_str` helper; replace the inline match and the hard-coded
   `'owner'` with it. Add a round-trip test over all three variants.
   *Risk:* a typo in a stored literal silently corrupts roles; adding a domain method touches a
   crate boundary other workers own.
   *Mitigation:* keep the three literals exactly `owner`/`admin`/`member`, test
   `parse(as_str(x)) == x`, and keep the change additive so no downstream API shifts.

4. **Adopt the crate-wide conflict idiom.** Replace `DO NOTHING` + `rows_affected` with
   `INSERT ... ON CONFLICT DO NOTHING RETURNING` (or map `tenant::is_unique_violation` to
   `AppError::Conflict`) in `add_member`, and give `create` an explicit duplicate-id -> `Conflict`
   path instead of `Unavailable`.
   *Risk:* changing when `Conflict` is returned alters the invite/join flow (REQ-042) and could be
   an externally visible DEVIATION; a `RETURNING` rewrite can also change `rows_affected` semantics.
   *Mitigation:* pin current behavior in a test first (duplicate membership = `Conflict`, idempotent
   re-invite behavior unchanged), make the smallest change, and run `REQ-042`-named tests plus the
   account-delete flow before declaring green.

5. **Remove clone boilerplate and add focused store tests.** Move a single owned `String` into each
   `FnOnce` closure (drop the double clone in `member_ids_of`), and add integration tests in
   `crates/infra/tests/` for: `create` + first membership atomicity (REQ-042), duplicate membership,
   `default_for` tie-break ordering, cross-tenant `find` returning `None` (REQ-043), and a
   Postgres-vs-in-memory parity check for the eight `SpaceStore` methods.
   *Risk:* moving captures into HRTB boxed futures can hit borrow-checker lifetime friction; new
   tests that rely on unordered rows or wall-clock `now` can be flaky.
   *Mitigation:* seed fixed ids and a fixed `now`, assert order only where SQL guarantees it
   (`default_for`, `spaces_of`), reuse `crates/infra/tests/common` fixtures, and keep REQ ids in
   test names (`tools/trace_check.py` counts declarations, not comments).

## Cross-cutting risks / sequencing

- Sequence: 1 → 2 (scope helpers first, then reasons) → 3 → 4 → 5 (tests last pin the refactor).
  Items 3 and 5 are largely independent of 1-2.
- **Parity:** all five target internal structure. Any change to a returned value, error variant
  surfaced to clients, or REQ-042/043/044 behavior is a DEVIATION requiring `docs/PARITY.md` and
  maintainer approval (AGENTS.md). Target zero deviations and unchanged golden fixtures.
- Verification gate per step: `cargo fmt --check`, `clippy -D warnings`, unit + infra integration
  tests (`crates/infra/tests/m1.rs`), and the relevant `REQ-0xx`-named tests.

## Out of scope for this module

The SQL schema and RLS policies (`migrations/`), the `SpaceStore` trait shape (`crates/app`), the
domain model of `Space`/`SpaceRole`, and the in-memory implementation (`crates/testkit`) are owned
by other workers; this proposal only restructures how this adapter scopes tenants, maps roles,
reports conflicts and is tested.
