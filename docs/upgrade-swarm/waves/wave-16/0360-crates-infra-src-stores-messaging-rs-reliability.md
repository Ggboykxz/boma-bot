# v0.4.0 reliability proposal — `crates/infra/src/stores/messaging.rs`

- Worker: 360 (upgrade swarm, wave 16). Lens: reliability. Target: v0.4.0.
- Scope: read-only review; this document is the only file written.
- Markers: **CONFIRMED** = read from source in this session; **GUESS** = inference to verify.
- Sources: this file (621 lines), `crates/infra/src/tenant.rs`, `migrations/0042_messaging.sql`, `crates/app/src/{error.rs,ports.rs,usecases.rs}`.

## 1. What is already sound (CONFIRMED)

`tenant::with_tenant` opens a real transaction and commits/rolls back the closure, so `create`'s
INSERT + conflict re-SELECT and `resolve_external_thread`'s SELECT → INSERT → SELECT are each atomic
on one connection (the architecture proposal's "no explicit transaction" claim is wrong here).
`threads_external_identity` is a partial unique index on `(external_platform, external_workspace_id,
external_conversation_id) WHERE kind='external'`, so concurrent first messages dedupe via
`ON CONFLICT DO NOTHING` + re-SELECT. `take_install_state` is an atomic `DELETE … RETURNING`, and
`unwrap_or_fresh_in` handles the space-DEK race with `ON CONFLICT DO NOTHING` + survivor read.
Reuse these patterns.

## 2. Failure modes

### 2.1 Loss / phantom failure
- **CONFIRMED** `create` (l. 228-289) commits the INSERT in `with_tenant`, then calls `find` in a
  **second** transaction. A transient read failure returns `Unavailable` (l. 289) though the install
  committed; retry re-hits the unique slot and returns `Conflict`. An existing install is reported
  failed, then as a duplicate — state drift with no port-level recovery.
- **CONFIRMED** `mark_removed` only flips `status`; `messaging_installations_platform_workspace`
  covers *all* rows, there is no delete port, and `create`'s conflict re-SELECT has no status filter.
  A removed workspace can never be reinstalled; `find_active_by_platform_workspace` returns `None`
  forever → permanent inbound loss. A fresh OAuth install's new credentials are silently discarded
  by `ON CONFLICT DO NOTHING`; the old, possibly revoked token stays live, so replies fail later.
- **CONFIRMED** `resolve_external_thread` checks `find_active` in its own transaction (l. 402-405),
  creates the thread in another, re-reads in a third (l. 472). Removal or bot archival between
  steps yields an orphan thread or a dropped message.
- **CONFIRMED** `reveal` reads credential and space DEK in separate transactions (l. 142-158): no
  single snapshot, and `status` is ignored, so a removed installation's secret can still be revealed.
- **CONFIRMED** `list` (l. 346) and `list_external_threads` (l. 495) hardcode `LIMIT 50` with no
  cursor or `has_more`: rows beyond 50 are invisible and truncation is undetectable.

### 2.2 Delivery order
- **CONFIRMED** Both lists use `ORDER BY created_at DESC` only; `created_at` comes from `now()` or
  callers, so ties are common and PostgreSQL gives no stable tie order — the same list can reorder
  between requests. **GUESS**: real send ordering lives in the worker/runs layer; do not add
  sequencing here without a port-level idempotency key.

### 2.3 Deduplication
- **CONFIRMED** Thread dedup is sound *per workspace*, but the key excludes `installation_id`
  (`let _ = installation_id;`, l. 471). If reinstall is enabled (§2.1), a new installation inherits
  old conversations — desired for history, a cross-owner leak if ownership changed: needs a decision.
- **CONFIRMED** `store_install_state` has no `ON CONFLICT` handling: a repeated `state` raises a
  unique violation mapped to `Unavailable` (which `is_retryable()` treats as retryable), so the
  caller retries the same poisoned state forever instead of getting `Conflict`.
- **CONFIRMED** `take_install_state` filters `expires_at > now()` but never deletes expired rows;
  `messaging_install_states` grows without bound and only becomes slower over time.

### 2.4 State drift / error classification
- **CONFIRMED** Every `sqlx::Error` maps through `tenant::unavailable` → `Unavailable` (retryable).
  Permanent failures (unique, FK on a stale bot/space/user id, check) are indistinguishable from a
  transient outage: retries are wasted and real bugs hidden behind "dependency unavailable".
- **CONFIRMED** `create` uses the caller's `created_at` for both timestamp columns while
  `mark_removed` uses `now()`; mixed clocks can move `updated_at` backwards, and `list` does not
  order by it, so the drift is invisible.

## 3. Concrete hardening (v0.4.0)

H1. Return the created installation from the INSERT path (build `MessagingInstallation` from the
    input plus sealed rows already in hand) or re-read with a bounded retry *inside* the same
    transaction. Delete the post-commit `find`. *Risk*: constructing the view must match `find`
    exactly (timestamps, `parse_platform`); add a test comparing them to prevent drift.

H2. Give removal a replacement path: either a `create` branch that reactivates a `removed` row and
    rotates token/webhook/reply_url/`updated_at` in one `UPDATE`, or make the unique index partial
    (`WHERE status='active'`) via a new migration and keep historical rows. *Risk*: both change
    observable behavior — reactivation needs `docs/PARITY.md`, the partial index needs a migration
    plus a reindex plan and a concurrent-install test. Do not ship without owner approval.

H3. Fold `find_active` + thread identity lookup + insert + final read into **one** `with_tenant`
    closure; bind `installation_id` into the thread's stored identity (new column + index) or at
    least re-check installation status at the end. *Risk*: new schema and a DEVIATION if thread
    identity semantics change; the cheap version (single transaction, status re-check) is safe now.

H4. Map DB errors by kind with a local helper using `tenant::is_unique_violation`: unique →
    `Conflict`, FK/check → a non-retryable error, driver/timeout → `Unavailable`; add
    `tracing::warn!` spans with `installation_id`/platform/state-hash only (never secrets).
    *Risk*: new client-visible error codes — audit the API envelope and record any change.

H5. Make install-state writes collision-safe (`INSERT … ON CONFLICT (state) DO NOTHING` plus
    `rows_affected` → `Conflict`, regenerate upstream) and opportunistically delete expired rows for
    the same space on `store_install_state` (`DELETE … WHERE expires_at < now() … LIMIT n`), with a
    periodic cleanup job as the real fix. *Risk*: extra write on the install path; keep the batched
    delete off the hot read path.

H6. Add a deterministic tiebreaker `ORDER BY created_at DESC, id DESC` to both lists now; stage a
    typed cursor/page parameter as a separate, PARITY-approved change. *Risk*: among equal
    timestamps the visible order changes — document it; a port signature change is a DEVIATION.

H7. Make `reveal` a single transaction (credential + DEK together) and refuse to reveal for
    `status='removed'` unless a named caller needs it. *Risk*: callers relying on post-removal
    reveal (e.g. cached tokens) must be audited first.

H8. Use one clock source for `created_at`/`updated_at` (`now()` in SQL, or validated RFC 3339 from
    the port with a single `updated_at = created_at` insert). *Risk*: minor visible timestamp change.

## 4. Test plan (REQ-tagged, names carry the id)

- `req_048_create_is_atomic_on_read_failure`: a failing follow-up read cannot report a committed install as `Unavailable`.
- `req_048_removed_workspace_can_be_reinstalled`: second install after `mark_removed` succeeds with rotated credential.
- `req_048_first_message_race_creates_one_thread`: N concurrent resolves for one conversation yield one `Thread`.
- `req_048_install_state_is_single_use`: two takes → one `Some`, one `None`.
- `req_048_state_collision_is_conflict_not_retryable`: duplicate state → `Conflict`, not retryable.
- `req_048_list_order_is_stable_on_ties`: equal `created_at` rows keep `id DESC` order across reads.
- `req_048_expired_states_are_pruned`: expired rows are gone within a bounded count after cleanup.

## 5. Deviations / questions to owner

- H2, H3 (schema/identity) and H6 (pagination) change observable behavior: record in `docs/PARITY.md`
  and wait for approval before shipping.
- Decide whether removed installations may be reinstalled and whether that inherits old threads;
  the current code makes both impossible/ambiguous by accident.
- Confirm which caller, if any, legitimately reveals a removed installation's credential (H7).
