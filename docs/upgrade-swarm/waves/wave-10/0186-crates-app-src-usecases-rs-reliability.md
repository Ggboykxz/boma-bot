# v0.4.0 reliability proposal — `crates/app/src/usecases.rs` (module 0186, lens: reliability)

Scope: the application use-case layer (REQ-001..060). Status tags per AGENTS.md: **CONFIRMED** =
verified by reading this file and the named store implementations this session; **GUESS** = needs a
command before shipping. READ-ONLY pass: no product code changed. Line numbers are against this file
unless stated. Goal: bound partial writes and make external retries safe without changing the wire.

## 1. Failure modes

### FM-1 — Multi-row writes are not one transaction (partial state on any `?`)
- **CONFIRMED** `send_message` (4282-4372) inserts the message (4336) then creates/steers runs one
  per target in `create_or_steer_runs` (4402-4453). Each `messages.insert` / `runs.create` is its own
  tenant transaction (infra `messages.rs`, `runs.rs`): a failure on target N leaves the message and
  runs for targets 1..N-1 committed. Group fan-out (`mentioned_bots` 4462-4484, N `bots.find`)
  widens the window. Unlike `create_bot`/`create_space`, no store-level atomic wrapper exists here.
- **CONFIRMED** `mail_held_release` (2495-2567) calls `take_held` (2500) first — the held row is
  deleted — then inserts a message (2524) and creates a run (2546). A failure in the last two loses
  the held mail entirely (no re-queue). `mail_inbound` (2135-2382) and `webhook_wake` (1965-2103)
  likewise interleave message/run/routine writes with no rollback.
- **CONFIRMED** `marketplace_install` (1328-1413) is destructive-first: `remove_plugin_skills`
  (1344) deletes all skills for a source before the `teach_skill` loop (1350-1365) and MCP loop
  (1368-1400) write the new set. A mid-loop error leaves old skills gone and a partial new set; the
  install row is still recorded via the `record_install` error path (1386-1397).
- **CONFIRMED** `record_csp_violations` (3447-3475) appends audit rows in a loop with `?`, then
  increments a metric per row: a failure at row k leaves k-1 rows and k-1 counters — a partial,
  un-retriable batch.
- **CONFIRMED** `dispatch_routines` (5271-5339) does `create` + `record_fired` per due routine; one
  error aborts the whole batch after earlier routines fired. `due`/`next` derive from one `now`
  (5272), so a slow batch reschedules from a stale instant.

### FM-2 — Idempotency is uneven
- **CONFIRMED** `send_message` retries are safe at the store layer: `messages.insert` and
  `runs.create` catch unique-nonce violations and re-read the existing row (infra messages.rs
  110-131, runs.rs 102-124). But the *combination* is not atomic: a retry recovering a message can
  still steer one bot and create another run depending on which partial writes landed.
- **CONFIRMED** The public ingress paths carry no idempotency key: `webhook_wake`, `mail_inbound`,
  `messaging_inbound` (3073-3232) all set `client_nonce: None` on message and run. These are
  at-least-once external deliveries (Slack/GitHub/Telegram retry on timeout); a redelivery produces
  duplicate user messages and runs. The reply path is explicitly at-most-once ("delivering twice is
  worse than never", 3238-3240) — the inbound side has no equivalent guard.
- **CONFIRMED** `pages_save` (1493-1502) and `artifact_create` (1651-1661) do their nonce replay
  check in the app layer (`find_by_nonce`) before a separate write — a check-then-write TOCTOU;
  safety depends entirely on a DB unique constraint (**GUESS**: present in infra pages/artifacts
  stores — verify before relying on it).
- **CONFIRMED** `pages_share` (1555-1567) mints a fresh token on every call, silently invalidating
  the previous link. Retrying a share is not idempotent and there is no audit event on share/read.
- **CONFIRMED** Single-use states are burned before the irreversible call: `mcp_oauth_complete`
  `take_oauth_session` (766-773) and `messaging_slack_callback` `take_install_state` (2692-2696)
  consume state before the POST; a transport failure forces the member to restart the handshake.

### FM-3 — Cross-store / external side effects cannot be rolled back
- **CONFIRMED** `artifact_create` writes blob bytes (1663) before the DB row (1680): an insert
  failure leaves an orphan blob; a nonce replay after the blob write still pays the write.
  `artifact_remove` (1780-1802) deletes the family then best-effort blobs (`let _ = ...remove`,
  1798) — leaked blobs on error, and keys are snapshotted before the delete (a concurrent create
  can add a referenced blob that then gets removed).
- **CONFIRMED** `ensure_computer` (5513-5600): row create → `runtime.ensure` (5583) → row update
  (5595). A failed update leaves a running external computer with stale DB state; a failed ensure
  leaves an orphan row. `computer_navigate/click/type/scroll` call the runtime before
  `computer_status` re-reads the row (5673-5674, 5729-5730, 5756-5757, 5783-5784): an external
  action can succeed while the persisted status fails.
- **CONFIRMED** `mcp_oauth_complete` stores the bearer (816-823) then probes (825); a failed probe
  sets auth state "error" despite a valid token. `mcp_server_add` (638-648) writes the server then
  audits a private URL — a failed audit leaves the server registered without its audit row.

## 2. Concrete hardening (v0.4.0)
1. Add one store-level atomic primitive per composite write and call it from the use case:
   `insert_message_with_runs`, `held_release→message+run`, `install_plugin(source)` (delete +
   skills + MCP in one tenant transaction). Preserve per-store nonce dedup inside the transaction.
2. Give every public ingress a stable dedup key: derive `(platform, installation_id, envelope_id)`
   for messaging, `(bot_id, delivery_id)` for webhook/mail; store it in a `processed_deliveries`
   table with a unique constraint and skip message+run creation on replay, returning the prior
   outcome. No wire change if the key derives from existing headers/body.
3. Move nonce checks into the store: make `pages.put_version` / `artifacts.put` atomic
   insert-or-return (as messages/runs already do) and drop the app-level pre-check. For artifacts,
   write the blob after a successful row insert (or into a store-committed staging key) so a failed
   insert leaves no orphan.
4. Make artifact/computer cleanup reconcile, not best-effort: after `remove_family`, recheck
   `storage_key_refs` before each blob delete; on failure record a pending-reclamation row. Wrap
   `ensure_computer` in compensating actions (stop/delete the external computer if the row update
   fails; delete the row if `ensure` fails).
5. Make single-use states survive transport failure: mark state "in-flight" with a short lease and
   consume it only after the exchange commits; allow a bounded retry within the lease while keeping
   the single-use guarantee after success.
6. Bound batch fan-outs: iterate `dispatch_routines` / `record_csp_violations` with per-item error
   isolation and an aggregated result; persist `next_run_at` before firing so a mid-batch failure
   cannot lose a schedule. Keep the single captured `now` and document it.
7. Separate "data committed" from "notification failed": in `record_routine_failed` and the OAuth
   paths, do not turn a failed notice/probe into a top-level error after the state write succeeded;
   log and emit a metric so a worker retry does not double-count or re-seal.

## 3. Risks / trade-offs
- R1 — One transaction around fan-out lengthens lock hold time and serializes group sends; benchmark
  with N bots and cap targets. Dropping partial-steering behavior may be a DEVIATION (docs/PARITY).
- R2 — Delivery dedup adds a table + migration and needs a stable external id; where platforms give
  none, a body hash is a **GUESS** (a legitimate retry of an identical message would be dropped).
- R3 — Compensating external actions (stop computer, revoke token) are not truly atomic; they need
  idempotent provider APIs and must never be assumed to have succeeded.
- R4 — Delaying blob writes risks a window where a row references a not-yet-present blob; reads must
  tolerate it (or the store stages within one transaction) — validate against artifact goldens.
- R5 — In-flight OAuth leases add a crash-recovery path and extend state lifetime; keep them short
  and re-verify the state on completion.

## 4. Acceptance tests (REQ id in name)
- `req_002_send_message_fanout_is_all_or_nothing`
- `req_002_send_message_nonce_replay_recovers_partial_run`
- `req_050_webhook_redelivery_creates_one_run`
- `req_047_mail_redelivery_creates_one_turn`
- `req_048_messaging_redelivery_is_idempotent`
- `req_036_artifact_insert_failure_leaves_no_orphan_blob`
- `req_037_page_save_nonce_is_atomic_under_concurrency`
- `req_047_held_release_failure_keeps_the_held_mail`
- `req_034_plugin_install_failure_keeps_old_skills`
- `req_026_computer_ensure_failure_has_no_orphan`
