# 0412 — `crates/infra/src/tenant.rs` — reliability lens — v0.4.0 proposal

Worker #412 (wave 19). READ-ONLY review of `rebuild-workspace/.worktrees/before-m5` (v0.3.0-rc5);
this document is the only file written. Lens: reliability — system bypass, RLS leaks, context
propagation, error classification, pool saturation. Anchors: REQ-043, ADR-002, D-09, REQ-054.
**CONFIRMED** = read this session; likelihood/impact = **GUESS** until §5 runs. No code changed.

## 1. Current behaviour (CONFIRMED)

- `with_tenant` (:51-63) dispatches `Member` → `in_space` (:75-84: `space_id=<id>, system_reason=""`),
  `System` → `as_system` (:95-104: `space_id="", system_reason=<reason.as_str()>`); both ignore the
  variant's `user_id`.
- `scope` (:107-143) does `pool.begin()`, two sequential `SELECT set_config(..., true)` statements,
  runs `body`, then commit/rollback. `is_local=true` makes settings transaction-scoped: a pooled
  connection carries no tenant onward, and a query with no setting sees no rows
  (`tests/tenancy.rs:59-70`). No statement timeout, setting validation, or bound on `body`.
- `boma.is_system()` (`0002_m1.sql:38-42`) is `COALESCE(current_setting('boma.system_reason', true),
  '') <> ''` — it depends **only** on the reason string, never on `space_id`. Policies OR it with
  `space_id = boma.current_space_id()`; `FORCE ROW LEVEL SECURITY` is on every tenant table.
- `SystemReason` is a closed 8-variant enum (`domain/src/tenancy.rs:19-36`); `TenantContext::System`
  is public and freely constructible (:88-103); `tests/policy.rs:15-68` scans `SystemReason::` names
  against a 9-name allow-list (8 variants plus `ALL`).
- `unavailable()` (:153-156) maps **every** driver error to retryable `AppError::Unavailable` and logs
  `error = %error` at **warn** — its comment says "debug level". `is_unique_violation` (:158-164) is
  the only differentiated driver check.

## 2. Failure modes
### 2.1 System-context bypass (mechanism CONFIRMED, likelihood/impact GUESS)

- The bypass is keyed on a non-empty string, not a capability: any transaction setting
  `boma.system_reason` sees every tenant. The static gate restricts only *which names* may be
  written (`policy.rs`); all 8 allowed reasons remain usable by **any** caller holding a
  `TenantContext`. A mistaken or hostile path can pick `DataMigration` and read all spaces, with no
  runtime check, per-caller allow-list, or audit of who opened the bypass.
- ADR-002:46 says CI greps `SET LOCAL boma.space_id`, but this module uses
  `set_config('boma.space_id', $1, true)`; no grep script was found under `tools/` this session. If
  the check really matches only `SET LOCAL`, a future `set_config(..., false)` (session-level,
  sticky on a pooled connection) evades it. The "one place" claim (:11-12) also omits
  `boma.system_reason`; a stray session-level write of the reason grants permanent cross-tenant
  access. No startup assertion that the pool role lacks `BYPASSRLS`/superuser; tests use a
  low-privilege role (`tenancy.rs:4-5`), but production config is unverified here.

### 2.2 RLS leaks / isolation gaps (CONFIRMED shape)

- No DB-side invariant ties the settings. `scope` sets them in a fixed order before `body`, so they
  are coherent today; nothing stops a future helper or raw SQL from setting a non-empty `space_id`
  **and** reason in one transaction. Policies OR the two, so `is_system()` wins and every row is
  visible — convention, not enforcement.
- `body` runs arbitrary SQL. A raw `COMMIT`/`ROLLBACK` in `body` ends the outer transaction early:
  later statements see no tenant (fail closed) while earlier writes are durable — a partial-write
  hazard, not a leak (CONFIRMED shape, GUESS occurrence).
- Cancellation between `begin()` and `commit()` drops the future while `Transaction` is held; sqlx's
  drop-rollback is relied on. `is_local=true` limits the blast radius, but a shutdown before rollback
  finishes could return a connection with stale settings.
- No `statement_timeout` or `idle_in_transaction_session_timeout`: a stuck `body` holds a pooled
  connection indefinitely; under fan-out the pool exhausts and isolation becomes an outage.

### 2.3 Context propagation (CONFIRMED)

- `user_id` is dropped by this module: the DB sees only the pool role, so no DB-side actor
  attribution, per-user RLS, or audit trigger can be built here; callers must thread it explicitly.
- Tenant state is ambient in the transaction, not in Rust values. A `tokio::spawn` or queued job
  started inside `body` runs on another connection with no setting (sees nothing), or — if handed a
  `System` context by convenience — sees everything. `TenantContext` is `Clone` and freely
  constructible, so propagation is a review obligation only.
- A rejected `set_config` (a permanent config/programming fault) maps to `Unavailable`, which
  REQ-024 treats as retryable: clients hammer the DB on an error that will never heal.
- Warn-level driver `Display` can quote the connection target, contradicting the "debug level"
  comment; the HTTP response stays safe (REQ-054), but log hygiene drifts.

## 3. Concrete hardening for v0.4.0 (ordered by value/risk)

- **H1 — Combine settings into one statement (low).** `SELECT set_config('boma.space_id', $1, true),
  set_config('boma.system_reason', $2, true)` halves round trips and closes the one-setting window.
  *Risk:* none locally; keep `is_local=true`.
- **H2 — Enforce the XOR invariant (low-medium).** After H1, assert in SQL that
  `(boma.current_space_id() <> '') <> boma.is_system()` for the intended mode; fail closed with a typed
  permanent error. *Risk:* find any caller that legitimately sets both before enforcing.
- **H3 — Make ADR-002's check real (low).** Source-scan test failing if either setting name appears
  outside `tenant.rs` in `crates/**/*.rs`, independent of `SET LOCAL`; align ADR-002:46. *Risk:*
  migrations/tests embed the names — scope to production Rust, exempt the policy test.
- **H4 — Restrict System-context creation (medium, highest value).** Keep the enum closed but gate
  `TenantContext::System` behind an allow-listed factory, or extend `policy.rs` to list `as_system`
  call sites, and audit reason + callsite (no tenant data). *Risk:* many call sites, compile churn;
  ADR + owner sign-off, DEVIATION if observable.
- **H5 — Assert pool role at startup (low).** Check `rolbypassrls`/`rolsuper` (or
  `current_setting('is_superuser')`) and fail fast. *Risk:* one startup query; confirm catalogue access.
- **H6 — Bound transactions (low).** Set `statement_timeout` and `idle_in_transaction_session_timeout`
  per transaction from typed config, mapped to a distinct timed-out error. *Risk:* long migrations
  need overrides; pick values from measurements.
- **H7 — Error fidelity and log hygiene (low).** Split "refused to set tenant context" (permanent) from
  transport `Unavailable`; log detail at `debug` as documented. *Risk:* new `AppError` variant ripples
  into API mapping; additive.
- **H8 — Propagate the actor (medium).** Optionally set `boma.user_id` from `TenantContext::user_id`
  inside `scope`; document that spawned work must re-enter via `with_tenant(context, …)`. *Risk:* new
  setting + helper migration; if exposed in audit output it is a DEVIATION — ADR.

## 4. Risks / non-goals / gates
Non-goals: changing RLS policy SQL (migrations owner), per-user row policies, connection pinning;
never widen the escape hatch to free text (D-09 stays closed). `unsafe_code` forbidden; clippy
`-D warnings`; no `unwrap`/`expect`/`panic` outside tests; never move `boma.space_id` out of
`tenant.rs` (ADR-002). H4/H8 change visible behavior → `docs/STATE.md` + `docs/PARITY.md` DEVIATION
and owner approval; the rest are local/reversible. `tools/trace_check.py` counts declaration lines,
not comments: every test below carries REQ-043 (or D-09) in its name. Sequence H1 → H3 → H7 → H5
→ H6 → H2 → H4 → H8. Impact is GUESS until §5 runs.

## 5. Suggested tests (integration, real PostgreSQL)

`req_043_settings_are_transaction_scoped_across_pool_reuse` · `req_043_neither_setting_name_is_written_outside_tenant_module` ·
`req_043_mixed_space_and_reason_fails_closed` · `req_043_application_role_cannot_bypass_rls` ·
`req_043_timed_out_body_releases_a_clean_connection` · `req_043_cancelled_scope_leaves_no_setting_on_the_connection` ·
`req_043_spawned_task_without_context_sees_no_rows` · `d_09_system_call_sites_are_allow_listed_and_audited`

## Bottom line

The core mechanism is sound — transaction-scoped settings, closed reason enum, fail-closed RLS — but
the bypass is a string with no runtime capability check, the setting pair has no enforced invariant,
and nothing bounds a transaction or propagates the actor. H1/H3/H5/H6/H7 are cheap, reversible wins;
H2/H4/H8 harden deeper paths but change behavior and need ADRs plus sign-off. Impact is GUESS.
