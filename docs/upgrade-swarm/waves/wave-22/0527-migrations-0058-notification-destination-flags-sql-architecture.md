# Worker #527 — `migrations/0058_notification_destination_flags.sql` — architecture (v0.4.0)

- Worker #527 · Lens: architecture (structure, boundaries, coupling, debt) · Target: v0.4.0.
- Status: PROPOSAL, READ-ONLY. No migration, code or doc changed; no tests run. CONFIRMED = direct
  read this session; GUESS = inference or a claim from a sibling brief not re-verified here.
- Module: `migrations/0058_notification_destination_flags.sql` (5 lines): REQ-065 per-bot opt-in
  flags added to the existing `notification_policies` table created in `0056_notifications.sql`.
- Read for context (CONFIRMED): `0056_notifications.sql:34-52` (base table, RLS ENABLE+FORCE),
  `0057_notification_destinations.sql:6,16-25` (`kind` CHECK, RLS), `spec/01:610-618` (REQ-065),
  `docs/adr/028-notification-destinations.md:11-48`, `design.m15-push.md:154-178`,
  `notification.rs:84-126`, `push.rs:151-162`, `stores/notifications.rs:62-77,190-255`,
  `wire.rs:2230-2263`, `rpc.rs:4366-4381`, `crates/infra/src/db.rs:20` (`sqlx::migrate!`).

## 1. Current structure (CONFIRMED)

Five lines, no transaction wrapper: one REQ-065 comment, then two independent
`ALTER TABLE notification_policies ADD COLUMN … BOOLEAN NOT NULL DEFAULT false` statements
(`:2-5`). No index, no constraint, no RLS change, no data rewrite. The base table is keyed
`(space_id, user_id, bot_id)` and is `FORCE` RLS with the standard tenant policy (`0056:46-52`).
`NOT NULL DEFAULT false` implicitly opts every pre-existing policy row out, with no DML.

Four architectural traits:
1. **The kind vocabulary is duplicated in three places.** `0057:6` defines
   `('slack_webhook','teams_webhook')`; `0058` encodes the same two kinds as column *suffixes*;
   `push.rs:151-162` re-encodes them as `destination_kinds()` `if` branches. Nothing ties the three
   together; adding a third kind requires edits in all three (and in the wire/domain structs).
2. **Flag↔destination existence is not DB-enforced.** A policy may set a flag to true while the
   space has no destination of that kind; `push.rs` turns that into a silent best-effort no-op.
   That matches ADR-028's "destination delivery never fails a run", but the inconsistent state is
   invisible to the schema and to operators.
3. **The spec names a third flag with no column.** The design spec (`:161`) lists
   `destination_browser BOOLEAN NOT NULL DEFAULT true`; grep finds no such column anywhere in
   `0058`, the store, or the wire (CONFIRMED). Browser delivery is gated only by
   `notify_waiting`/`notify_completed`; this is an unresolved spec↔schema seam, not a defect to
   silently fix.
4. **The whole migration is a default, not a contract.** `DEFAULT false` is the only guardrail:
   there is no constraint for "at least one channel enabled" (all-false is legal, per ADR-028
   opt-in), no audit of when a flag was flipped, and no ordering guard tying `0058` to `0057`.

## 2. Guiding principle for v0.4.0

`0058` is immutable: SQLx verifies a checksum via `sqlx::migrate!` (`db.rs:20`), so editing its bytes
breaks `boma migrate` on every deployed DB. Every change ships as a NEW, additive file (e.g.
`0060_notification_destination_flags_*.sql`) that only creates objects, adds defaulted columns, or
adds constraints via `NOT VALID` → `VALIDATE`. Never DROP / RENAME / ALTER / DELETE / backfill rows.
The column names and the `false` default are public interface (wire/RPC via `rpc.rs:4366-4381`);
freezing them byte-for-byte is mandatory; any externally visible change is a DEVIATION for
`docs/PARITY.md` with owner approval.

## 3. Concrete upgrades (5), each with a risk

**U1 — Freeze `0058` and add an additive migration-shape contract test (structure; do first).**
Never edit the file; add a fresh-PG test running `boma migrate` that asserts the exact `0058` shape
(two boolean columns, `NOT NULL`, default `false`, no index/constraint) and that a checksum edit is
rejected. *Risk:* the assertion drifts as U3/U4 add columns. *Mitigation:* assert from one shared expected-object constant and regenerate `.sqlx` metadata same commit.

**U2 — Give the kind vocabulary a single DB-side representation, additively (boundary).** In a NEW
migration create a read-only `notification_policy_destination_kinds` view that projects a policy row
to the enabled-kind set using the same literals as `0057`, so the opt-in contract becomes greppable
and auditable instead of living only in `push.rs:151-162`. *Risk:* a second source of truth can drift
from `destination_kinds()`, and a broad GRANT could expose policy rows past the tenant policy.
*Mitigation:* grant no base-table privileges, keep it diagnostic in v0.4.0 with the domain struct
still authoritative, and add a parity test asserting view membership equals `destination_kinds()`.

**U3 — Add audit columns for opt-in, additively (observability).** New file: `ALTER TABLE
notification_policies ADD COLUMN destination_slack_enabled_at timestamptz`, and the same for teams
(nullable, no backfill), so a flag flip carries a timestamp for incident review. *Risk:* the spec
does not require these, so unused columns are debt and guessing their semantics could be wrong.
*Mitigation:* record as a GUESS under `docs/STATE.md` → "Decisions needed", leave both NULL/unused
in v0.4.0, and add no data rewrite until the owner confirms.

**U4 — Close the spec seam for `destination_browser`, additively (completeness).** If the owner
confirms the design-spec mapping (`:161`), add `destination_browser BOOLEAN NOT NULL DEFAULT true`
in a NEW file — the default preserves current behavior for all existing rows. *Risk:* no store,
wire or push code consumes it today (CONFIRMED), so the column would be inert until a documented
handler lands, and a wrong default would silently change who gets pushes. *Mitigation:* decide in
`docs/STATE.md` first; ship column only with a companion comment recording "inert in v0.4.0", and
gate any behavior change as a DEVIATION.

**U5 — Add a flag↔destination diagnostic, not a blocking constraint (invariant).** New file: a view
joining enabled flags to `notification_destinations` by `(space_id, kind)` that lists opted-in kinds
with no matching destination, plus an additive `NOT VALID` CHECK only if the owner wants the pair
enforced. *Risk:* a blocking CHECK/trigger would change ADR-028's best-effort semantics into a hard
error (a DEVIATION), and `FORCE` RLS plus `SECURITY DEFINER` could widen visibility. *Mitigation:*
ship the view read-only first, use a fixed `search_path` for any future definer function, and defer
any `VALIDATE` to a reviewed step.

## 4. Cross-cutting risks / sequencing

- Order **U1 → U2 → U3 → U4 → U5**: freeze/test first, then boundary, observability, spec seam,
  invariant. Each ships as a separate additive file; U2-U5 only after their ADRs / decisions are
  approved.
- Coordinate with the reliability lens on this same file: this brief owns only migration structure;
  store upsert/`COALESCE` semantics, cap counting and error mapping stay there. Freeze the flag
  names, the `false` default, the `kind` literals and the tenant RLS expression; regenerate `.sqlx`
  metadata and have CI run `boma migrate` on a fresh DB plus replay the REQ-065 suites.
- No new dependency; work is SQL + migration-level tests. Mark claims CONFIRMED/GUESS per `AGENTS.md`
  and gate with `cargo fmt --check`, clippy `-D warnings`, golden and contract suites.

## 5. Out of scope

The `NotificationPolicy` domain struct, wire/RPC DTOs, `push.rs` fan-out, destination sealing under
the space DEK, and any in-place edit of `0058` — each belongs to another brief or is forbidden by
the checksum invariant.

## 6. Evidence & suggested tests

- CONFIRMED this session: `0058` (:1-5), `0056` (:34-52), `0057` (:6,16-25), `spec/01:610-618`,
  ADR-028 (:11-48), design spec (:154-178), `notification.rs:84-126`, `push.rs:151-162`,
  `notifications.rs:62-77,190-255`, `wire.rs:2230-2263`, `rpc.rs:4366-4381`, `db.rs:20`.
- GUESS (re-verify before implementing): PG fast-default avoids a physical rewrite of existing
  policy rows; enabled-at columns are acceptable unused debt; the schema is clean enough for any
  future `VALIDATE`.
- Suggested names: `req_065_migration_0058_is_immutable_and_shape_stable` (U1),
  `req_065_policy_kind_view_matches_push_destination_kinds` (U2),
  `req_065_no_dangling_destination_opt_in` (U5).

Line budget: <120. Status: proposal only; READ-ONLY worker, no code changed and no tests run.
