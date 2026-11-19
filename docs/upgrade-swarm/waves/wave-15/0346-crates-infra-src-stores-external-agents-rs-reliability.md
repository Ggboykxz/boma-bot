# Worker #346 — `crates/infra/src/stores/external_agents.rs` — reliability lens — v0.4.0 proposal

- Module: `crates/infra/src/stores/external_agents.rs` (137 lines) · Lens: reliability · READ-ONLY.
- Status: proposal only, no code changed. CONFIRMED = read this session in `before-m5`
  (v0.3.0-rc5): this store, `infra/src/tenant.rs`, `app/src/ports.rs:4341-4371`,
  `app/src/usecases.rs:9532-9590`, `domain/src/external.rs`,
  `migrations/0054_external_agents.sql`, `docs/adr/026-external-agents.md`. Concurrency and
  runtime outcomes are GUESS until testcontainers tests run.
- REQ in scope: REQ-059 (external agents over AG-UI); ADR-026 §4 (failure semantics are ours).

## Current behavior (CONFIRMED)

- `PostgresExternalAgentStore { pool }` implements the port with three methods, each one
  `tenant::in_space(...)` transaction setting `boma.space_id` for RLS (`tenant.rs:75-84,107-143`).
- `connect` (53-86) is an `INSERT ... ON CONFLICT (bot_id) DO UPDATE` (61-71). Tenant scope comes
  from **`config.space_id`** (57-58), and both `created_at` and `updated_at` come from the
  caller-supplied `saved_at` string (62, 78). `updated_at` is overwritten on every conflict.
- `disconnect` (88-108) and `find` (110-136) instead take the space from
  `context.space_id()` (94, 116). Both return success on zero rows (no `rows_affected` check).
- `into_domain_row` (39-49) maps any `BotId`/`SpaceId` parse failure to `AppError::Unavailable`;
  all SQL errors go through `tenant::unavailable` → `Unavailable` (81, 103, 131).
- Schema: `bot_id` is the PK, `space_id` an FK, CHECKs on `base_url`/`secret_name`, RLS
  `USING/WITH CHECK (space_id = boma.current_space_id() OR boma.is_system())` (0054:4-25).

## Failure modes

1. **Stale agents / stale overwrite (CONFIRMED shape, GUESS interleaving).** The upsert is
   unconditional: any `connect` replaces config regardless of the row's current `updated_at`,
   and `updated_at` is the *caller's* `saved_at`, not DB `now()`. Two RPC retries (e.g. the UI's
   double-attach) arriving out of order, or a client with skewed/frozen clock, let an **older**
   config win — `updated_at` can regress. There is no version/etag, no monotonic guard, no
   `last_verified_at`: row presence is the only "connected" signal, so a dead executor or a
   rotated secret still looks connected indefinitely (status = config, not liveness).
2. **Session drift (CONFIRMED asymmetry).** The port (ports.rs 4349/4359/4366) has `connect`
   trust the payload's `space_id` while `find`/`disconnect` trust the `TenantContext`. A buggy or
   malicious caller can scope a write to a space the authenticated context never held — `in_space`
   sets the RLS variable *from the payload*, so `WITH CHECK` cannot catch the mismatch. The store
   also never checks that the FK'd `bot_id` actually belongs to `config.space_id`; only the
   use-case does (`usecases.rs:9549-9557`), leaving a TOCTOU window. A `System` context cannot
   disconnect at all (`ok_or(NotFound)`), so operator cleanup of an orphaned config is blocked.
3. **Duplicate upserts / misclassified faults (CONFIRMED).** Because the key is `bot_id`, retries
   are idempotent in *cardinality* but not in *recency* (see 1). Under concurrent upserts,
   serialization failures/deadlocks (`40001`/`40P01`) and a conflict-row invisible to RLS both
   collapse to `Unavailable` via `fetch_one` + `tenant::unavailable`: transient, retryable faults
   and permanent data/corruption faults are indistinguishable. A corrupt id becomes
   `Unavailable` (41-42), which drives pointless retries and hides bad data. `disconnect` of a
   row in another tenant returns `Ok(())`, masking drift.

## Concrete hardening (v0.4.0), with risks

- **A. Monotonic, DB-authoritative upsert.** Derive `updated_at` from DB `now()` on write and add
  `WHERE bot_external_agents.updated_at < EXCLUDED.updated_at` (or a `version` column + CAS),
  switch to `fetch_optional` and re-`SELECT` the incumbent when a stale write is a no-op, so a
  replay never regresses config. *Risk:* changes duplicate-connect semantics → DEVIATION in
  `docs/PARITY.md`; needs a migration and a decision on whether caller `saved_at` is kept as an
  audit field.
- **B. Close the tenant asymmetry.** Take `TenantContext` in `connect` (as the other two methods
  do), derive `space_id` from it, and assert `config.space_id` matches; add an SQL check that the
  bot's own space matches (guard trigger or `EXISTS` subquery). *Risk:* port signature change
  ripples to `usecases.rs` and mocks; schema guard = migration; GUESS whether any system path
  legitimately connects cross-tenant (ADR-026 says none).
- **C. Classify errors instead of blanket `Unavailable`.** Map FK/unique/check violations to
  `NotFound`/`Rule`, keep only connection/timeout as `Unavailable`, and give corrupt rows a
  distinct invariant error; add a bounded retry for `40001`/`40P01` at the transaction wrapper.
  *Risk:* new `AppError` variants touch the stable API format — keep the wire shape unchanged.
- **D. Honest deletes.** Use `rows_affected`; return `NotFound` for a genuine mismatch so silent
  drift is visible, while keeping delete idempotent for already-absent rows. *Risk:* callers that
  assumed unconditional `Ok(())` need review — GUESS which; check RPC layer first.
- **E. Freshness signal (optional).** Add `last_verified_at` written by the existing turn-time
  health check so reads can report "unverified/stale" without liveness IO in this store.
  *Risk:* extra column + writer path outside this file; do not add synchronous probes to `find`.
- **F. Observability & timeouts.** `tracing::warn!` with `bot_id`/`space_id` on error branches
  (never the secret value) and an explicit pool-acquire/statement budget at this boundary.
  *Risk:* log volume; prefer reusing existing pool settings over per-query knobs.

## Risks / non-goals

- **R1** Real executor reachability, ACP/AG-UI session lifecycle and secret rotation live in the
  provider/usecase layer, not here — flag in `docs/STATE.md`, do not patch this store.
- **R2** Keep the "secret *name* only, never a value, never logged" contract (module header) and
  all parameterized queries; no new dependency.
- **R3** A/B/C change behavior → each needs REQ-named integration tests and a `docs/PARITY.md`
  entry; no silent gate changes.

## Success criteria / tests (REQ-named, testcontainers)

- `REQ-059 stale_retry_does_not_overwrite_newer_external_agent_config`
- `REQ-059 connect_scopes_to_context_space_and_rejects_payload_mismatch`
- `REQ-059 concurrent_connects_leave_one_monotonic_config`
- `REQ-059 corrupt_external_agent_row_is_not_reported_as_unavailable`
- `REQ-059 disconnect_reports_missing_row_without_crossing_tenants`

## Bottom line

The happy path is clean and RLS-aware, but reliability rests on three fragile assumptions: a
caller-controlled timestamp makes upserts non-monotonic, tenant scope is taken from the payload
on writes and from the context on reads, and every fault is flattened to `Unavailable`. A–D are
local, testable fixes; E–F are optional hardening; the live-executor liveness question in R1 is
the only part this lens cannot close.
