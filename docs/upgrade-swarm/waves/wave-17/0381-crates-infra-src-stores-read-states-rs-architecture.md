# Architecture proposal — `crates/infra/src/stores/read_states.rs` (v0.4.0)

- Worker: 381 · Lens: architecture · Module: `PostgresReadStateStore` · Requirement: REQ-009
- Status: proposals only (CONFIRMED = read in this session; GUESS = inferred). No code changed.

## Current structure (CONFIRMED)

A thin adapter: `PgPool` behind `#[derive(Debug, Clone)]`, `new()` is `const`, and the
`ReadStateStore` port (`crates/app/src/ports.rs:1199`) exposes exactly two methods —
`position` and `set`. Both funnel through `tenant::with_tenant`, so every statement runs in a
transaction that sets `boma.space_id` locally (ADR-002), and all driver errors collapse through
`tenant::unavailable` into `AppError::Unavailable`.

- `position` (lines 28-50): `user_id = context.user_id().cloned().ok_or(NotFound)?`;
  `SELECT last_seq ... WHERE thread_id=$1 AND user_id=$2`; `fetch_optional`; default `0` when no
  row. **No explicit space predicate** — isolation rests entirely on RLS.
- `set` (lines 52-91): rejects `last_seq < 0` with `AppError::Rule(DomainError::InvalidName)`;
  requires both `user_id` and `space_id` from the context; a single
  `INSERT ... WHERE EXISTS (thread belongs to space) ON CONFLICT (thread_id, user_id) DO UPDATE
  SET last_seq = EXCLUDED.last_seq, updated_at = now() RETURNING last_seq`. The `EXISTS` guard is
  the "defense in fact" and the RLS policy the "defense in depth". Row 0 → `NotFound`.
- Schema (`migrations/0010_read_states.sql`): PK `(thread_id, user_id)`, `last_seq bigint >= 0`
  DEFAULT 0, `updated_at timestamptz`, FK cascade on thread/space/user, RLS FORCE with a
  `space_id = boma.current_space_id() OR boma.is_system()` policy.
- Consumer: `PostgresRosterStore::list_bots` (`roster.rs:51-59`) does
  `count(messages) WHERE m.seq > COALESCE(p.last_seq,0) AND m.role <> 'tool'` via a LEFT JOIN.
  Snooze lives there, not here (correct: the divider is derived, not stored).
- The in-memory twin (`testkit/src/memory.rs:3411`) mirrors both methods and the negative guard.

Architecturally the file is clean and small; the upgrades below target *invariants*, not size.

## Concrete upgrades (v0.4.0)

- **U1 — Make the position monotonic and skip no-op writes.** `set` currently overwrites with
  `EXCLUDED.last_seq`, so a late/stale client can move the divider *backwards* and resurrect
  already-read messages. Change to `SET last_seq = GREATEST(thread_read_states.last_seq,
  EXCLUDED.last_seq), updated_at = now() WHERE thread_read_states.last_seq < EXCLUDED.last_seq`
  and, on conflict-no-op, still `RETURNING last_seq`. Precedent exists in `runs.rs:273-278`
  (`GREATEST(steering_consumed_seq, $2)`). Also removes write amplification on repeated reads of
  the same seq. *Risk:* externally visible retry semantics and the returned value change when a
  smaller seq is sent → DEVIATION in `docs/PARITY.md`, needs approval; must mirror in the memory
  store; verify `markUnread` is not expected to lower the position (spec/SHOULD currently only
  defines forward reads and snooze — CONFIRMED from `spec/01-requirements.md:74-80`, GUESS on the
  original's exact intent).

- **U2 — Give `position` the same defense-in-fact as `set`.** Add the space predicate to the
  SELECT, matching `set`'s `EXISTS`/`WHERE` pattern, so a superuser connection (which RLS does not
  bind) cannot leak another tenant's position. Concretely: bind `space_id` and require
  `EXISTS (SELECT 1 FROM threads WHERE id=$1 AND space_id=$2)`; return `0` when absent. *Risk:*
  `TenantContext::space_id()` is `None` for `System` contexts (`domain/src/tenancy.rs:108-113`),
  so the guard must apply only to `Member` contexts or system paths lose a legitimate cross-tenant
  read; one extra indexed lookup on a hot read. Hold at GUESS whether any system path reads read
  states at all.

- **U3 — Classify driver errors instead of flattening them.** `tenant::unavailable` turns FK
  violations, CHECK failures and pool timeouts all into 503. Use `tenant::is_unique_violation`
  (`tenant.rs:158`, currently unused here) and a narrow match: constraint/FK → `Rule`/`NotFound`,
  timeout/disconnect → `Unavailable`, keeping driver text at `debug` (REQ-054) and checking
  `AppError::is_client_fault()`. *Risk:* a wrong split turns a real outage into a client fault
  (or vice versa); adds branches to a now-trivial method. Cross-reference wave-17/0376 H2.

- **U4 — Bound and observe the hot write path.** Add a `tracing::debug` span with outcome
  (`advanced`/`noop`/`refused`/`failed`) and ids only — never `user_id`/`thread_id` values as
  metric labels (PII; no personal data in logs). Document reliance on the pool's acquire timeout;
  optionally enforce a statement timeout. *Risk:* log volume on read-heavy rooms; keep at
  `debug`, no new metric cardinality.

- **U5 — Cap `last_seq` to a real message (integrity, GUESS).** `set` accepts any `i64`, so a
  malformed or malicious client could mark unread history as read by naming a seq beyond the
  thread's last message. Add `AND $4 <= COALESCE((SELECT max(seq) FROM messages WHERE
  thread_id=$1), $4)` semantics or a domain-level cap. *Risk:* the seq space may legitimately
  include gaps/reserved values, and an extra `max(seq)` lookup adds cost; this is a behaviour
  change → DEVIATION and needs spec clarification before shipping. Defer to v0.5.0 if unsure.

## Constraints / non-goals (CONFIRMED)

- Keep the two-method port and the `i64` seq type; do not add a thread-wide position (module doc:
  two readers = two rows).
- Keep both the RLS policy and the explicit space predicate in `set`; do not remove either layer.
- Snooze/archived filtering stays in `roster.rs`; this store owns only the per-member position.
- No new dependency, no blocking call, and all values stay parameterized (persistence standard).

## Tests to add (traceability: `req_009_` in the name)

- `req_009_stale_position_does_not_regress` — set 5, then set 3, assert 5 (U1).
- `req_009_position_cannot_cross_tenants` — keep existing `tenancy.rs:122`; add a superuser-path
  variant for U2.
- `req_009_constraint_error_is_client_fault` — classification (U3).
- Keep `crates/infra/tests/m1.rs` round-trip green; mirror U1 in `MemoryReadStateStore`.

**Recommendation:** ship U1 + U2 + U4 together (real invariant fixes, low blast radius, but file
a DEVIATION for U1's changed return value); treat U3 as a small follow-up and defer U5 until the
seq semantics are confirmed in `docs/STATE.md`.
