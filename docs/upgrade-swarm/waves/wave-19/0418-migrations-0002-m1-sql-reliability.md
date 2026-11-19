# Worker #418 — `migrations/0002_m1.sql` reliability proposal (v0.4.0)

- Module: `migrations/0002_m1.sql` (M1); lens: reliability; READ-ONLY proposal.
- CONFIRMED (read this session): all 299 lines of `0002_m1.sql`; `0001_init.sql` L1-18;
  `crates/infra/src/db.rs` L19-20/73 (`sqlx::migrate!`, forward-only, checksummed);
  `crates/infra/src/tenant.rs` L116-140 (`set_config(..., true)`, transaction-local);
  `spec/02-data-model.md` (closed role/status/trigger sets); `spec/05-known-defects.md` D-09;
  `spec/06-decisions-needed.md` DEC-05. Proposals/risks are GUESS (not implemented, not tested).
- **Additive only:** no edit to `0002_m1.sql` — all DDL in a NEW later migration
  (e.g. `0060_m1_reliability.sql`) so applied checksums stay valid. Externally visible changes
  are flagged DEVIATION below.

## Scope note

The file is the tenancy foundation: global tables (L70-113), tenant tables (L122-236), three
helpers (L27-53), RLS policies (L249-280). Mapped onto it: idempotency → migration/nonce replay;
constraints → closed sets and cross-table tenant consistency; RLS → context sourcing and coverage.

## Failure modes

### FM1 — Idempotency: two replay paths (CONFIRMED)
- Re-application is only idempotent through sqlx's `_sqlx_migrations` ledger (`db.rs` L20/73).
  The DDL is not: bare `CREATE TABLE` (L70-226), `CREATE POLICY` (L252, L272-277) and bare
  `CREATE INDEX` (L289-299) all fail on a second run outside that ledger, or after a checksum
  change from **editing this file in place** — which breaks startup on every deployed DB.
- `SET search_path TO public` L19 is session-scoped, not `SET LOCAL`: the migration connection
  keeps it after commit (harmless today, but implicit).
- Nonce replay (REQ-002) is guarded by partial unique indexes L186-188/L211-213 — good. But under
  RLS, `INSERT ... ON CONFLICT DO NOTHING` on a nonce whose row is invisible to the current tenant
  returns **zero rows**: the retry cannot read back its original message and may 409/loop. No narrow
  read path for the original result exists here.

### FM2 — Constraints: invalid states are representable (CONFIRMED)
- Closed sets are unconstrained `text`: `organization_members.role` L96, `space_members.role` L136
  (owner/admin/member), `messages.role` L173, `runs.status`/`trigger` L196-197, `events.type` L220,
  `invitations.status` L234 (pending/approved/declined/revoked). A buggy writer persists a value
  the state machine cannot interpret (REQ-012/017); there is no second line of defence.
- `invitations.token_hash` L231 has only a non-unique index L297; `spaces.is_default` L126 has no
  single-default constraint → duplicate single-use tokens, two default spaces.
- RLS is per-row: FKs key on `id` alone (L171, L193-194, L218) and `bots.id`/`threads.id` lack
  `(space_id, id)` uniqueness, so a child can point at a parent in another space. A system-path
  write creates a cross-tenant link the child's own policy still considers valid.
- Soft text refs with no FK (`bots.thread_id` L150 by design; `messages.bot_id`/`author_user_id`/
  `run_id` L175-178; `runs.source_message_id` L199) dangle after cascades. `created_at NOT NULL`
  with no `DEFAULT` makes an omitted field a hard insert failure; seq columns L162-164 lack `> 0`.

### FM3 — RLS: context sourcing and coverage (CONFIRMED)
- `current_space_id()` L30 uses `current_setting(..., true)`: unset → NULL → deny. Good fail-closed.
  `is_system()` L41 trusts any non-empty `boma.system_reason`; the closed enum is enforced only in
  Rust (`tenant.rs` L32, D-09). Any path that sets the GUC with session scope, or from untrusted
  text, gets full cross-tenant access. Both helpers accept a session-scoped value as readily as
  the transaction-local one written by `with_tenant` (`tenant.rs` L120-129), so a mis-scoped
  pooled connection can leak a tenant/reason; RLS cannot distinguish them.
- `FORCE RLS` L250/L271 covers the owner, but superuser/`BYPASSRLS` still bypass, unasserted at
  startup. Coverage is manual: a later tenant table with `space_id` and no policy is silently
  exposed. Policies are PERMISSIVE (default), so a future additive policy ORs and can widen access.

## Concrete hardening for v0.4.0 (GUESS — not implemented)

- **H1 (FM1):** put all DDL in NEW `0060_m1_reliability.sql`; guard its own statements with
  `to_regclass`/`pg_policies`/`CREATE INDEX IF NOT EXISTS` so it is re-runnable; test migrating
  fresh then re-running with no checksum error. *Risks:* treat 0060 as immutable once shipped — a
  later edit has the same checksum trap.
- **H2 (FM1):** replace L19 with `SET LOCAL search_path TO public`. *Risks:* no-op outside a
  transaction, so keep migrations transactional.
- **H3 (FM1):** on nonce unique violation, read the original row by `(space_id, thread_id,
  client_nonce)` via a narrowly scoped, audited system-context helper and return it (REQ-002).
  *Risks:* any widening is a leak — pin `search_path`, `SECURITY DEFINER`, log the lookup;
  returning a row the caller could not otherwise see is externally visible → DEVIATION/approval.
- **H4 (FM2):** CHECK constraints via `NOT VALID` then `VALIDATE CONSTRAINT` for the closed sets,
  `next_*_seq > 0`, unique `lower(email)`, unique invitations `token_hash`, and partial unique
  `spaces(organization_id) WHERE is_default`. *Risks:* pre-existing bad rows block VALIDATE —
  audit/clean first; new statuses/roles need a migration; case-insensitive email changes sign-in
  → DEVIATION.
- **H5 (FM2):** add `UNIQUE (space_id, id)` on parents (`spaces`, `bots`, `threads`) and composite
  FKs `(space_id, thread_id/bot_id)` on children; `DEFAULT now()` on `created_at`. *Risks:* index
  write cost; wrong-space links must be repaired or FKs stay `NOT VALID`; system paths must set the
  child's real space. Keep soft text refs soft (parity) and document them.
- **H6 (FM3):** add `boma.system_reasons(reason text PRIMARY KEY)` seeded from `SystemReason` and
  redefine `is_system()` to require a seeded value; add a transaction-local sentinel GUC checked
  by both helpers; CI check that every `space_id` table has ENABLE + FORCE + policy; startup assert
  that the pool role is neither superuser nor `BYPASSRLS`. *Risks:* a missed seed row silently
  downgrades a system path to an empty tenant (fail-closed outage, not a leak); paths outside
  `with_tenant` need a break-glass; restrictive-vs-permissive must be chosen so additive policies
  cannot widen access.
- **H7 (tests/observability):** REQ-tagged tests: migrate twice from scratch; nonce replay under
  RLS returns original; session GUC cannot grant system access; wrong-space FK rejected; every new
  tenant table has a policy. Counters for nonce violations/system-reason denials, no PII or ids in
  logs. *Risks:* more testcontainers time; H6 invalidates any test pinning today's GUC behavior.

## Verification (proposed; NOT run — read-only)
- `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, plus `sqlx migrate run` from
  empty and a re-run; assert `_sqlx_migrations` checksums unchanged.
- `spec/golden/send-message-replay.json` (REQ-002) and REQ-012/043 tests stay green; add a
  wrong-space-link case only if it asserts DB, not wire, behavior.

## Constraints / parity
- Never edit `0002_m1.sql` in place; additive migration only. Keep fail-closed RLS and the
  transaction-local tenant contract. No additive policy that ORs with the existing ones; no
  soft-ref → FK conversion without an ADR (parity change). Any newly rejected value or
  replay-returned row is externally visible → `docs/PARITY.md` + approval. No secrets/ids/PII in logs.
