# Worker #331 — Architecture proposal: `crates/infra/src/stores/bots.rs` (v0.4.0)

Lens: architecture. Scope: REQ-004 (one continuous thread per bot). Read-only review; no code changed.
Claims are **CONFIRMED** (verified by reading the worktree this session) or **GUESS** (inferred).

## Current shape (CONFIRMED)

- `PostgresBotStore` is a thin adapter: `new(pool)`, plus `BotStore::{create_with_thread, find}`.
- `create_with_thread` runs **two INSERTs inside one `tenant::in_space` transaction** (bot first,
  thread second). This is the REQ-004/REQ-042 atomicity guarantee, and it is sound.
- Tenant scoping is asymmetric: `find` uses `with_tenant(context, ...)`; `create_with_thread` uses
  `in_space(&pool, &bot.space_id, ...)` because the port carries no `TenantContext`
  (`crates/app/src/ports.rs:758-783`). The caller-supplied id structs are trusted.
- `bots.thread_id` is plain text, deliberately not a FK (two-FK cycle); `threads.bot_id` is a FK.
  The only DB index on `threads.bot_id` is **non-unique** (`migrations/0002_m1.sql:291`).
- `ThreadStore::for_bot` enforces "one conversation" only by
  `WHERE bot_id=$1 AND kind='bot' ORDER BY created_at ASC LIMIT 1` (`stores/threads.rs:65-67`).
- `rows::bot` does `ThreadId::new(row.thread_id.unwrap_or_default())`: a NULL `thread_id` becomes
  an empty id, which `opaque_id!` rejects (`domain/src/ids.rs:29-34`). A half-created bot therefore
  makes `find` return a domain error, not `None` (CONFIRMED by code path).
- Create returns hand-built aggregates with defaults restated in Rust (`enabled_tools: vec![]`,
  model fields `None`, seqs `1`, `external: None`), duplicating schema defaults from migrations
  0005/0006/0042.
- `tenant::unavailable` maps every SQL error to `AppError::Unavailable`; `is_unique_violation`
  exists in `tenant.rs:159-164` but is unused here.
- Coverage: conformance replays golden `bot-create` (`conformance/src/golden_world.rs:90,158,203`);
  a grep found **no test whose name carries `req_004`** in `crates/infra/tests` (CONFIRMED).

## Upgrade 1 — Make REQ-004 a database invariant, not a query convention

Add `CREATE UNIQUE INDEX threads_one_bot_thread ON threads (bot_id) WHERE kind = 'bot'` and have
`ThreadStore::for_bot` read `bots.thread_id` (or assert the indexed row matches it). Today two
concurrent/duplicate bot-thread inserts can both commit; `LIMIT 1` then silently picks one and the
two identity columns can disagree.
*Risks:* existing duplicate rows would block the index; `bots.thread_id` is unconstrained text and
may dangle. *Mitigation:* pre-migration audit query, archive/backfill extra threads, create the
index `CONCURRENTLY`, and keep the current `for_bot` query as a temporary fallback.

## Upgrade 2 — One aggregate for the create input; derive the tenant from context

Replace the loose `(NewBot, NewThread)` pair with a single validated `NewBotWithThread` (or add a
fallible constructor in `app`) so `bot.id == thread.bot_id`, `bot.space_id == thread.space_id`, and
the `thread.id` written into `bots.thread_id` are equal **by construction** — make the invalid state
unrepresentable instead of re-checking it in SQL. Prefer `with_tenant` with the authenticated
context over trusting `bot.space_id`.
*Risks:* touches the port and the use case (`usecases.rs:367-388`), widening the diff; a context-
derived space changes behavior for product-owned creates and may be a DEVIATION. *Mitigation:* keep
the public trait signature for parity, validate inside the adapter with a typed error, and put any
context change behind an ADR + `docs/PARITY.md` entry.

## Upgrade 3 — Return database state instead of restating defaults

Use `INSERT ... RETURNING` and feed both rows through the existing `rows::bot` / `rows::thread`
mappers, so a schema default change (migrations 0005/0006/0042) cannot silently diverge from what
the RPC reports. Remove the explicit `None`/`false` binds and let the database own the defaults.
*Risks:* `RETURNING` columns must match `BotRow`/`ThreadRow` exactly; mapping now runs on the create
path too. *Mitigation:* compile-time-checked SQLx queries plus a test asserting the created
aggregate equals a subsequent `find` + `for_bot`.

## Upgrade 4 — Typed errors and explicit half-created-bot handling

Use `tenant::is_unique_violation` to distinguish a duplicate id from an outage, and handle a NULL
`bots.thread_id` explicitly (treat as corrupt/incomplete, not as `ThreadId::new("")`). Keep driver
detail at debug level (REQ-054). Consider idempotent reuse keyed by the supplied ids, since
`/rpc/bots/create` is retryable.
*Risks:* changing the client-visible status for conflicts is an externally visible change
(DEVIATION) and needs approval; idempotency is a semantic addition the spec does not request.
*Mitigation:* start with distinct logging/metrics and a clear internal error, then propose the
wire-level change separately in `docs/STATE.md` / `docs/PARITY.md`.

## Upgrade 5 — Close the traceability gap for REQ-004

Add a named integration test in `crates/infra/tests` (e.g. `req_004_one_continuous_thread_per_bot`)
asserting: repeated opens yield the same `for_bot` id; both rows roll back together when the second
INSERT fails; a cross-tenant `find` returns `None`; and an archived thread stays readable with
`includeArchived`. Keep the `bot-create` golden replay as the wire-level check.
*Risks:* needs a seeded Postgres test database; low risk, test-only. The 1:1 transaction rollback
and cross-tenant behaviors are exercised piecemeal today but not under the REQ-004 name
(`tools/trace_check.py` counts declaration names only).

## Sequencing / recommendation

Upshift 3 and 5 first (near-zero risk, immediate drift/traceability wins), then 2 (bounded app-layer
change), then 1 (schema migration with data audit), then 4 (approval-gated behavior change). Net
effect: REQ-004 becomes enforced by the schema and enforced by tests, with the adapter carrying one
mapping path and honest errors — without changing the public API contract.
