# 0528 — migrations/0058_notification_destination_flags.sql — reliability (v0.4.0)

Worker #528 · module `migrations/0058_notification_destination_flags.sql` · lens: reliability ·
READ-ONLY review of `rebuild-workspace/.worktrees/before-m5`. Proposals only; no code changed.
English. **Additive only** — every proposal is a new statement, test or `006x` migration file;
none edits 0058 or any applied migration in place.

## Ground truth (CONFIRMED by reading the tree this session)

- 0058 is two `ALTER TABLE notification_policies ADD COLUMN ... BOOLEAN NOT NULL DEFAULT false`
  statements — no transaction marker, `COMMENT`, `CHECK`, index or backfill.
- The table (created in 0056) has PK `(space_id, user_id, bot_id)`, CASCADE FKs, `notify_waiting
  DEFAULT true`, `notify_completed DEFAULT false`, `daily_cap DEFAULT 20 CHECK (>= 0)`, and RLS
  `ENABLE` + `FORCE` with policy `space_id = boma.current_space_id() OR boma.is_system()`.
- 0057 holds the space-wide, sealed destinations (`kind CHECK IN ('slack_webhook','teams_webhook')`),
  Owner/Admin-managed per REQ-065.
- Consumers CONFIRMED: `domain/notification.rs` bools default false, `SetNotificationPolicy` uses
  `Option<bool>`; `wire.rs` DTO uses `Option<bool>`; `rpc.rs` maps straight through; the PG store
  upserts with `COALESCE($7/$8, existing)` and `get_policy` falls back to `NotificationPolicy::
  defaults()` (both false) when no row exists; `push.rs::destination_kinds()` gates fan-out and
  `push_destinations()` is best-effort against a shared `daily_cap`.
- The M15.2 plan explicitly rejects a `destination_browser` flag (browser is the baseline when a
  policy row exists); the older design doc that floated one is superseded. Do not add it.
- `db.rs:20` embeds `sqlx::migrate!`, `MIGRATOR.run` (72–74) is forward-only, one transaction per
  file, checksummed in `_sqlx_migrations`. Editing 0058 in place changes its checksum and refuses
  to start everywhere — a deploy-wide outage. All new work belongs in a new file.

## What it already does right (keep, do not re-propose)

- `NOT NULL DEFAULT false`: metadata-only add on PG 11+; fail-closed opt-in, no pre-existing member
  is silently enrolled into an external channel (REQ-065 "opted-in initiators").
- Both ALTERs share one file, hence one sqlx transaction: no half-upgraded schema on failure.
- RLS is inherited unchanged; booleans carry no tenant-identifying data, so no new leak surface.
- No index on the flags (low cardinality) — correct.

## Failure modes → concrete additive hardening

### F1. Flag semantics (the largest gap)

`false` is overloaded: "never opted in", "opted out", and "no destination of this kind configured
in the space" all look identical. A flag set true with no matching destination row is a silent
no-op (`destination_kinds` → `list()` empty → early return): the opt-in cannot be honoured and
nothing is logged. A DB `CHECK` cannot fix this — destination rows live in another RLS-scoped table.
The `Option<bool>` patch plus `COALESCE` also means a JSON `null` means "leave unchanged", never
"turn off"; a client treating `null` as `false` silently fails to disable a shared webhook.

- (P1) `COMMENT ON COLUMN` for both flags in a new `006x` file: false = channel not enabled for
  this (space, member, bot), independent of `notify_waiting`/`notify_completed`; true still
  requires a matching space destination and draws from the shared `daily_cap`.
- (P2) REQ-065 test: flag true + no destination of that kind → no send, no cap consumed, no error;
  flag false + configured destination → no send.
- (P3) REQ-065 test: `set_policy` with `null` flags preserves `true`; explicit `false` clears it.
- (P4) warn log / metrics counter when `destination_kinds` is non-empty but `list()` returns no row
  of that kind — observability only. Do **not** add a CHECK tying flags to `notify_*` or to
  destination existence: the spec says destinations go "alongside the browser" behind the same
  policy, and a strict CHECK would reject legitimate rows.

### F2. RLS

No new policy is introduced, but `FORCE RLS` means any future backfill or verification `UPDATE`
outside `tenant::in_space`/`as_system` silently affects zero rows and reports success.

- (P5) REQ-065 test: a member of space B cannot `SELECT` or `UPDATE` space A's flags; a
  system-context backfill sees all rows.
- (P6) Runbook note: any flag backfill must use the system path, never a bare `UPDATE`.
- Current reads all go through `tenant::in_space` or a `TenantContext` (CONFIRMED by grep); keep it.

### F3. Defaults and upgrade behaviour

Existing rows receive `false` at ALTER time and `NotificationPolicy::defaults()` also returns
`false`: members with browser notifications on are not opted into webhooks until they re-opt in.
That is intended (fail-closed) but must be documented.

- (P7) `docs/UPGRADING.md` note: v0.4.0 requires a per-member re-opt-in for each destination kind.
- Each ALTER takes `ACCESS EXCLUSIVE` for the migration transaction — metadata-only, but it still
  blocks app reads/writes until commit. (P8) pre-flight table size and migrate before serving (the
  embedded stack already does initdb → migrate → serve); (P9) group future flag adds into one ALTER
  in a new file; consider `ADD COLUMN IF NOT EXISTS` there for partial-retry safety (not needed for
  0058 itself).
- (P10) REQ-065 test that `daily_cap` is shared, not double-counted across browser and webhook
  sends — the "same budget" claim is GUESS until that test exists.

### F4. Process

- (P11) Record the downgrade (`DROP COLUMN`, `DROP COMMENT`) for any new `006x` file per spec
  04 §4.9; never edit 0058.
- (P12) Contract test: a fresh migrate from empty DB yields both columns false on a seeded policy;
  mark claims CONFIRMED only after that output exists.

## Risks and trade-offs

- R1. P1/P4/P6–P12 add comments, observability, tests and process, not schema changes — no lock or
  checksum risk; every item is independently revertable.
- R2. If the owner instead intends destination sends to follow `notify_waiting`/`notify_completed`,
  that is a behaviour change → DEVIATION + ADR, not additive (GUESS: spec is silent).
- R3. Keeping no `destination_browser` flag preserves parity; adding one later is a new flag with
  its own default, not an edit to 0058.
- R4. No index, no CHECK, no in-place edit, no wire change proposed.

## Suggested rollout for v0.4.0

Wave A (this doc): P1–P12 in new files; run `cargo test -p boma-infra` and the REQ-065 infra tests.
Wave B only on owner confirmation of R2: ADR + DEVIATION entry. READ-ONLY pass — no command was
executed here; CONFIRMED means read from the tree this session, GUESS is flagged inline.
