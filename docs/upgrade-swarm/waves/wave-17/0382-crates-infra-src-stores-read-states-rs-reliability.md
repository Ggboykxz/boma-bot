# Worker 0382 — `crates/infra/src/stores/read_states.rs` — reliability lens (v0.4.0)

Status: proposal only (read-only worker). Workspace is `0.3.0-rc5` (`Cargo.toml:14`, CONFIRMED);
target v0.4.0. Scope: `PostgresReadStateStore::position`/`set` (`read_states.rs:26-92`) and callers
in `usecases.rs:4613-4665`. Evidence labels follow AGENTS.md (CONFIRMED = read this session,
GUESS = inferred, no reproducing test run).

## Current behavior (CONFIRMED)
- `position` = `SELECT last_seq WHERE thread_id AND user_id`, default `0` when absent
  (`read_states.rs:33-49`).
- `set` validates `last_seq >= 0`, then `INSERT ... ON CONFLICT (thread_id,user_id) DO UPDATE
  SET last_seq = EXCLUDED.last_seq, updated_at = now()`, guarded by `WHERE EXISTS (thread in
  caller space)` (`read_states.rs:58-88`). Unconditional overwrite; no ceiling at the thread's
  max seq; no compare-and-swap; no row lock.
- Each call opens its own transaction (`tenant::with_tenant` → `pool.begin()`, READ COMMITTED,
  no serialization retry, `tenant.rs:107-143`). `head_with` spans three separate transactions:
  `messages.page` → `set`/`position` → `messages.count_above` (`usecases.rs:4635-4657`).
- Roster unread = `count(*) WHERE seq > COALESCE(p.last_seq,0) AND role <> 'tool'`
  (`roster.rs:51-53`); `count_above` filters `role <> 'tool'` the same way (`messages.rs:224-226`).
  `page` does NOT filter tool rows, so `mark_read` marks to the newest row including hidden steps.

## Failure modes
1. Out-of-order marks (mechanism CONFIRMED; user impact GUESS). `set` overwrites unconditionally,
   so an older mark that commits later wins. `mark_read` computes `last_seq` from an earlier page
   read (`usecases.rs:4635-4647`); with two tabs/devices or a slow request, `set(5)` can commit
   after `set(6)`, regressing the divider and resurrecting phantom unread. `mark_unread` uses
   `set(0)` — a legitimate backward move — so a naive `GREATEST` guard would break REQ-009.
2. Lost unread (GUESS; no reproducing test found). `set` accepts any `last_seq >= 0` with no
   ceiling: a buggy/replayed client or a stale head writing a seq above the thread's real max
   makes `count_above` return 0 until messages pass that value — unread silently vanishes. Also
   `set` and `count_above` sit in different transactions, so a message inserted between them
   yields a returned `unread_count` that contradicts `read_seq` (phantom badge right after
   mark_read). Cancellation after `set` commits but before the response leaves state advanced
   with no ack (cancellation safety not handled in `head_with`).
3. Races (mechanism CONFIRMED). No lock/CAS: concurrent `set`s are last-writer-wins by commit
   order, not by mark order; the page→set sequence is a TOCTOU window. READ COMMITTED with no
   serialization retry means concurrent inserts/deletes are not seen at a common snapshot.
   The `EXISTS` guard and upsert share one statement's snapshot, but a thread deleted after that
   snapshot can leave a row referencing a removed thread (FK blocks/rejects). RLS keeps tenancy
   safe; this is an availability/consistency issue, not a cross-tenant leak.

## Concrete hardening (priority order)
- H1. Monotone read-advances without breaking unread: split the port into
  `advance(ctx, thread, last_seq)` = `ON CONFLICT ... SET last_seq = GREATEST(last_seq,
  EXCLUDED.last_seq), updated_at = now()` and `reset(ctx, thread)` that writes `0`. Map
  `mark_read`→advance, `mark_unread`→reset; keep the visible API identical. Risk: touches every
  `ReadStateStore` impl/wiring (`memory.rs:3431`, bin/api/service/testkit); "mark to an older
  seq" becomes impossible (not in spec, but check against the original oracle before shipping).
- H2. Ceiling the advance: inside the upsert use
  `LEAST(GREATEST(last_seq, EXCLUDED.last_seq), (SELECT max(seq) FROM messages WHERE thread_id=$1))`.
  Risk: extra subquery; if messages are hard-deleted and `max(seq)` shrinks, a clamp could
  regress the position — verify `seq` is append-only or apply the ceiling only when
  `EXCLUDED.last_seq` exceeds it. No schema/RLS change needed.
- H3. Self-consistent head: add `mark_and_head` doing the set and `count_above` in a single
  `with_tenant` closure (or one SQL statement with a CTE) so the returned `ThreadHead` is
  coherent. Risk: moves read logic into infra or adds a port method; larger diff. Keep the pure
  read-only `thread_head` path unchanged.
- H4. Observability without PII: a counter/`tracing::debug!` for backward-write attempts and
  ceiling clamps, excluding legitimate `reset`. Never label by thread/user id (cardinality +
  personal data). Risk: minimal; false positives until H1/H2 are in.
- H5. Tests (testcontainers Postgres, REQ-009 in names): stale `set(5)` after `set(6)` does not
  regress; `mark_unread` still resets to 0; `set(i64::MAX)` clamps to thread max; two concurrent
  advances converge to the max; head after a concurrent insert is self-consistent; property test
  over random mark sequences for monotonicity. Mark CONFIRMED only after running them.

## Risks & deviations
- Largest risk is H1's port change across many crates; mitigate by adding methods rather than
  replacing `set`, or keeping `set` as a compatibility shim.
- Monotone semantics may differ from the original (likely last-writer-wins, GUESS). If any
  externally visible behavior changes, record a DEVIATION in `docs/PARITY.md` and wait for
  approval before shipping.
- No migration required; PK `(thread_id,user_id)`, FKs and RLS policy stay unchanged.

## Open questions (GUESS)
- Is `messages.seq` strictly append-only per thread? H2's ceiling safety depends on it.
- Should `mark_read` define "latest" as newest *visible* message? `page` includes `role='tool'`
  rows while `count_above` excludes them; confirm the UI divider's intended position with the
  `REQ-009-unread-divider` golden before changing seq selection.
