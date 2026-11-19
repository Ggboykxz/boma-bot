# Worker #323 — `crates/infra/src/stores/admin.rs` — architecture (v0.4.0)

- Module: `crates/infra/src/stores/admin.rs` (305 lines, REQ-058); lens: architecture / store structure.
- Mode: READ-ONLY proposal; no code changed, no tests run. Shape/findings CONFIRMED by reading this
  session (module, port, use cases, `tenant.rs`, `rows.rs`, migrations, memory twin). Upgrades and
  risks are GUESS (design inference). No wire/API/data-format change proposed by default; anything
  externally visible is flagged as a DEVIATION needing `docs/PARITY.md` + owner approval.

## 1. Current shape (CONFIRMED)

- One `PostgresAdminStore { pool: PgPool }` implements all seven `AdminStore` methods
  (`ports.rs:4250-4339`); no state beyond the pool, no tests in this file.
- Two trust domains share one impl: tenant-scoped `members`/`invitations`/`revoke_invitation` use
  `tenant::with_tenant` (L39,77,116); deployment-wide grants and plugin review use
  `tenant::as_system(..., SystemReason::ScheduledJob)` (L134,173,215,238,282). The boundary is only
  a module comment (L4-6).
- Every read decodes an anonymous `sqlx::query_as::<_, (String, ...)>` tuple and maps it inline
  (L41-68,79-105,139-159,244-267). No `Admin*Row` structs exist in `rows.rs`, despite its stated
  rule that "every read goes through one of these structs" (`rows.rs:1-5`). `rows::role` (L63,100)
  is the only shared mapper.
- Newtype failures (`UserId::new`, `Email::new`) collapse to `AppError::Unavailable` (L60-62).
- `members` selects `boma.rfc3339(u.deactivated_at)` and discards it as `_deactivated_at` (L42,58),
  and always returns `last_seen_at: None` (L64; port field at `ports.rs:4201-4202`). The memory twin
  does the same (`memory.rs:8590`).
- `grant_access` = `INSERT ... ON CONFLICT DO NOTHING` then `SELECT` in one transaction (L178-206),
  `Conflict` on 0 rows. `revoke_grant` is a hard `DELETE` (L220). `decide_submission` ignores
  `_reviewer` (L278) and trusts `decision: &str` (L288-293); migration 0027 has no `reviewed_by`.
- `plugin_submissions` binds a raw `limit: i64` and `status: Option<&str>` via
  `($1::text IS NULL OR status = $1)` (L244-252). The status vocabulary is repeated in trait docs,
  use-case validation (`usecases.rs:9166-9175,9204-9212`) and the SQL CHECK (`0027:15`).

## 2. Architecture findings (CONFIRMED unless marked)

1. **Scope is a convention, not a structure.** Any future method can silently pick `with_tenant` vs
   `as_system`; RLS makes the wrong choice read empty rather than fail loudly. GUESS: the highest-value
   structural fix is to make the scope choice the adapter's, not each method's.
2. **Wrong system reason.** Human admin actions run as `ScheduledJob` (L136,175,217,240,284);
   `SystemReason` has no admin variant (`tenancy.rs:19-36`). The reason setting/telemetry says a cron
   did it. A correct variant touches domain `ALL`, `as_str`, `parse` and is a coordinated change.
3. **`rows.rs` invariant broken here.** Six inline maps mean a column rename or mapping fix must be
   repeated; the rest of `infra` centralizes this. Internal refactor, low parity risk if mappings
   stay byte-identical.
4. **Weak port types push validation outward.** `decision: &str`, `status: Option<&str>`, raw
   `limit: i64`, `granted_by: String` (`ports.rs:4220-4244,4301-4338`) allow invalid states down to
   SQL; the reviewer argument carries no effect. "Who decided" lives only in the audit append
   (`usecases.rs:9219-9233`).
5. **Members read is dishonest.** `last_seen_at` is structurally `None` and `deactivated_at` is read
   then dropped, so a deactivated member looks active. GUESS: populating needs a sessions/activity
   source not found in migrations; deleting the field is a wire change.

## 3. Concrete upgrades for v0.4.0 (with risks)

**U1 — Add `Admin*Row` structs and mappers in `rows.rs` (do first, internal).** Define row structs for
`users`+`space_members` join, `invitations`, `access_grants`, `plugin_submissions` and pure mappers to
the four port types; replace all six tuple decodes. Risks: (a) every mapping must stay byte-identical,
especially `rows::role` error strings; (b) keep the unused `deactivated_at` column in the row until U5
decides; (c) never touch the two shared helpers other stores use.

**U2 — Encapsulate the two trust domains.** Keep the public port, add private `in_space`/`as_system`
helpers so each method only supplies SQL, and split the impl into two clearly labelled blocks; put the
system reason in one constant. Risks: (a) `with_tenant` on a system context would take the `as_system`
branch (`tenant.rs:59-62`) — lock scope with integration tests for both context kinds; (b) no change to
RLS or migration policy; (c) GUESS: a full port split into two traits is cleaner but touches app wiring
and the memory twin — defer beyond v0.4.0.

**U3 — Bound and type the submission inputs.** Clamp `limit` to `1..=200` in the store (the use case
already hardcodes 200, `usecases.rs:9176`), and introduce a typed decision/status in the port with
`TryFrom<&str>` at the wire boundary, leaving the RPC DTO strings intact. Risks: (a) port change
requires the memory twin and all callers to move together; (b) preserve exact `Rule` error strings;
(c) a typed status must still accept all six CHECK values or goldens break; (d) no new DB constraint
without a versioned migration.

**U4 — Give operator actions an honest reason and record the reviewer.** Add
`SystemReason::AdminAction` (domain `ALL`/`as_str`/`parse` together, ADR note), use it in the five
system methods, and add `reviewed_by`/`reviewed_at` to `plugin_submissions` in a versioned migration
written from `&reviewer`. Risks: (a) new wire value for the reason enum — check no golden pins the
current `scheduled_job` string; (b) backfill/nullable column for existing rows; (c) audit already
records the decision — do not emit a duplicate user-visible event, just persist the durable fact;
(d) `grant_by`/`reviewed_by` IDs, never free text.

**U5 — Resolve the member-read dead fields.** Cheapest: drop the unused `deactivated_at` SELECT once
U1 has the row struct; richer: populate `last_seen_at` from a real activity source and surface
deactivation. Risks: (a) removing/emptying a port field is externally visible — DEVIATION + owner
approval; (b) a sessions join on an admin-only list is a new query cost — measure before adding;
(c) GUESS about the source needs a decision recorded in `docs/STATE.md`; pick "keep field, drop dead
SELECT" as the safe default for v0.4.0.

## 4. Ordering, invariants, non-goals

Order: **U1 → U2 → U3 → U4 → U5** (wire-neutral refactors first; U4/U5 need decisions). Invariants:
tenant data only inside `with_tenant`, deployment data only via `as_system`, RLS never weakened,
status vocabulary and error strings stable, `req_058` names in the trace gate, memory twin in lockstep.
Non-goals: no new dependency, no port split, no hard-delete-to-soft-delete change, no audit redesign,
no ADR/spec/gate edit by this worker.

## 5. Evidence & tests to add

- CONFIRMED — read this session: `admin.rs:1-305`; `ports.rs:4190-4244,4250-4339`; `usecases.rs:8988-9235`;
  `tenant.rs:41-143`; `rows.rs:1-5,253-256`; `tenancy.rs:19-36`; migrations `0027`, `0047`;
  `memory.rs:8528-8714`; sibling wave-14 architecture proposals.
- GUESS — whether goldens pin the `scheduled_job` reason or exact error strings; source for
  `last_seen_at`; whether an `AdminAction` reason is wanted; U3 port-change appetite.
- Suggested names: `req_058_admin_tenant_and_system_scopes_are_not_confused`,
  `req_058_submission_limit_is_bounded`, `req_058_a_reviewer_is_recorded_on_the_submission`,
  `req_058_member_rows_map_through_rows.rs`.

Line budget: <120. Status: proposal only; READ-ONLY worker, no code changed and no tests run.
