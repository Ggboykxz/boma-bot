# 0416 — migrations/0001_init.sql — reliability (v0.4.0)

Worker #416 · module `migrations/0001_init.sql` · lens: reliability · READ-ONLY review of the
v0.3.0-era `before-m5` tree. Proposals only; no code changed. **Additive only** — each proposal
is a new statement or new migration file; none edits an existing migration in place.

## Ground truth (CONFIRMED by reading the tree this session)

- 0001 creates one global table `app_meta(key text PRIMARY KEY, value jsonb NOT NULL,
  updated_at timestamptz NOT NULL DEFAULT now())`, inserts `('schema','{"milestone":"M0"}')` with
  `ON CONFLICT DO NOTHING` + a COMMENT. No RLS, grants, triggers, CHECKs or FKs.
- 0002 installs `boma.current_space_id()` / `boma.is_system()` and the RLS pattern for tenant
  tables (`ENABLE` + `FORCE ROW LEVEL SECURITY`, policy `... OR boma.is_system()`, default-deny).
  Helpers do **not** exist at the time 0001 runs — a policy in 0001 cannot call them.
- `crates/infra/src/db.rs` embeds `sqlx::migrate!("../../migrations")`; `_sqlx_migrations` records
  success + checksum, `boma migrate` is an out-of-band deploy step (spec 04 §4.9), and no
  `-- no-transaction` directive exists, so each migration runs in one transaction: DDL + seed
  are atomic. Spec 04 §4.5 / REQ-043 require default-deny isolation with a system escape; app_meta
  is "never tenant data, never readable by a client." Spec 04 §4.9 requires additive migrations
  and a documented downgrade story.

## What it already does right (keep, do not re-propose)

- `PRIMARY KEY` gives a btree on `key` and implies `NOT NULL UNIQUE`; for a handful of metadata rows
  that is the only index worth having. Do not add GIN on `value` — write amplification, no reader.
- `ON CONFLICT DO NOTHING` makes a replayed seed non-destructive: it cannot reset a milestone
  that a later migration bumped. Keep this property.
- Seed + DDL in one migration transaction; a partial `app_meta` never commits alone.
- The COMMENT states the security intent even though it is not yet enforced — useful contract.

## Failure modes → concrete additive hardening

### F1. Weak input constraints on a control-plane table (missing constraints)

`value jsonb NOT NULL` accepts scalars/arrays/null-objects; a reader doing `value->>'milestone'`
gets NULL silently and reports "unknown version." `key` accepts empty, huge or garbage strings.

P1. In a **new** migration `0060_app_meta_hardening.sql`, add
`ALTER TABLE app_meta ADD CONSTRAINT app_meta_value_is_object CHECK (jsonb_typeof(value)='object');`
P2. Add `ALTER TABLE app_meta ADD CONSTRAINT app_meta_key_shape CHECK (length(key) BETWEEN 1 AND 128
   AND key ~ '^[a-z][a-z0-9_]*(-[a-z0-9_]+)*$');` — documents a key namespace and bounds abuse.
P3. Keep the PK as the sole index; add a COMMENT explaining why no index on `value`/`updated_at`
   is warranted, so a future "helpful" index is rejected with a reason.

### F2. Isolation is documented, not enforced (RLS gaps / privilege gaps)

app_meta has no RLS and no explicit REVOKE. If a client role ever gains SELECT (broad grant, future
role change, a dynamic table name), metadata is exposed — the exact leak REQ-043 prevents. Being
global like `users`/`sessions` is no defence: those rely on app-layer checks, app_meta's comment
promises clients never read it.

P4. In `0060` (after `boma.is_system()` exists), `ALTER TABLE app_meta ENABLE ROW LEVEL SECURITY;`
   and `FORCE ROW LEVEL SECURITY;`, with
   `CREATE POLICY app_meta_system_only ON app_meta USING (boma.is_system()) WITH CHECK (boma.is_system());`
   This is default-deny for tenant connections and preserves migration/ops writes under the
   existing system reason. Do **not** inline `current_setting(...)` in 0001 — the helper is the
   contract the CI check greps for (spec 04 §4.5).
P5. Audit grants (`\dp app_meta`): REVOKE all client-role privileges, grant only the migration /
   system role. Prefer privileges as the first line of defence, RLS as the backstop.
P6. Add a regression test (name carries REQ-043) that a tenant-scoped connection SELECTing
   app_meta gets zero rows and that a `boma.system_reason`-scoped connection sees the seed.

### F3. Weak cascade/immutability semantics (drift and accidental mutation)

app_meta has no FKs, so no cascade *correctness* risk today; the opposite is the risk — nothing
stops an operator or stray script from UPDATE/DELETE-ing the deployment marker, and `updated_at`
never advances on UPDATE (no trigger), so diagnostics go stale. The seed is also hardcoded `M0`:
once later migrations move the product to M5 the marker stays M0 unless something bumps it — a
misleading source of truth for on-call.

P7. Add a `BEFORE UPDATE OR DELETE` trigger that raises an exception for `OLD.key = 'schema'`
   (or for all rows if app_meta is intended as append-only metadata), making the marker immutable.
P8. Add a `BEFORE UPDATE ... SET NEW.updated_at = now()` trigger so any legitimate mutation
   carries an honest timestamp; supersedes the misleading DEFAULT-only column.
P9. Add `0061_app_meta_milestone.sql` that upserts the current milestone with
   `ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now() WHERE app_meta.value
   IS DISTINCT FROM EXCLUDED.value` — idempotent and self-healing, and it *does* advance the marker.
   Derive the value from the highest applied `_sqlx_migrations.version` rather than a literal, so
   it cannot drift again.
P10. Add a test asserting readiness is driven by `_sqlx_migrations` (spec 04 §4.5), **not** by the
   app_meta marker; app_meta is observability only. If any code already reads the marker, list it
   in docs/STATE.md as a dependency before P7/P9 land.

### F4. Migration-pipeline integrity (checksum / downgrade)

The main trap is process: editing 0001 changes its SQLx checksum, so `MIGRATOR.run` refuses to start where it already applied — a deploy-wide outage.

P11. Land every proposal above strictly as new migrations (`0060`, `0061`), never as edits to
   `0001`. Add a CI grep/gate (or a note in the deploy runbook) forbidding edits to applied
   migration files, mirroring the `boma.space_id` gate in ADR-002.
P12. Record the additive downgrade story required by spec 04 §4.9: document the `DROP CONSTRAINT` /
   `DROP POLICY` / `DROP TRIGGER` / `DROP FUNCTION` statements for `0060`–`0061` in docs/ so an
   operator can roll back without hand-writing SQL under pressure.

## Risks and trade-offs

- R1. FORCE RLS + system-only policy (P4): any ops/maintenance query that forgets
  `boma.system_reason` sees an empty table and may misdiagnose. Mitigate by routing all metadata
  access through existing system-path helpers and documenting the escape.
- R2. CHECK on key shape (P2) could reject a future key containing dots or uppercase. Mitigate by
  confirming the namespace with the maintainers; if unknown, ship only P1 and the length bound.
- R3. Immutability trigger (P7) blocks a legitimate future edit to the marker. That is intentional
  for `schema`; if app_meta later becomes a general KV store, scope the trigger to `schema` only.
- R4. `updated_at` trigger and extra policy add minor write overhead on a tiny table — negligible;
  no N+1 or hot path touches app_meta.
- R5. P9 changes the marker's value (M0 → current). It is metadata, not an API, but confirm no
  golden/test asserts the literal `"M0"` before landing (GUESS until that grep is run).
- R6. All proposals are independently revertable and none changes gate semantics or lockfiles.

## Suggested rollout for v0.4.0

Wave A (this doc): P1–P3, P4–P6, P11–P12 — constraints, isolation, pipeline integrity.
Wave B: P7–P10 — immutability, timestamp, milestone self-healing, readiness test.
Run `cargo test -p boma-infra` (REQ-053 plus the new REQ-043 test) after each wave; mark each
claim CONFIRMED only after that output exists.

_Scope note: references are against `before-m5` as read this session; no CI, DB or test command was
run in this READ-ONLY pass. P4/P5/P7–P10 assume the 0002 helpers and system-role conventions above
(CONFIRMED from source); exact client-role names and any reader of the `schema` marker remain GUESS
pending the greps noted in P5/R5._
