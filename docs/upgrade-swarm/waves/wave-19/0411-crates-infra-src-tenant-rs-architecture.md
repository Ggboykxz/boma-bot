# Worker 0411 — architecture proposal (v0.4.0)

Module: `crates/infra/src/tenant.rs` | Lens: architecture | Status: PROPOSED (owner review)
Traceability: REQ-043, D-09, ADR-002

Legend: CONFIRMED = read from source this session. PROPOSED / GUESS = recommendation, not verified.

## The RLS tenant context today (CONFIRMED)

`tenant.rs` is the single writer of two PostgreSQL settings:

- `boma.space_id` — the active space, read by `boma.current_space_id()` in `migrations/0002_m1.sql`.
- `boma.system_reason` — non-empty only on a cross-tenant path, read by `boma.is_system()`.

Both are set with `set_config(..., true)` inside `scope()` on a freshly begun transaction, so they
revert at commit/rollback and a pooled connection never carries a tenant to its next borrower. RLS
policies compare each row's `space_id` to `boma.current_space_id()` OR `boma.is_system()`; with
`FORCE ROW LEVEL SECURITY` a query with no context sees nothing — deny by default.

The public entry points are layered:

```text
with_tenant(pool, &TenantContext, body)
  Member{space_id, user_id} -> in_space(pool, &SpaceId, body)      -> scope(space_id, "", body)
  System{reason, user_id?}  -> as_system(pool, SystemReason, body) -> scope("", reason.as_str(), body)
```

`TenantContext` and `SystemReason` live in `boma_domain`; `SystemReason` is a closed 8-variant enum
closing D-09. `TenantFuture` is a boxed, `Send`, higher-ranked future — needed to express a closure
that borrows the transaction's `PgConnection`. Driver errors map to `AppError::Unavailable`; closure
errors pass through unchanged and trigger an explicit rollback.

## Structural weaknesses (CONFIRMED by reading the module)

1. `scope()` takes two loose `&str`; the "exactly one setting is empty" invariant is upheld by
   convention in `in_space`/`as_system`, not by the type system.
2. Two `set_config` round-trips per tenant transaction, on the hottest path in the app.
3. No timeout around `body()` and no cleanup on cancellation; a hung body pins a pooled connection.
4. Setting names are duplicated across Rust constants, `0002_m1.sql` and the ADR-002 CI grep —
   nothing detects drift.
5. No structured span for the scope: system paths are auditable in code (`rg 'SystemReason::'`)
   but not in logs.

## Proposed upgrades for v0.4.0

### U1 — Typed `Scope<'a>` (make the invariant unrepresentable)

Replace `scope(pool, space_id: &str, system_reason: &str, body)` with
`scope(pool, scope: Scope<'a>, body)` where
`enum Scope<'a> { Tenant(&'a SpaceId), System(SystemReason) }`.
`in_space`/`as_system` become thin constructors; `scope` derives both settings in one place, so
"tenant with a reason" and "system with a space" cannot be constructed. All three public functions
keep their signatures — no caller changes, no DEVIATION.
Risk (GUESS, low): lifetime plumbing through the HRTB closure. Mitigation: leave the closure
signature alone; change only the two private strings.

### U2 — One round-trip for both settings

`SELECT set_config('boma.space_id', $1, true), set_config('boma.system_reason', $2, true)` replaces
the two executes, halving per-request setting overhead.
Risk (GUESS, low): a multi-value `set_config` SELECT yields two rows; `execute` ignores them, but
verify against `crates/infra/tests/tenancy.rs`. If ordering ever matters, keep two statements.

### U3 — Bounded, cancellation-safe transactions

Wrap `body(&mut transaction)` in `tokio::time::timeout`; roll back explicitly on timeout or error.
Give each scope a budget: short for member paths, longer for `ScheduledJob`/`DataMigration`.
Risk (GUESS, medium): a timeout turns a slow success into `AppError::Unavailable` — externally
visible, so log a DEVIATION in `docs/PARITY.md` and ask the owner before shipping. Confirm sqlx's
rollback-on-drop is not being relied on before removing the explicit `rollback()`.

### U4 — Setting-name contract test

Add a test that reads `migrations/0002_m1.sql` and asserts it references exactly `SPACE_ID_SETTING`
and `SYSTEM_REASON_SETTING` (plus `boma.current_space_id` / `boma.is_system`). ADR-002's grep and
the Rust constants then fail together on drift.
Risk (GUESS, low): test reads the repo at runtime; harmless in CI, skip if the file is absent.

### U5 — Tenant span for system-path auditing

Open `tracing::info_span!("tenant_scope", space = ..., reason = ...)` in `scope()`, carrying only
the space id (member) or the reason (system) — never `user_id` (personal data, REQ-054). This closes
the audit half of D-09 at runtime, not just statically.
Risk (GUESS, medium): log volume and accidental PII. Mitigation: allow-list the two fields, source
the reason only from `SystemReason::as_str()`, keep `unavailable()` detail debug-only.

## Sequencing and cross-cutting risks (GUESS)

U1 + U2 first (private, low risk, verify with `tenancy.rs`), then U4, then U5, then U3 last — it is
the only externally visible change. After each step run `cargo fmt --check`, clippy with
`-D warnings`, and the tenancy/concurrency tests. Non-goals for v0.4.0: nested scopes/savepoints and
removing the boxed future — both need an ADR (async-closure MSRV; a single-connection pool deadlocks
if a scope nests). Keep `SPACE_ID_SETTING` set via `set_config(..., true)` in all cases; a session-scoped
write from any future closure would survive the transaction and defeat the isolation guarantee.
