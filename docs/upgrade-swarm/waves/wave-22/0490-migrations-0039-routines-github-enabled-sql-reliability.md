# v0.4.0 reliability proposal — `migrations/0039_routines_github_enabled.sql` (worker #490, lens: reliability)

- **Status:** PROPOSAL (read-only review; no source or migration changed). CONFIRMED = read from this worktree; GUESS = needs a runtime/test/oracle run.
- **Target file as reviewed:** one statement — `ALTER TABLE routines ADD COLUMN github_enabled boolean NOT NULL DEFAULT false;` (lines 6-7).
- **Trace:** REQ-050; `docs/STATE.md:837`; `docs/TRACEABILITY.md:70`; consumer `crates/app/src/usecases.rs::webhook_wake` (1965-2080, filter at 2064), binds in `crates/infra/src/stores/routines.rs` (42, 66, 109, 141, 171, 219, 275), wire field `crates/api/src/wire.rs:525`.
- **Additive only:** no existing migration, column, index, policy, constraint or row is dropped, renamed or rewritten; every change below is a NEW migration (006x) or NEW object.

## Summary

0039 is the smallest migration in the tree and, for a fresh install, correct: a non-null boolean defaulting to false is inert on upgrade (existing routines keep sleeping until an owner opts in) and is natively two-valued. Its reliability risk is not the value but the *rollout and surrounding contract*: non-idempotent DDL in a possibly non-transactional runner (D1/D2), a column that silently inherits a permissive ALL policy so any tenant can flip external-trigger enrollment with no audit (R1/R2), and no DB-level support or invariant for the "webhook wakes my opted-in routines" path it exists to serve (C2/C3/C4).

## Failure modes

### Defaults (D)

- **D1 (HIGH on upgrade, GUESS PG version):** `NOT NULL DEFAULT false` is a metadata-only fast default on PostgreSQL >= 11, but a full table rewrite under ACCESS EXCLUSIVE lock on <= 10. The tree uses `cardinality`, FORCE RLS and a `boma.*` schema (CONFIRMED), consistent with PG 12+, but the pinned version was not read. Confirm before release: a rewrite on a large `routines` table is an upgrade-window outage.
- **D2 (HIGH, CONFIRMED):** no `IF NOT EXISTS`. If the runner commits the DDL but fails to record version 0039 (or does not wrap each migration in a transaction), the next startup re-runs 0039 and dies on `duplicate column "github_enabled"`. Additive-only means 0039 itself cannot be hardened; the partial state can only be healed and the runner fixed (H1).
- **D3 (MED, CONFIRMED):** the false default means every pre-existing routine is silently dormant for webhook wakes after v0.4.0 — correct by design, but an operator expecting an existing "repo watcher" to auto-enable sees no wakes and no signal. Needs a `docs/STATE.md` note, not a backfill.

### RLS / tenant isolation (R)

- **R1 (HIGH, CONFIRMED):** the column inherits `routines`' single PERMISSIVE ALL policy (0014:53-58) — no column-level grant and no audit. Any tenant member can UPDATE `github_enabled` on their own routine, enrolling it in external-trigger execution with no record of who or when. `webhook_wake` reads the flag in System context (usecases.rs:2061-2064), so a stray client toggle changes what an inbound webhook is authorized to run.
- **R2 (MED, CONFIRMED pattern / GUESS impact):** `boma.is_system()` is true for any non-empty `boma.system_reason` (0002_m1.sql), so a misconfigured system-context connection can flip flags across tenants. As in 0442 (R2), this cannot be fully fixed additively; gate and log trusted reasons for new writes.
- **R3 (LOW, CONFIRMED):** for *reads* RLS protects this column well — no new leak exists. No policy is needed for confidentiality; the gap is change control, not exposure.

### Constraints (C)

- **C1 (non-finding, CONFIRMED):** boolean + NOT NULL is inherently constrained; no malformed value is storable. No CHECK is required — recorded so no one adds a pointless one.
- **C2 (MED, CONFIRMED):** no cross-field invariant. `github_enabled = true AND active = false` is storable and silently never wakes (usecases.rs:2064 filters `active && github_enabled`). A hard CHECK would reject a legitimate "parked but opted-in" routine, so use a monitoring view, not a constraint.
- **C3 (MED, CONFIRMED):** the webhook path calls `routines.list(&member, &bot.id)` then filters in memory (usecases.rs:2061-2064). No index supports an SQL-side `WHERE active AND github_enabled` for that bot, so per-bot routine growth adds latency inside a request that has an outbound timeout budget.
- **C4 (MED, CONFIRMED):** nothing ties opt-in to a usable WEBHOOK bot secret. `bot_secrets` is UNIQUE(space_id, bot_id, name) and RLS-protected (0025:7-17); `webhook_wake` fails auth and no routine wakes when the `WEBHOOK` row is missing (usecases.rs:1985-1998). A github_enabled routine with no secret is silently dead.

## Concrete hardening (all additive; proposed NEW migrations)

1. **H1 — idempotent heal + transactional runner (D1/D2):** new `006x_github_enabled_heal.sql` with `ALTER TABLE routines ADD COLUMN IF NOT EXISTS github_enabled boolean NOT NULL DEFAULT false;` plus `COMMENT ON COLUMN` recording REQ-050 and the false default. Separately (runner change, not a migration edit; needs ADR) wrap each migration in BEGIN/COMMIT and record its version in the same transaction. Risk: `IF NOT EXISTS` silently accepts a pre-existing column with a different default/nullability, so add a `DO $$` verification that raises unless `is_nullable = 'NO' AND column_default = 'false'`; the runner change is externally invisible but must pass a simulated mid-migration crash test.
2. **H2 — change audit (R1/R2):** new `006x_github_enabled_audit.sql`: table `routine_github_enabled_audit` (id, routine_id, space_id, old_value, new_value, actor_reason text, occurred_at) using the repo RLS DO-loop, plus a BEFORE UPDATE OF github_enabled trigger on `routines` that inserts one row only when `OLD.github_enabled IS DISTINCT FROM NEW.github_enabled`. Purely observational, no behavior change. Risk: one extra INSERT per flag toggle (cheap); the audit table needs its own retention policy; `actor_reason` must hold a trusted reason token only, never secret material — GUESS: confirm what `boma.current_space_id()` / `system_reason` expose in trigger context.
3. **H3 — monitoring views (C2/C4):** new `006x_github_enabled_health.sql`: views `boma.github_enabled_inactive` (opted-in but `NOT active`) and `boma.github_enabled_no_secret` (opted-in routine whose bot has no `bot_secrets` row named `WEBHOOK`). Expose ids/names only, never ciphertext (0025:5). Risk: define views to run under invoker RLS so they cannot widen visibility; verify with a tenant role on real Postgres.
4. **H4 — supporting index (C3):** new `006x_github_enabled_index.sql`: `CREATE INDEX routines_github_due_idx ON routines (space_id, bot_id) WHERE active AND github_enabled;`; leave the existing due/thread indexes untouched. Optional paired app change to filter in SQL. Risk: write amplification on every active/github toggle; row counts are small today (GUESS) — confirm with EXPLAIN before enabling the app-side query change.

## Risks / trade-offs

- H2/H3 add objects with no reader yet: insurance for incident forensics, not a feature. Do not expose them via RPC without an ADR (DEVIATION).
- H1's runner fix is the only item that changes deploy behavior; ship it behind a staging crash-injection test. If the runner already wraps migrations in a transaction (GUESS — not verified here) D2 is already closed and H1 reduces to the heal migration.
- Do NOT add a hard `CHECK (NOT github_enabled OR active)` or a FK-to-secret: both reject states the app currently stores and would be externally visible changes needing owner approval.
- No change to `webhook_wake` semantics: all fixes are DB-side or observability-side, so REQ-050 parity is preserved.

## Verification to add

Real-Postgres integration tests (`--locked`, RLS enabled): a simulated partial apply of 0039 is healed by H1 and a duplicate apply is a no-op (D2); a `github_enabled` update writes exactly one audit row and a no-op update writes none (R1); a tenant role cannot read another tenant's routine through the health views (H3); EXPLAIN uses the new partial index for the bot+github lookup (C3); a github_enabled routine with no `WEBHOOK` secret appears in `github_enabled_no_secret` (C4). All source findings are CONFIRMED except the pinned PG version, `system_reason` visibility inside triggers, and current row counts (GUESS).
