# Worker 0349 — Architecture proposal: `crates/infra/src/stores/invitations.rs` (v0.4.0)

Lens: architecture. Scope: the Postgres `InvitationStore` adapter; the port, use cases and schema
are read only as context. Everything marked CONFIRMED was verified by reading files in this worktree
on 2026-11-19; items marked GUESS are inferences. No code was changed by this worker.

## Current structure (CONFIRMED)

`PostgresInvitationStore` is a thin, stateless adapter: one `PgPool`, a `const fn new`, and a single
`#[async_trait]` impl of `boma_app::ports::InvitationStore` with three methods — `create`,
`find_by_token_hash`, `accept` (lines 20-35). SQL is inline `sqlx::query*` with bound parameters in
each method; the row shape and its mapping live once in `stores/rows.rs` (`InvitationRow` lines
178-188, `rows::invitation` lines 392-406). The port collapses status into `pending: bool`
(`ports.rs` 252-267) while the DB stores a `text` status.

Tenant routing is deliberately split by method:
- `create` uses `tenant::in_space` under the caller's space context (lines 63-85).
- `find_by_token_hash` and `accept` use `tenant::as_system(..., SystemReason::Authentication, ...)`
  (lines 38-55, 94-110). This matches REQ-043: a token hash or invite id is all the caller has, so
  the space is the *answer* of the lookup, not a precondition (module doc, lines 5-8).

Schema (CONFIRMED, `migrations/0002_m1.sql` 226-236, 297-298; `0050_invitation_inviter.sql`):
`invitations(id PK, space_id FK ON DELETE CASCADE, email, token_hash bytea, role text, expires_at
timestamptz, status text DEFAULT 'pending', created_at timestamptz, inviter_user_id)`, non-unique
indexes on `token_hash` and `space_id`, RLS tenant policy. The app hashes the token before create
and never returns it (`usecases.rs` 303-304, golden `space-invite.json`).

## Concrete upgrades

### U1 — Enforce expiry in the atomic `accept` statement (HIGH)
`accept` takes `_now: &str` but ignores it; the `UPDATE` only checks `status = 'pending'`. Expiry is
checked in the app layer (`invite_preview` 8839, `invite_accept` 8884), so a token can expire
between the check and the accept, and a pending-but-expired row is still findable by hash. Upgrade:
`WHERE id = $1 AND status = 'pending' AND expires_at > $2::timestamptz` with `now` bound.
Risk: string-to-`timestamptz` cast and clock-source consistency; `rows_affected()` becomes false for
expired rows, changing the loser path in `invite_accept` (8934-8937); any externally visible change
needs a DEVIATION/PARITY entry and a REQ-042-named regression test.

### U2 — Replace magic status strings and the `pending: bool` collapse (MEDIUM)
The spec data model says status ∈ pending/approved/declined/revoked (`02-data-model.md` 25) but the
store writes `'accepted'` and reads `status == "pending"` (line 44, `rows.rs` 400). Upgrade: a typed
`InvitationStatus` newtype in domain, a DB CHECK constraint, and a faithful port field.
Risk: a real spec-vs-code conflict — resolve it in `docs/STATE.md` before changing behavior; a
column/port migration touches `MemoryInvitationStore` (`testkit/src/memory.rs` 2859) and any DTO
exposing status; old rows need a backfill. Do not silently rename a public field.

### U3 — Make `token_hash` unique and remove arbitrary tie-breaking (MEDIUM)
The `token_hash` index is not unique and the lookup does `ORDER BY created_at DESC LIMIT 1`
(lines 97-102), so a hash collision silently shadows one invitation. Upgrade: UNIQUE constraint on
`token_hash`, drop the `LIMIT 1`, and let a collision surface as a conflict.
Risk: existing duplicate hashes would block the index (audit/dedup migration first); combined with
the current blanket `Unavailable` mapping (U4) a collision would still be misreported; uniqueness
changes create semantics for repeated hashes — verify the token generator's collision guarantees.

### U4 — Typed error mapping at the adapter boundary (MEDIUM)
Every sqlx failure is flattened to `AppError::Unavailable` via `tenant::unavailable` (lines 50, 81,
107). Unique/PK/FK violations and serialization failures are indistinguishable from a dead DB.
Upgrade: classify constraint violations as `Conflict`, connection failures as `Unavailable`, keep
no internal detail in the API error body.
Risk: error classification is part of the stable API error format; changing it is externally visible
and must stay identical in `MemoryInvitationStore` and be recorded in `docs/PARITY.md`.

### U5 — Give the port the full invitation lifecycle (LOW/MEDIUM)
`spec/02-data-model.md` lists `revoked`, and `/admin/access` shows pending invitations
(`08-ui-ux.md` 150), but the port has only create/find/accept. Upgrade: add tenant-scoped
`list_for_space` (`in_space`) and conditional `revoke`, with an index on `(space_id, status)`.
Risk: new port methods force edits in both implementors; any new RPC/field is a DEVIATION needing
approval; an admin list is a new query shape — bound and paginate it. GUESS: no revoke RPC exists
yet; confirm against `03-api-contract.md` before starting.

## Sequencing and deferred work

U4 then U3 then U1 (error semantics first, then uniqueness, then the atomic expiry check); U2 and
U5 are independently shippable but both need a STATE.md decision first. Deferred: folding the
membership insert, status flip and audit append (`usecases.rs` 8903-8950) into one transaction would
remove a crash window, but it breaks the port's documented single-statement idempotence contract
and is a cross-aggregate refactor — ADR required. Keep the system-path reason narrow and logged per
REQ-043 throughout.

## Evidence (CONFIRMED)

`crates/infra/src/stores/invitations.rs`; `crates/infra/src/stores/rows.rs`;
`crates/app/src/ports.rs` (229-267, 1062-1090); `crates/app/src/usecases.rs` (294-332, 8806-8954);
`migrations/0002_m1.sql` (226-236, 297-298); `migrations/0050_invitation_inviter.sql`;
`crates/testkit/src/memory.rs:2859`; `spec/01-requirements.md` REQ-042/REQ-043;
`spec/02-data-model.md:25`; `spec/08-ui-ux.md:150`.
