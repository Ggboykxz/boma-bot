# Reliability proposal — `crates/infra/src/stores/plugin_installs.rs` (v0.4.0)

- Worker #374 · Lens: reliability · Module: `PostgresPluginInstallStore` (126 lines, `before-m5`, v0.3.0-rc5).
- Status: proposals only (CONFIRMED = read in this session; GUESS = inferred). No code changed.
- Other modules consulted (CONFIRMED): `crates/app/src/ports.rs:2727–2796`, `usecases.rs:1328–1447`
  (`marketplace_install`, `record_install`, `teach_skill`, `mcp_server_add`), `tenant.rs:41–163`,
  `migrations/0035_plugin_installs.sql`, `migrations/0028_skills.sql`, `error.rs:10–66`,
  `crates/infra/tests/m1.rs:2155–2243`, wave-17 sibling proposals 0373 (architecture) and 0376.

## Current behaviour (CONFIRMED)

All three methods wrap one `tenant::with_tenant` transaction (RLS via `boma.space_id`). `list`
orders `created_at DESC, id ASC`; `record` is `INSERT … ON CONFLICT (space_id, source) DO UPDATE`
refreshing `resolved_sha`, `name`, `parts`, `skipped`, `created_at`, `updated_at` but never `id`;
`remove_plugin_skills` is `DELETE FROM skills WHERE origin='plugin' AND plugin_source=$1` with
`rows_affected()` saturated via `unwrap_or(u32::MAX)`. The store owns no orchestration:
`marketplace_install` calls revoke → loop `teach_skill` → loop `mcp_server_add` → `record_install`,
**each in its own transaction**. There is no uninstall/delete port method.

## Failure modes

- **F1 — Partial install is the normal failure shape (HIGH, CONFIRMED).** The four-plus independent
  commits at `usecases.rs:1343–1437` are not atomic. A DB error, a later skill's validation failure
  (`teach_skill` :531–533), an MCP conflict (:578), an id/clock failure, or client cancellation
  after the `DELETE` leaves: old skills revoked, only a prefix of the fresh skills written, and an
  old-or-absent ledger row. `marketplace.list` then advertises old `parts`/`resolved_sha` while the
  skills it describes are gone — stale, internally contradictory state. Retry re-deletes and
  converges, but there is no atomicity or compensation in between.
- **F2 — Concurrent installs of one source can interleave (MEDIUM, CONFIRMED).** Two requests for
  the same `(space, source)` each run their own DELETE/insert/upsert transactions; A's skills and
  B's skills can mix, and the last `record` wins with `parts` from a different plan. No lock
  serialises the sequence.
- **F3 — Id and timestamp drift make state stale and outcome non-replayable (MEDIUM, CONFIRMED).**
  `record_install` mints a fresh `pin_*` id (:1427) and returns it in `PluginInstallOutcome`, but the
  upsert never persists `id`, so `list` shows the original id: the returned id cannot be found.
  `created_at` is reset to `now()` on every reinstall, so it means "last replaced", not "installed".
- **F4 — No idempotency key (MEDIUM, CONFIRMED).** `NewPluginInstall.id` is generated per call; the
  caller supplies no key and the store cannot distinguish a replay from a new request. A retry
  after a post-commit timeout refetches, rewrites skills, and returns a new outcome id with fresh
  `skipped` error strings (which embed the error text at :1386). Last-write-wins upsert is
  deterministic on data but not on identity or reporting.
- **F5 — Deterministic faults reported as outage (MEDIUM, CONFIRMED).** `tenant::unavailable`
  flattens *every* `sqlx::Error` — PK/unique violation, the `$8::timestamptz` cast of the untrusted
  `created_at` string (:94), bad jsonb, pool timeout, lost connection — into `Unavailable` (503).
  Bad input looks transient, so clients retry, worsening F2/F4; real data bugs stay invisible.
  `is_unique_violation` exists (`tenant.rs:159`) but is unused here.
- **F6 — No removal path / silent count (LOW, CONFIRMED).** `remove_plugin_skills` deletes skills
  only; the ledger row is never deleted, so revoked plugins linger in the catalogue forever.
  `unwrap_or(u32::MAX)` (:124) silently masks an impossible-but-real overflow count.
- **F7 — No store-level observability or bound (LOW, CONFIRMED).** No `tracing` in this store
  (only the generic warn at `tenant.rs:154`) and no statement timeout of its own; failures cannot be
  attributed to install vs. revoke vs. ledger from logs.

## Concrete hardening (v0.4.0)

- **H1 — One transaction per logical install (addresses F1, F2).** Expose a store-orchestrated
  `replace(install, skills, mcp_servers)` (or thread one borrowed `&mut PgConnection` through private
  helpers) so revoke → rewrite → upsert commits together; take `pg_advisory_xact_lock` on
  `hashtext(space_id||source)` to serialise concurrent installs of one source. *Risk:* pulls
  `SkillStore`/`McpServerStore` writes into this module and crosses other workers' modules; a naive
  rewrite holds a borrow across `.await` (forbidden). *Mitigation:* keep `usecases` as orchestrator
  with a single connection handle; add a test injecting a mid-install failure that asserts rollback
  to the previous state. If too invasive, document the window in `docs/STATE.md` rather than ship it.
- **H2 — Fix identity/timestamp semantics (addresses F3).** Pin current behavior in a test, then
  keep a stable `id` on upsert and rename `created_at` to `replaced_at` via a versioned migration.
  *Risk:* externally visible through `PluginInstallSummary`/ORDER BY — a DEVIATION in
  `docs/PARITY.md` needing approval; changing order affects catalogue view.
- **H3 — Caller idempotency key (addresses F4).** Thread a key from the RPC into
  `NewPluginInstall` and upsert on `(space_id, source, key)`; on replay, skip the rewrite and return
  the stored outcome id. *Risk:* touches port, wire, testkit memory store and fetch flow —
  DEVIATION-gated; `skipped` error text must be stabilised for a replay to compare equal.
- **H4 — Classify errors (addresses F5).** Add a focused `tenant::map_write_error`: unique/CHECK/cast
  → `Conflict`/`Rule`, connection/timeout → `Unavailable`; keep driver detail at `debug` (REQ-054).
  *Risk:* a wrong branch turns a real outage into a client fault; verify with `is_client_fault()`.
- **H5 — Removal and honest count (addresses F6).** Add a `remove` port method deleting the
  `plugin_installs` row alongside skills, and return a checked count (0 on overflow) with a test
  pinning today's value before any change. *Risk:* new method in both adapters + `usecases`; a wrong
  predicate could delete another tenant's row (RLS is the backstop).
- **H6 — Observability + bound (addresses F7).** One `tracing::instrument` span per method with
  outcome (`inserted`/`replaced`/`revoked`/`failed`) and canonical source + SHA only — never the raw
  URL (credential-bearing refs); document the configured statement timeout. *Risk:* id cardinality —
  stay at `debug`, never a metric label.

## Cross-cutting risks / sequencing

- Sequence H4 → H6 → H2 → H5 → H1 → H3: signature-preserving local work first, DEVIATION-gated
  changes next, the cross-module transaction redesign last and only with 0373's agreement.
- Freeze public port signatures, JSON field names, error text and ORDER BY while doing H4/H6.
  `domain/src/plugin.rs`, `usecases.rs` and the skill/MCP stores are other modules — coordinate.
- Gate with `cargo fmt --check`, `clippy -D warnings`, the REQ-034 Postgres round-trip and RPC tests,
  goldens, and `tools/trace_check.py`. No new dependency needed.

## Tests to add (REQ-034 in the name)

- `req_034_mid_install_failure_rolls_back_to_previous_skills`; `req_034_concurrent_installs_of_one_source_do_not_mix`
  (H1); `req_034_retry_with_same_key_returns_stored_id` (H3);
  `req_034_bad_timestamp_is_client_fault` (H4); `req_034_remove_deletes_ledger_row` (H5).
- Keep `req_034_a_plugin_install_round_trips_through_postgres` green.

**Recommendation:** ship H4 + H6 first (low risk, no visible change); treat H1/H2/H3/H5 as
DEVIATION-gated for v0.4.0 approval, with H1 the single highest-value reliability fix.
