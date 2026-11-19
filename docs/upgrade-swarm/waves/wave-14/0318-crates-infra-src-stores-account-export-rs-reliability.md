# v0.4.0 reliability proposal — `crates/infra/src/stores/account_export.rs` (worker #318, lens: reliability)

## Current design (CONFIRMED — read in this session)

`PostgresAccountExportStore` implements four methods of `AccountExportStore`, all wrapped in
`tenant::as_system(SystemReason::AccountExport)`:

- `bundle(user_id)` issues **seven sequential SELECTs** (profile, bots, threads, messages,
  bot_memory, routines, artifacts) and assembles `ExportBundle`. Each query is scoped to the owner
  (`user_id = $1`, or `bot_id = ANY($1)` / `thread_id = ANY($1)` derived from the owner's rows),
  with explicit `Vec::new()` short-circuits when there are no bot/thread ids.
- `create(id, user_id, created_at)` is one `INSERT ... status='pending'`.
- `find(id, user_id)` is one `SELECT ... WHERE id=$1 AND user_id=$2` (ownership enforced here).
- `mark_ready(id, payload_bytes, skipped, now)` is one `UPDATE ... SET status='ready', ... WHERE id=$1`.

`as_system` opens one transaction, sets `boma.space_id`/`boma.system_reason` locally, commits on `Ok`,
rolls back on `Err` (CONFIRMED in `tenant.rs`). RLS on `account_exports` allows only `boma.is_system()`.
The schema (`0046_account_exports.sql`) has `status IN ('pending','ready','failed')`, `payload_bytes`,
`skipped`, `created_at`, `updated_at` — **no `expires_at`, no TTL, no error field** (CONFIRMED).
The archive zip is not stored: it is re-rendered on demand by `app/src/account_export.rs`
(`status`/`download`), which then calls `mark_ready` (CONFIRMED). `rows::user()` deliberately omits
`password_hash` from the domain `User`, so the bundle carries no password hash (CONFIRMED), although
the raw `UserRow` still holds it in memory and the SELECT still fetches it (CONFIRMED).

## Failure modes

### 1. Torn / inconsistent snapshot (CONFIRMED; severity: high)

The seven `bundle` SELECTs run at default `READ COMMITTED`, so each statement sees a fresh snapshot.
Concurrent writes between statements yield an impossible archive: a thread whose messages were
deleted, a bot removed after the `bots` query but whose memory revisions are still read, or a routine
referencing a thread not in the bundle. The `#[allow(clippy::too_many_lines)]` comment ("the bundle
is one SQL transaction; splitting hides the contract") asserts atomicity that READ COMMITTED does
not provide. A read-only transaction is still a *sequence* of point-in-time reads, not one point.

### 2. Partial writes and silent status divergence (CONFIRMED; severity: high)

- `mark_ready` filters on `id` only: no `user_id`, no state guard, and `rows_affected` is discarded.
  A stale id, a re-used id, or an id that no longer exists returns `Ok(())` — silent success. It can
  overwrite a `ready` or `failed` row, and each re-download rewrites `payload_bytes`/`skipped`, so the
  stored audit metadata can drift from any particular delivered archive.
- No code path ever writes `failed` despite the CHECK allowing it (CONFIRMED in this module; GUESS:
  no other path does — repository-wide search found none). A transient artifact read error
  (`load_artifact_bytes` returns `Err` for non-`NotFound`) leaves the row `pending` forever, with no
  retry, no terminal state and no recorded reason.
- The bytes are returned to the client and `mark_ready` is a separate statement. If rendering succeeds
  but `mark_ready` fails, the client has a zip while the row says `pending`; if the response then
  fails, the row says `ready` while the client has nothing. There is no transaction spanning the
  delivery and the metadata update (inherent to on-demand rendering, but currently unacknowledged).
- `create` is a single atomic INSERT, but a duplicate id surfaces as a generic `Unavailable`
  (`tenant::unavailable` maps *every* driver error), not a typed conflict; no idempotency key.

### 3. Secret leakage (partly CONFIRMED, partly GUESS; severity: high)

No `secrets` table is queried and `password_hash` is dropped by `rows::user()` (CONFIRMED), so there
is no *direct* secret export. But the bundle copies free-text columns verbatim: `routines.prompt`,
`routines.email_senders`, `memory_documents.content`, `messages.blocks`, `bots.description`. REQ-046
allows secret values to be injected into runs and tool results; if such a value lands in any of those
text fields, the archive exfiltrates it with no redaction step. GUESS — verify how REQ-046 stores and
references secrets before asserting exposure; the point is that this module has no allow-list or
scrubber, so leakage is unchecked. Defense-in-depth also argues for removing `password_hash` from the
SELECT column list entirely, so the hash never enters the process during export.

### 4. Expiry and unbounded retention (CONFIRMED schema/code; severity: medium)

No `expires_at` column and no cleanup sweep for `account_exports` (GUESS: no external cron exists —
none found in the repo). `pending` rows from abandoned starts and `ready` rows with personal data are
retained indefinitely, in tension with the GDPR rationale of REQ-044. The download capability is the
bare id at a stable URL (`/api/v1/me/exports/{id}`), reusable by any session of the owner with no
expiry or rotation. There is no cap on exports per user, so both the row count and the repeated
full-bundle renders grow without bound.

## Concrete hardening for v0.4.0

1. **State machine + ownership in `mark_ready` (closes #2).** Change the UPDATE to
   `WHERE id=$1 AND user_id=$2 AND status IN ('pending','ready')`, require `rows_affected == 1`, and
   return a typed error otherwise; add `mark_failed(id, user_id, reason)` writing `status='failed'`
   plus a new `error_code` column, called from the download path's error branch. Replace the free-form
   `ExportRow.status: String` with a domain enum to make invalid states unrepresentable. Risk: the
   port signature change ripples to the app layer and the in-memory test store in `testkit`; status is
   a documented field, so the new `failed` value should be noted in `docs/PARITY.md` (internal-only
   change, no API shape change). Decide explicitly whether re-download re-marks `ready` (idempotent)
   or freezes the first archive — freezing conflicts with on-demand rendering.
2. **Snapshot isolation for `bundle` (closes #1).** In `tenant::as_system` (or a dedicated
   `as_system_snapshot`), issue `SET TRANSACTION ISOLATION LEVEL REPEATABLE READ` before the body, or
   return all sections from one statement with CTEs. Add a concurrency integration test that mutates
   rows between the section reads and asserts a self-consistent bundle. Risk: REPEATABLE READ can
   raise serialization failures under concurrent writes, requiring a bounded retry wrapper; longer
   snapshots add vacuum/bloat pressure. Verify the setting is accepted on the pooled connection
   before relying on it.
3. **Secret hygiene (closes #3).** Drop `password_hash` from the profile SELECT. Introduce an
   explicit export allow-list/scrubber for the free-text fields, resolving secret references to
   placeholders rather than values, driven by the REQ-046 secret model. Risk: false-positive redaction
   removes legitimate archive content (golden mismatch / DEVIATION); the archive's exact permitted
   content is a spec decision — write it under "Decisions needed" in `docs/STATE.md`, pick the safe
   default (redact), and get approval before shipping.
4. **Lifecycle and expiry (closes #4).** Add `expires_at timestamptz` + index, a cleanup job that
   deletes expired rows (and their artifact bytes per retention policy), an expiry check in `find`
   and `download`, and a per-user cap on live exports. Bind the download to a short-lived, rotated
   token rather than a permanent bare id. Risk: new migration and background job (single-maintainer
   simplicity cost); expired downloads becoming 404 is externally visible (DEVIATION → `docs/PARITY.md`
   + approval); the retention period needs an ADR. GUESS: confirm the original's retention window
   before choosing a number.
5. **Robustness and observability.** Add a bounded timeout around each `artifact_content.read` call
   (no timeout today — GUESS), cap artifact read concurrency, and emit structured tracing on
   `create`/`mark_ready`/`mark_failed` transitions with ids and counts only — never names, emails or
   secret values (per the "no personal data in logs" rule). Keep errors typed; never let a driver
   error reach the client verbatim.

## Verification to add

Integration tests (testcontainers) for: zero-rows-affected `mark_ready`, `mark_failed` on artifact
read error, no cross-user overwrite, a concurrent-write snapshot test, and expiry deletion. A
property/fuzz test for the redaction scrubber. Replay `spec/golden/export-start.json`. Traceability:
test names carry `REQ-044`. All claims above are CONFIRMED from source/schema reading except items
explicitly marked GUESS.
