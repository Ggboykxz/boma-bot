# 0489 — `migrations/0039_routines_github_enabled.sql` — architecture (v0.4.0)

- Worker #489 · Lens: architecture · Target: v0.4.0 · Read-only review; no source or migration changed.
- Module: `migrations/0039_routines_github_enabled.sql` (7 lines) at `before-m5` (v0.3.0-rc5).
- Read for context (CONFIRMED by direct reads): `0014`/`0015`/`0016`/`0041` migrations,
  `crates/infra/src/db.rs`, `stores/routines.rs`, `stores/rows.rs`, `usecases.rs` (`webhook_wake`
  1943–2112), `ports.rs`, `spec/01-requirements.md` REQ-050, `spec/02-data-model.md:79-87`, sibling proposals.

## Current structure (CONFIRMED)

`0039` is a header comment (REQ-050) plus one statement:

```sql
ALTER TABLE routines ADD COLUMN github_enabled boolean NOT NULL DEFAULT false;
```

No `COMMENT`, index, `CHECK`, backfill, or policy accompanies it (RLS is table-level from `0014`, so
the column inherits tenant isolation). `DEFAULT false` makes opt-in safe for existing rows. The
`routines` entity is **fragmented**: `0014` (base), `0015` (`runs.routine_id`), `0039`
(`github_enabled`), `0041` (`email_senders`), while `02-data-model.md:83` still lists `webhookEnabled`,
`messageProvider`, model fields and `watch`/`watchState` in no migration (GUESS: deferred).

Consumption path (CONFIRMED): `webhook_wake` authenticates the bot's `WEBHOOK` secret, starts a
conversational run (`RunTrigger::Webhook`), then — only if `payload_is_a_repository_event(payload)`
(an app-side JSON check for a `repository` object) — calls `routines.list(bot_id, space_id)` and
filters in Rust with `active && github_enabled` (usecases.rs:2060-2096). Each match gets its own
`runs.create` with `RunTrigger::Routine` and `routine_id` set. `list()` queries
`WHERE bot_id=$1 AND space_id=$2 ORDER BY created_at, id` with no matching index (0442/R4); the DB
stores a bare boolean while the *meaning* of "repository event" lives only in Rust.

**Migration mechanics constraint (CONFIRMED):** `crates/infra/src/db.rs:20` embeds migrations via
`sqlx::migrate!("../../migrations")`; applied versions and checksums are recorded in
`_sqlx_migrations`, and `req_053_the_migrations_apply_and_are_recorded` asserts idempotency. Editing
`0039` in place would break the checksum, so **every upgrade below is a new migration with the next
free version (≥ 0060)** — never an edit to `0039` itself.

## Architectural assessment

The column is parity-correct and minimal, but it is one half of an implicit "external wake channel"
model: `github_enabled` (a bool) and `email_senders` (`0041`, an array) are two additive opt-ins
governed by separate use cases, with no schema-level contract, no audit of webhook-triggered wakes,
and no index for the lookup that consumes the flag. Three layers disagree on where the contract
lives — comment (`0039`), Rust (`usecases.rs`), spec data model (`02-data-model.md`). All upgrades
below close that gap additively; none changes observable behavior, so no new DEVIATION is expected
(final parity sign-off remains the operator's).

## Concrete upgrades (additive only)
### 1. Self-describing column and entity provenance — metadata, low risk
New migration adds `COMMENT ON COLUMN routines.github_enabled` spelling out REQ-050, `false` =
opt-in, that the webhook wakes the bot itself on every call, and that "repository-shaped payload"
is decided by `payload_is_a_repository_event` in `usecases.rs`; plus a `COMMENT ON TABLE routines`
recording ownership by `0014 + 0015 + 0039 + 0041` and the two wake channels.
- **Risk:** low; no behavior change. Only risk is drift if the payload rule changes. Mitigation:
  name the REQ id and the exact function in the text, matching sibling proposals' convention.

### 2. Index the opt-in wake lookup — new index (+ optional additive store method), medium risk
Add `CREATE INDEX routines_bot_github_idx ON routines (bot_id, space_id, created_at, id)
WHERE active AND github_enabled` so the webhook wake can fetch opt-in routines without scanning all
of a bot's routines. A partial index is unused until the query carries the predicate: either add an
additive `RoutineStore::list_github_enabled(bot_id)` query (`WHERE bot_id=$1 AND space_id=$2 AND
active AND github_enabled`) used by `webhook_wake`, or add a general `(bot_id, space_id)` index to
serve `list()` as written (0442/R4). Keep all existing indexes untouched.
- **Risk:** medium. Write amplification on a frequently-updated table; the planner may ignore the
  index on a small `routines` table, and a Rust post-filter means today's query cannot use the
  partial form. Mitigation: measure with `EXPLAIN ANALYZE` before/after on a seeded DB, keep only
  indexes the plan uses, and gate any consumer change behind the REQ owner.

### 3. Audit webhook-triggered wakes — new table + RLS, medium risk
CONFIRMED gap: due wakes insert into `routine_runs` (`routines.rs:236/292/335/391` via
`record_fired`), but the webhook path creates a routine run directly (`usecases.rs:2076-2093`) with
**no `routine_runs` row**, so `github_enabled` fires are invisible to that audit log. Additively
create `routine_webhook_wakes (id bigserial PK, routine_id text REFERENCES routines(id) ON DELETE
SET NULL, space_id text NOT NULL, run_id text REFERENCES runs(id) ON DELETE SET NULL, delivered_at
timestamptz NOT NULL)` with a `routine_webhook_wakes_tenant_isolation` policy byte-identical to the
`0014` predicate (`space_id = boma.current_space_id() OR boma.is_system()`), and a partial index on
`(routine_id)`. Do **not** widen the existing `routine_runs.outcome` CHECK — that rewrites a
constraint and breaks additive-only.
- **Risk:** medium. New table + app write is a schema change with no consumer yet; `ON DELETE SET
  NULL` keeps history readable (consistent with `0015`, unlike `routine_runs`' `CASCADE`). Mitigation:
  keep it internal, add a REQ-050 integration test that one repository payload writes one row per
  opted-in routine, and record the feature as a proposal in `docs/STATE.md` before exposing it.

### 4. Unified wake-channel read model — new view, low/medium risk
Add `CREATE VIEW boma.routine_wake_channels AS SELECT id AS routine_id, bot_id, space_id, active,
github_enabled, (cardinality(email_senders) > 0) AS email_enabled FROM routines;` to make the
`0039`+`0041` opt-in model queryable in one place, without touching either column. A view is a new
object, so it is additive and reversible via `DROP VIEW`.
- **Risk:** low/medium. An unused view is dead weight, and duplicating the trigger logic invites
  drift if the wake rules change. Mitigation: derive purely from existing columns, add a test that
  the view agrees with `usecases.rs`, and do not route runtime writes through it.

### 5. Resolve the spec/DB representation gap — additive columns, medium risk
`02-data-model.md:83` still lists `webhookEnabled`, `messageProvider`, model fields, `watch` and
`watchState` with no migration. Decide explicitly in `docs/STATE.md` whether v0.4.0 lands them; if
yes, add them **additively with safe defaults** (`webhook_enabled boolean NOT NULL DEFAULT false`,
`message_provider text NULL`, `watch jsonb NOT NULL DEFAULT '{}'`, etc.) in a new migration — not by
editing `0014`/`0039`. Note that `webhookEnabled` (per-bot webhook exposure) is distinct from
`github_enabled` (per-routine repository opt-in) and must not be conflated.
- **Risk:** medium. GUESS on scope; DB spelling (`snake_case` vs spec `camelCase`) is unpinned, and
  unused columns are harmless but add schema churn. Mitigation: confirm against `spec/openapi.yaml`
  and `spec/golden/` first; keep public interface names (`github_enabled` / `githubEnabled`) exactly
  as-is for compatibility (AGENTS.md).

## Cross-cutting risks / sequencing

- Order 1 → 2 → 4 → 3 → 5: comments/index/view are local and measurable; the audit table and columns
  need a writer/spec audit first. Every step stays an independently revertable new migration.
- Freeze the public surface: column names, `DEFAULT false`, `github_enabled`↔`githubEnabled` mapping
  (`wire.rs`), the `Routine`/`NewRoutine` port fields, and the `0014` RLS predicate.
- Migration numbering must be the next free version (≥ 0060); never edit `0039` (sqlx checksum).
- Traceability: no new REQ id (REQ-050, SHOULD); `tools/trace_check.py` unaffected. Gate with
  `cargo fmt --check`, `clippy -D warnings`, the REQ-050 webhook round-trip (`m1.rs:2747`,
  `rpc_surface.rs`), `req_053` migration idempotency, and goldens. No new dependency.

## Out of scope for this module

Rewriting/backfilling `0039`, widening `routine_runs.outcome`, changing `routines.list` ORDER BY or
JSON wake-shape detection, and mail/routine orchestration (`0041`, `mail_inbound`) belong to other
modules (#490 reliability, wave-20 #441/#442); this proposal only strengthens `0039` additively.
