# Worker #322 — `crates/infra/src/stores/acp_agents.rs` — LENS: reliability — v0.4.0 proposal

- **Status:** PROPOSAL (read-only review; no source changed). CONFIRMED = verified by reading files in this worktree; GUESS = needs a run against the real Postgres/oracle to settle.
- **Scope:** the Postgres adapter for `ports::AcpAgentStore` (`REQ-061`): upsert / delete / tenant-scoped read of one attachment row per bot.
- **Consulted (CONFIRMED):** this store; `infra/src/tenant.rs`; `migrations/0059_acp_agents.sql` and `0002_m1.sql` (bots); `domain/src/acp.rs`; `app/src/ports.rs:4373-4401`; `app/src/usecases.rs:9603-9666`; `app/src/worker.rs:596-621`; `testkit/src/build.rs:10900-10944`; `spec/01-requirements.md:620-634`.

## Summary

The three-method store is small and the RLS + transaction-local `set_config` plumbing in `tenant.rs` is sound, but the write path is an unconditional blind upsert that trusts a caller-supplied `space_id`, and the read path returns a row without any liveness check. The result is three reliability holes the current happy-path test does not touch: silent last-writer-wins on concurrent/duplicate attaches, unbounded staleness (the row outlives the CLI it names), and a store API that cannot enforce the tenant boundary it documents. All three are addressable without leaving REQ-061's observable contract, except where noted as DEVIATION candidates.

## Failure modes

### Stale agents (S)

- **S1 (HIGH, CONFIRMED):** `find` (108-120) returns whatever `executor` string the row holds; no check against the compiled-in registry. The worker resolves it at `worker.rs:602-616` and, on an unknown id (CLI uninstalled, registry renamed, hand-edited row), logs a warn and silently falls back to the internal model. REQ-061 requires a *legible failure* when the executor is missing; silent degradation hides the misconfiguration from the operator and from the user.
- **S2 (MED, CONFIRMED):** no liveness metadata and no TTL. Once attached, the row lives until detach or bot deletion (`ON DELETE CASCADE`). A CLI removed from `PATH` leaves a permanently stale attachment that every run re-reads and re-warns on; there is no `last_seen_at`/`checked_at` to age it out or report it.
- **S3 (MED, CONFIRMED):** `saved_at` is caller-supplied (`NewAcpAgent.saved_at`, domain/acp.rs:35) and written verbatim via `$4::timestamptz` as both timestamps (lines 55, 66). A retried request carrying an older clock value moves `updated_at` *backwards*, so consumers ordering by it see stale state. A malformed timestamp string surfaces through `tenant::unavailable` as `Unavailable`, not as a validation error.

### Duplicate upserts (U)

- **U1 (HIGH, CONFIRMED):** `ON CONFLICT (bot_id) DO UPDATE SET executor = EXCLUDED.executor, updated_at = EXCLUDED.updated_at` (56-58) is blind last-writer-wins. Two concurrent attaches with different executors both return success and each `RETURNING` shows *its own* value, but only one row persists — operators get contradictory confirmations. No version column, no expected-value compare-and-swap. Retries are also unconditionally re-logged as "the bot now runs…" at `usecases.rs:9645`, so duplicate requests inflate telemetry.
- **U2 (HIGH, GUESS — verify on the pinned Postgres):** the conflict target is `(bot_id)` but tenancy lives on `space_id`. Conflict detection runs on the unique index, outside RLS; an upsert in a transaction scoped to space B for a bot owned by space A can hit A's hidden row. Because the `SET` clause does not rewrite `space_id`, the resulting row keeps A's space and should fail B's `WITH CHECK` as `Unavailable` (fail-safe, confusing error). If a future edit adds `space_id = EXCLUDED.space_id`, it becomes a genuine cross-tenant hijack. This RLS/`ON CONFLICT` interaction must be pinned by a test before any change to the `SET` list.
- **U3 (MED, CONFIRMED):** the FK only guarantees the bot *exists*, not that `bots.space_id = config.space_id`. `attach` takes `space_id` from `NewAcpAgent`, so any direct caller of the public port can write a row into space B referencing a bot in space A; `find(A, bot)` filters by `(bot_id, A)` and never sees it, so the bot looks unattached in its own space. The normal use case prevents this via `bots.find(context, …)` (usecases.rs:9624-9632), but the store has no second line of defence.
- **U4 (LOW, CONFIRMED):** `detach` (86-91) discards `rows_affected` and always returns `Ok(())`. A no-op detach is indistinguishable from a real one; harmless for idempotency but removes any signal for "detached" telemetry.

### Tenant leakage (T)

- **T1 (HIGH, CONFIRMED):** `attach` (49) has **no `TenantContext`** and scopes the transaction to the config-supplied `space_id` (51). Authorization is delegated entirely to the use case; the store itself cannot check that the caller acts in that space. `detach`/`find` do take a context, so the port is inconsistent and any future call site can bypass the only in-app check. RLS cannot help because the injected space equals the caller's own.
- **T2 (MED, CONFIRMED):** `detach`/`find` use `context.space_id().cloned().ok_or(AppError::NotFound)?` (82, 104): a **System** context yields `None` → `NotFound`, so maintenance paths cannot inspect or purge attachments (relevant to S2). `with_tenant` already supports a closed-enum `SystemReason`; this store opts out (cf. defect D-09).
- **T3 (MED, CONFIRMED):** `into_domain_row` (37-45) maps a malformed stored `bot_id`/`space_id` to `AppError::Unavailable` — data corruption is reported as a transient DB error, inviting retries instead of quarantine, and nothing logs the offending id.

## Concrete hardening (prioritized)

- **P0 — make `attach` tenant-safe (T1/U3):** add `context: &TenantContext` to the port signature, derive `space_id` from it, and verify the bot belongs to that space inside the same transaction (`EXISTS (SELECT 1 FROM bots WHERE id=$1 AND space_id=$2)` folded into the upsert), so space and bot can never disagree. Stronger structural option: add `UNIQUE(id, space_id)` on `bots` and a composite FK `(bot_id, space_id)` in a new migration, making the invariant unrepresentable to violate. **DEVIATION candidate** (port/API change) — log in `docs/PARITY.md` and await owner approval.
- **P0 — kill last-writer-wins (U1/S3):** add `version bigint NOT NULL DEFAULT 1` (or compare on `executor`) and require `WHERE version = $expected`, returning a typed conflict when zero rows; add `AND EXCLUDED.updated_at >= updated_at` (or `GREATEST`) so a stale retry cannot turn the clock back. Keep same-payload retries idempotent. Take `saved_at` from the existing server clock (`usecases.rs:9634`) rather than any client value.
- **P1 — stale detection at read (S1/S2):** move the registry check to the point of use: when `attachment.executor` is unknown at `worker.rs:602-616`, fail the run with a legible, auditable reason instead of falling back (REQ-061). Optionally add `checked_at`, stamped when the executor is successfully probed, and let `find` hide/flag rows older than a configured TTL. **DEVIATION** if run outcome changes — parity sign-off required.
- **P1 — close the RLS/upsert hole (U2):** pin current behaviour with a two-space test first; then keep the conflict target explicitly `(bot_id, space_id)` *and* add the composite FK so a cross-space conflict cannot reach an update path. Never set `space_id` from `EXCLUDED`.
- **P1 — error mapping & observability (T2/T3/U4):** give corruption a typed integrity error and log ids at debug (never to the client); route system maintenance through `with_tenant` with a `SystemReason` instead of `NotFound`; log `rows_affected` in `detach` and surface conflict/last-writer-wins events with both executors for diagnosis.
- **P2 — tests (REQ-061, name carries the id):** integration tests against real Postgres for concurrent attach (one winner, loser typed error), cross-space attach rejected, stale timestamp does not regress `updated_at`, unknown-executor run fails legibly, system-context `find`/`detach`, and the U2 RLS interaction. These also raise coverage of the currently untested failure modes.

## Risks / trade-offs

- Adding `TenantContext` to `attach` changes the public port and every call site (app use case, testkit, future code): coordinated edit + parity approval; the alternative (keep the signature, verify inside via a trusted context argument) only moves the trust problem.
- A version/CAS column adds a conflict error path callers must handle; a too-strict `>= updated_at` guard can reject legitimate same-instant retries — use a monotonic server clock and define idempotency explicitly.
- The composite FK needs a migration and a `UNIQUE(id, space_id)` prerequisite; existing orphan rows (if U3 ever occurred in production) must be cleaned before it can be added.
- Failing instead of falling back on an unknown executor is a visible behaviour change; it is what REQ-061 asks for, but needs owner sign-off and a legible error surface.
- A `checked_at` TTL introduces clock-skew and "what counts as seen" questions; default the feature off to avoid an unplanned DEVIATION, and keep no personal data in the new logs.
- All SQL/API changes need `cargo fmt --check`, clippy `-D warnings`, and the integration suite run before any claim of success.
