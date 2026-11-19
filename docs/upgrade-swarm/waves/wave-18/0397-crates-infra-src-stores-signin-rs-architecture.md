# v0.4.0 architecture proposal — `crates/infra/src/stores/signin.rs` (sign-in request store)

Worker #397 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no code changed).

Claim status: "current state" is CONFIRMED by direct reads of this file, `crates/infra/src/tenant.rs`,
`crates/domain/src/signin.rs`, `crates/app/src/ports.rs:4405-4442`, `crates/app/src/usecases.rs:9700-9800`,
`crates/testkit/src/memory.rs:8848-8925` and `migrations/0055_signin_requests.sql`. Forward-looking
items are proposals (GUESS until implemented and verified). English per disk convention; no gate weakened.

## Current structure (CONFIRMED)

- `PostgresSignInStore` is a thin `Clone` adapter over one `PgPool` with three port methods:
  `record` (INSERT … ON CONFLICT DO UPDATE, :67-105), `list` (SELECT by space, newest-first,
  variable LIMIT, :107-135), `resolve` (conditional `UPDATE … WHERE id=$1 AND space_id=$4 AND
  status='pending'`; missing row → `AppError::Conflict`, :137-174). The compare-and-swap in
  `resolve` is sound and gives exactly-once answering (REQ-060); keep it.
- All three run inside `tenant::in_space` (transaction + transaction-scoped `boma.space_id`,
  tenant.rs:75-84,106-143). `list`/`resolve` scope from `TenantContext`; `record` scopes from the
  *payload* `request.space_id` (:71-72) — safe only because `request_signin` copies the space from
  the authenticated context (usecases.rs:9737-9745).
- The 10-12 column projection is duplicated verbatim three times (:85-88,117-120,157-160) and
  mirrored by `SignInRequestRow` (24-37) and `into_domain` (39-63): adding a column means four edits.
- `into_domain` maps *every* failure — unknown status, invalid `SpaceId`/`BotId`/`UserId`, bad
  `granted_by` — to `AppError::Unavailable` (:44,48-59), conflating corruption with downtime.
  Unknown status should be unreachable given CHECK at migration 0055:16; reaching it is an alarm,
  not a 503.
- Timestamps cross as RFC 3339 `String`s: written from caller `created_at` via `$8::timestamptz`
  (:76,97), read through `boma.rfc3339`. On re-record `updated_at` is set to the *incoming
  `created_at`* (:84), so it can move backwards; `resolve` uses `now()` (:155). Two time sources.
- The upsert never touches `status`/`granted_by` (:78-84): re-recording a granted id overwrites
  origin/display_name/thread_id but keeps the resolution. The memory adapter resets status to
  `Pending` on every `record` (memory.rs:8879). The port only says "upsert-by-id"
  (ports.rs:4408-4416): the implementations disagree and the correct behaviour is unspecified.
- Adapter error mappings diverge: Postgres returns `NotFound` for a System context (:112,144),
  memory `Forbidden` (memory.rs:8893,8910). Memory `list` ignores `space_id` when filtering and
  memory `resolve` ignores space entirely — a tenant leak/cross-write; this Postgres adapter is
  correct. The port should state the invariant explicitly.
- `LIMIT` uses `unwrap_or(50)` then `i64::try_from(...).unwrap_or(i64::MAX)` (:127): panic-free
  but unbounded for a hostile value. `ORDER BY created_at DESC` has no tiebreaker (:123), so
  equal timestamps order unstably. No `#[cfg(test)]`, no tracing span, no compile-time guard
  keeping SQL and `SignInRequestRow` in sync.

## Guiding principle for v0.4.0

Make this adapter the strict reference implementation of the port: one row-mapping seam, explicit
lifecycle semantics, server-owned time, bounded reads, tenant scope checked independently of the
caller, and errors that distinguish "down" from "corrupt"/"conflict". No wire change without a
`docs/PARITY.md` DEVIATION entry.

## Upgrades (5 concrete items)

1. **One mapping seam + typed decode errors.** Extract a `const SIGNIN_COLUMNS` used by all three
   queries and one `TryFrom<SignInRequestRow>` returning a dedicated decode error logged at
   `error!`, mapped off the 503 path; keep `tenant::unavailable` for driver failures only.
   *Risk:* a new `AppError` variant changes the stable API; legacy bad-status rows surface
   differently. *Mitigation:* integration test seeding a bad status; test that the constant is
   used in all three statements. Happy path unchanged.

2. **Specify and enforce re-record semantics at the port.** Choose immutability: ids are minted
   once, so `record` should be `ON CONFLICT DO NOTHING` returning the stored row (truly
   idempotent), never silently rewriting an audited grant. Update the port doc (ports.rs:4408)
   and align `MemorySignInStore::record` in the same change.
   *Risk:* if a caller relies on re-record updating origin, behaviour changes — externally
   visible, so check goldens and file a DEVIATION. *Mitigation:* REQ-060-named test for
   record → resolve → record asserting status and granted_by survive; update the memory test in
   the same commit so the two adapters never disagree.

3. **Server-owned time and an append-only resolution trail.** Stop binding caller `created_at`
   into `updated_at`; let DB defaults own both timestamps and `resolve` own `now()`. Add a
   `signin_resolutions` table (request_id, status, resolved_by, resolved_at) written in the same
   `in_space` transaction as the status update.
   *Risk:* new table/RLS policy must be scoped correctly; goldens asserting `updated_at` change,
   a client-visible DEVIATION. *Mitigation:* write inside the existing compare-and-swap
   transaction (atomic on failure), keep `boma.rfc3339` field format, test RLS on the new table.

4. **Bounded, deterministic list and consistent errors.** Clamp `limit` to a documented ceiling
   (e.g. 200) instead of `i64::MAX`, add `, id DESC` as tiebreaker, and agree one mapping for a
   no-space context (`NotFound` vs `Forbidden`) across both adapters, documented on the trait.
   *Risk:* clamping changes results for large limits — DEVIATION if a spec endpoint passes one
   through. *Mitigation:* grep callers/goldens for limit first; test `None`, `Some(0)`,
   `Some(usize::MAX)` against a named constant.

5. **Defense-in-depth tenant scoping + observability.** Derive `record`'s scope from a port
   context, not only the payload: add a context argument (port change), or assert
   `request.space_id` equals the caller's space in the app layer and document the invariant.
   Wrap each method in a `tracing::info_span` with opaque request/space ids, never in error bodies
   (REQ-054).
   *Risk:* a port signature change fans out to both adapters and callers; ids must not leak
   personal data. *Mitigation:* prefer the additive invariant and test now; defer the signature
   change to a coordinated release.

## Cross-cutting risks / sequencing

- Order 1 → 2 → 4 → 3 → 5: 1/4 are local hardening; 2 is a port decision; 3 needs a migration and
  RLS review; 5 touches the port. Step 2 depends on the memory-adapter worker — land both halves
  together.
- Never replace `resolve`'s conditional `status='pending'` update with read-then-write;
  exactly-once answering is the requirement (REQ-060).
- Gate per step: `cargo fmt --check`, `clippy -D warnings`, unit + integration + golden + contract
  suites, REQ-060 in test names.

## Out of scope

Memory-adapter fixes, approval-card UI, browser sign-in flow and the `boma.rfc3339` SQL helper
belong to other modules; this proposal only reshapes this adapter and its port contract.

## Evidence

CONFIRMED — read this session: `crates/infra/src/stores/signin.rs:1-175`;
`crates/infra/src/tenant.rs:75-143,153-155`; `crates/domain/src/signin.rs:1-62`;
`crates/app/src/ports.rs:4405-4442`; `crates/app/src/usecases.rs:9700-9800`;
`crates/testkit/src/memory.rs:8848-8925`; `migrations/0055_signin_requests.sql:1-29`.
GUESS — intended re-record semantics and the canonical no-space error mapping (port silent);
whether any golden asserts `updated_at` on a re-recorded request (verify before steps 2 and 3).
