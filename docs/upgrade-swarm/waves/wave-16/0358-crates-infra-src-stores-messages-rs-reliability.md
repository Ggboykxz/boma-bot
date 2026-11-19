# Worker 0358 — `crates/infra/src/stores/messages.rs` — reliability lens — v0.4.0

Status: PROPOSAL (read-only review; no code changed). Evidence tags: **CONFIRMED** = read from
this worktree's source/migrations this session; **GUESS** = inferred runtime consequence.

## Scope

`PostgresMessageStore` implements `MessageStore` (`insert`, `find_by_nonce`, `page`, `count_above`,
`set_text`). Every method runs in one `tenant::with_tenant` transaction (RLS tenant setting,
commit-on-ok / rollback-on-error). Schema evidence: `migrations/0002_m1.sql` (messages table,
`UNIQUE (thread_id, seq)`, partial unique index on `(thread_id, client_nonce) WHERE NOT NULL`,
`threads.next_message_seq`), `migrations/0007_replies.sql` (`reply_to_message_id` FK).

## Failure modes

### 1. Ordering / sequence allocation
- **Hot single-row counter.** `insert` allocates `seq` with `UPDATE threads SET next_message_seq+1
  ... RETURNING` in the same transaction (CONFIRMED L51-58). All sends to one thread serialize on
  that thread row. Contention → lock waits; `with_tenant` sets no `statement_timeout` or
  `lock_timeout` (CONFIRMED `tenant.rs` L115-129) → a send can hang until the client gives up, and
  a cancellation rolls back the whole write (GUESS: source of "lost" sends under load).
- **No retry on transient faults.** A serialization failure (40001) or deadlock (40P01) is *not* a
  unique violation (`is_unique_violation` checks only `Database.is_unique_violation`, CONFIRMED
  `tenant.rs` L159-164), so it maps to `AppError::Unavailable` with zero retries (CONFIRMED L104)
  (GUESS: needless 5xx on recoverable races).
- **Missing thread is misreported.** If the thread row is absent/deleted, `fetch_one` on the counter
  returns no rows → `unavailable` → `Unavailable`, not `NotFound` (CONFIRMED L56-58).
- Gaps are *not* created by a failed insert because the bump rolls back with the transaction
  (CONFIRMED by `with_tenant` rollback semantics). Good — preserve this.

### 2. Dedup / idempotency
- **TOCTOU across two transactions.** A nonce conflict is detected inside txn 1 (rolled back), then
  `find_by_nonce` runs in a *separate* txn 2 (CONFIRMED L110-129). If the nonce row is deleted
  between them, txn 2 returns `None` → `Unavailable`: a replay becomes a hard error (GUESS).
- **Every unique violation is blamed on the nonce.** Any `is_unique_violation` becomes
  `AppError::Conflict` (CONFIRMED L103), but three constraints can fire: the nonce index,
  `UNIQUE (thread_id, seq)`, and the `id` PRIMARY KEY (client-supplied id). The last two lead to
  "conflicted but no message carries that nonce" → `Unavailable`, masking the real cause
  (CONFIRMED L121-124).
- **`nonce = None` collisions are unexplainable by design.** A duplicate id or seq with no nonce
  hits "a message was refused for a reason its nonce cannot explain" → `Unavailable` (CONFIRMED
  L117-120) instead of a clean `Conflict` (GUESS: clients cannot distinguish a bug from a replay).
- **`find_by_nonce` is unscoped by space** and relies solely on RLS (CONFIRMED L144-153). Under a
  superuser/owner role RLS does not apply (see `count_above` comment, L222-223), so a cross-tenant
  lookup is possible (GUESS, same latent risk shared by `page`/`set_text`).

### 3. Pagination
- **Unbounded `limit`.** `i64::try_from(limit).unwrap_or(i64::MAX)` with no cap (CONFIRMED L171);
  a large caller-supplied limit fetches unbounded rows → memory/availability failure (GUESS).
- **Dangling cursor / extra round trip.** The query fetches exactly `limit` rows (not `limit+1`), and
  `next_cursor` is emitted whenever `len == limit` (CONFIRMED L181, L199-203). When that page is the
  true last one, clients still receive a cursor and must issue one final empty fetch (CONFIRMED).
- **One bad row poisons the whole page.** `rows::message` maps any row with undecodable `blocks` to
  `Unavailable` (CONFIRMED `rows.rs` L325-329), and `page` collects with `?` (CONFIRMED L193-196):
  a single corrupt row fails the entire page.
- **Keyset is stable against new appends** (`seq < before`, `ORDER BY seq DESC`, seq unique per
  thread, CONFIRMED L179-181) — preserve this; do not switch to OFFSET.

### 4. Orphan rows
- **`set_text` is a silent no-op.** It ignores `rows_affected` (CONFIRMED L251-256): updating a
  deleted/foreign-tenant message returns `Ok(())`, so streamed reply text is silently lost (GUESS).
- **Unenforced references.** `bot_id`, `author_user_id`, `run_id` are plain text with no FK
  (CONFIRMED `0002_m1.sql` L175-178; run is written after the message by design). `rows::message`
  only parses their *format* (CONFIRMED `rows.rs` L336-340), so dangling ids surface to clients.
  `reply_to_message_id` is the exception: FK `ON DELETE SET NULL` (CONFIRMED `0007_replies.sql` L7).

## Concrete hardening (with risks)

1. **Resolve dedup in one transaction.** On unique violation, look the nonce up in the *same* txn
   before rolling back, and match the violated constraint (Postgres `constraint_name` / SQLSTATE
   + column) instead of assuming the nonce. Fall back to `find_by_nonce` only when the violation is
   actually the nonce index. *Risk:* error mapping changes (`Unavailable`→`Conflict`) — externally
   visible; record any API change in `docs/PARITY.md` and get owner approval.
2. **Atomic replay via SQL.** Prefer `INSERT ... ON CONFLICT (thread_id, client_nonce) WHERE ...
   DO NOTHING RETURNING`; if no row returns, `SELECT` the existing one in the same txn. Kills the
   TOCTOU window. *Risk:* requires a migration/index tweak and the `WHERE` partial-index clause must
   match exactly, or the conflict target is rejected.
3. **Bound transient faults.** Add bounded retry (2-3 attempts, small jitter) for 40001/40P01 only,
   plus `statement_timeout`/`lock_timeout` on write transactions. *Risk:* retries amplify load on an
   already-hot counter; keep attempts few and never retry `Conflict`.
4. **Cap and correct pagination.** Clamp `limit` to a fixed `MAX_PAGE_SIZE` (reject or clamp) and
   fetch `limit+1` rows, deriving `next_cursor` from whether an extra row exists. *Risk:* clamping
   changes a public parameter's behavior → DEVIATION / parity note; decide reject-vs-clamp with owner.
5. **Make `set_text` report reality.** Check `rows_affected`; return `NotFound` on 0 (or include
   `AND space_id = $tenant` so cross-tenant updates cannot succeed). *Risk:* callers that relied on
   a silent success will now see errors — verify use sites in `crates/api` and `crates/app`.
6. **Close orphan/tenant gaps.** Add explicit `space_id` predicates to `page`/`find_by_nonce`/
   `set_text` as defense in depth (as `count_above` already does, L225-226), and reconcile
   `count_above`'s `space_id().ok_or(NotFound)?` so system contexts behave consistently. *Risk:*
   FKs on `run_id`/`bot_id` cannot be added while runs are written after messages — do not attempt;
   limit to read-side tolerance (skip/flag unparseable rows) rather than schema changes.
7. **Reconcile missing-thread error.** Map the empty counter `fetch_one` to `AppError::NotFound`.

## Verification to add (REQ ids in test names)

- `req_005_replay_during_delete_returns_message_or_clean_error` — race nonce-delete vs replay.
- `req_002_duplicate_id_and_seq_report_conflict_not_unavailable` — constraint-specific mapping.
- `req_009_page_limit_is_capped_and_cursor_is_exact` — `limit+1` and clamp behaviour.
- `req_054_set_text_missing_row_is_not_silent` — 0 rows → error.
- Keep `req_005_two_concurrent_sends_get_different_sequences` and `req_002_the_same_nonce...`
  (`crates/infra/tests/concurrency.rs`) green as the regression baseline.

Note: the nonce-conflict pattern is duplicated in `stores/runs.rs` (L93-120); a shared fix should be
considered so the two stores do not drift (GUESS: coordinate with that module's worker).
