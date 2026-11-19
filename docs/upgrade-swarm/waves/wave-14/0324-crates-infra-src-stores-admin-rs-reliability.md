# Worker #324 — `crates/infra/src/stores/admin.rs` — LENS: reliability — v0.4.0 proposal

- **Status:** PROPOSAL (read-only review; no source changed). CONFIRMED = verified by reading this file and its call sites in this worktree; GUESS = needs a test/oracle run or a spec decision.
- **Scope:** `PostgresAdminStore` (REQ-058): space-scoped member/invitation reads and revocation, deployment-scoped grant and plugin-review writes, plus the audit events the use cases append beside them.
- **Anchors:** `usecases.rs` 8993–9235, `ports.rs` 4190–4339, `tenant.rs` 51–143, `audit.rs` store, `audit.rs` domain (kinds), migrations `0002_m1.sql` / `0027` / `0030` / `0047`.

## Summary

The SQL is small and mostly correct per statement, but reliability is weakened at the seams: authorization is checked in a different transaction from every write, the write and its audit line are two independent commits, and the read queries silently drop staleness signals the schema already carries (`deactivated_at`, `expires_at`). The result is a store that can commit an admin action after the actor lost the role, can commit it with no durable audit proof, and can render a roster that disagrees with both. No `#[cfg(test)]` exists in this file (CONFIRMED), so none of these races has a regression test.

## Failure modes

### Role-change races (R)

- **R1 (HIGH, CONFIRMED):** authorization is TOCTOU. `require_admin` (`usecases.rs:8993–9001`) calls `require_space_role` → `spaces.rs::membership`, which opens its **own** `tenant::in_space` transaction (`spaces.rs:214–236`). The subsequent `grant_access` / `revoke_grant` / `decide_submission` / `revoke_invitation` each open a **separate** `tenant::as_system` / `with_tenant` transaction. A demotion or membership removal between the two commits leaves a grant issued, a grant revoked or a submission decided by someone who is no longer admin. The store cannot defend itself: role checks live in the use case by contract (`ports.rs:4248`).
- **R2 (MED, CONFIRMED):** `grant_access` inserts with `ON CONFLICT DO NOTHING`, reads `rows_affected()`, then re-`SELECT`s the row (`admin.rs:178–200`). A concurrent `revoke_grant` between insert and select makes `fetch_one` return no row → `AppError::Unavailable` (an outage label for a benign race). The reverse interleaving can also return `Conflict` for a grant that was legitimately revoked and re-created.
- **R3 (MED, CONFIRMED):** `decide_submission` is a sound compare-and-set on `status = 'pending'` (`:288–296`, exactly one concurrent decision wins), but it **ignores `_reviewer`** (`:278`) and the table has no `reviewed_by`/`reviewed_at` columns (migration `0027`). The database row therefore carries no durable "who/when"; the only proof is a separate audit append (see A3).
- **R4 (LOW, CONFIRMED):** every deployment-scoped operation runs `as_system(…, SystemReason::ScheduledJob)` (`:134–175, 215, 238, 282`). The DB session records no per-request actor, so a human admin write is indistinguishable from a scheduled job at the storage layer; correlating it to an admin needs the fragile audit join below (same class as D-09).
- **R5 (LOW, CONFIRMED):** `AuditKind::AdminMemberRoleChanged` / `AdminMemberRemoved` exist (`domain/audit.rs:35–38`) but have **no emitter anywhere in `crates/`** (grep: 0 hits outside the enum). If v0.4.0 adds role management, the audit path does not exist yet.

### Stale roster (S)

- **S1 (HIGH, CONFIRMED):** `members()` selects `boma.rfc3339(u.deactivated_at)` and then discards it as `_deactivated_at` (`:42, 58`); the `JOIN`/`WHERE` has no `u.deactivated_at IS NULL` predicate. Deactivated accounts keep appearing as active members with a live-looking role.
- **S2 (MED, CONFIRMED):** `last_seen_at: None` is hard-coded (`:64`) despite the port field documented as "when they last signed in" (`ports.rs:4201`). The roster never reports freshness, so a stale session is indistinguishable from a current one. GUESS: whether sessions data is intended here needs a spec/STATE.md decision before adding a join.
- **S3 (MED, CONFIRMED):** `invitations()` filters only `status = 'pending'` (`:83`), not `expires_at > now()`, and `revoke_invitation` predicates only on `status = 'pending'` (`:120`). Already-expired invitations remain in the admin queue as actionable and can be "revoked"; the displayed expiry is present but the state is stale.
- **S4 (MED, CONFIRMED):** each list is a single `SELECT` (no torn rows), but `members()` and `invitations()` use separate transactions at separate read times with no `REPEATABLE READ` snapshot. Two admin panels (or a list followed by a mutation) can legitimately disagree about the same roster — amplified by R1.
- **S5 (LOW, CONFIRMED):** `plugin_submissions()` orders by `created_at DESC` only, with `LIMIT $2` (`:248–249`) and no unique tie-break; rows at the 200-item boundary can flap between refreshes. `invitations()` has the same non-deterministic ordering (`:84`).
- **S6 (LOW, CONFIRMED):** one malformed stored `email`/`user_id` maps to `AppError::Unavailable` (`:60–61`), taking down the **whole** roster instead of that row. A data-quality defect is reported as an outage; `rows::role` parse errors similarly propagate and abort the list.

### Audit gaps (A)

- **A1 (HIGH, CONFIRMED):** `let _ = audit.append(...)` swallows every audit failure on grant create (`usecases.rs:9102–9116`), grant revoke (`:9137–9149`) and plugin decision (`:9219–9233`). The state change commits; the audit row may not. This defeats the append-only trail's purpose (D-05, `audit.rs` doc: "the audit line is what the operator can prove").
- **A2 (HIGH, CONFIRMED):** `admin_invitations_revoke` (`usecases.rs:9040–9053`) emits **no** audit event at all, and no `AuditKind` variant exists for it (enum `:8–45`). Invitation revocation is invisible in the trail.
- **A3 (HIGH, CONFIRMED):** the mutation and the audit append are two separate transactions across two stores. A crash, cancellation, or `Unavailable` between them leaves an orphaned write with no proof (and A1 makes the failure silent). There is no outbox, idempotency key, or same-transaction append.
- **A4 (MED, CONFIRMED):** `decide_submission` records no reviewer at row level (R3), so when the audit append is lost (A1/A3) there is no fallback record of who approved a plugin — the highest-consequence write in this module (it gates installation).
- **A5 (LOW, CONFIRMED):** `revoke_grant` hard-`DELETE`s the row (`:220`); `access_grants` has no tombstone (migration `0047`). Audit is the sole history, and A1/A3 can erase it.

## Concrete hardening (prioritized)

- **P0 — make audit atomic with the write.** Add a store-side method that performs the grant/revoke/decision **and** the `audit_events` insert in one `tenant::scope` transaction (extend `audit.rs` store with a within-transaction append, or a transactional outbox table drained by a job). Stop discarding `append` errors: fail the admin operation, or persist an outbox row and retry. Add `AdminInvitationRevoked` to `AuditKind` and emit it. Targets A1–A5, R3.
- **P0 — re-check authorization inside the write.** Pass the acting `user_id` into the mutation and, in the **same** transaction, `SELECT … FROM space_members WHERE space_id = $1 AND user_id = $2 AND role IN ('owner','admin') FOR UPDATE` (or run under `SERIALIZABLE` with a bounded retry). Reject `Forbidden` there so a demotion cannot slip between check and write. Targets R1; R4 can be improved by using the actor's tenant context instead of a blanket `ScheduledJob`.
- **P1 — fix stale reads.** Add `AND u.deactivated_at IS NULL` to `members()`; add `AND expires_at > now()` to `invitations()` and `revoke_invitation`; add a deterministic `, id DESC` tie-break to invitations and submissions. Populate `last_seen_at` only after a spec decision (S2). Targets S1, S3, S5.
- **P1 — typed race errors.** Use `fetch_optional` after the grant insert and map `None` to `Conflict`/`NotFound` instead of `Unavailable`; in `decide_submission` distinguish "already decided" (`Conflict`) from "no such id" (`NotFound`). Add a `#[transaction]`-style integration test per write asserting the audit row exists. Targets R2, A3.
- **P2 — avoid all-or-nothing roster.** Map bad per-row ids to a logged skip (or a typed `AdminMember::Invalid` placeholder) so one corrupt row cannot fail the full list. Emit structured `tracing` (actor, target, decision, rows_affected) and a counter for audit-append failures. Targets S6, A1.

## Risks / trade-offs

- Same-transaction audit changes the `AdminStore`/`AuditStore` port shape and crosses the "store never decides authorization" boundary; an outbox avoids that but adds a table, a worker and a new failure mode. Decide with an ADR (alternatives + scoring) before coding.
- Failing an admin action when audit cannot be written is an externally visible behaviour change → record as a DEVIATION in `docs/PARITY.md` and wait for approval. A retry-only design keeps today's behaviour but leaves a small unaudited window.
- `FOR UPDATE` on `space_members` serializes admin writes with membership readers and can deadlock under concurrent grant + membership updates; prefer a short `SERIALIZABLE` retry budget and measure before committing to row locks.
- Filtering deactivated members and expired invitations changes API output (DEVIATION). Confirm no consumer relies on seeing them; if needed, add an explicit `include_inactive` flag rather than swapping semantics silently.
- Populating `last_seen_at` needs a sessions query and may expose freshness data not previously returned; treat as a GUESS until the port's intent is confirmed in `docs/STATE.md`.
- Tightening error typing (R2) is internal but changes error codes clients may branch on; include in the same DEVIATION review.
