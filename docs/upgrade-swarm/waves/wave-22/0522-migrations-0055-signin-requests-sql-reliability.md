# 0522 — migrations/0055_signin_requests.sql — reliability (v0.4.0)

Worker #522 · module `migrations/0055_signin_requests.sql` (29 lines) · lens: reliability · READ-ONLY
review of `rebuild-workspace/.worktrees/before-m5` (v0.3.0-rc5). Proposals only; no code or migration
changed here. Spec anchor: REQ-060 (`spec/01-requirements.md:500-510`). CONFIRMED = read this session
(0055, store/domain/usecases, peers, wave-18/21/22 reviews); GUESS = runtime likelihood/impact, no test
was run. **Additive only:** every item is a NEW forward-only migration (`ADD COLUMN`, `ADD CONSTRAINT
... NOT VALID`, new indexes, tests) — never edit 0055, never drop/rename/retype, never widen RLS.

## What it already does right (keep, do not re-propose)

- `id text PRIMARY KEY` (L5) is the answer capability; the store's `resolve` guards single use with a
  conditional `UPDATE ... WHERE id=$1 AND space_id=$4 AND status='pending'` (infra/stores/signin.rs
  :151-169), so concurrent answers serialise on the row lock (CONFIRMED shape, wave-18/0398).
- `status` CHECK (L16) and non-empty `origin` CHECK (L17) hold the wire vocabulary and a minimum shape;
  `created_at`/`updated_at` are server-defaulted (L14-15); FKs cascade on space/bot delete (L6-7).
- `ENABLE` + `FORCE ROW LEVEL SECURITY` with symmetric `USING`/`WITH CHECK` on `space_id` (L23-29) fail
  closed without a space context (CONFIRMED).

## F1. Token replay — the audit target can be rewritten under a live grant

- The table keeps only the *current* target (`origin`, `bot_id`, `display_name`, `thread_id`, L8-11)
  beside a terminal `status`/`granted_by` (L12-13). `record` is an upsert by id that overwrites those
  target columns while preserving `status`/`granted_by` (store :77-84), so a replayed id on a granted
  row rewrites what was approved while the grant stays: the audit trail 0055 promises no longer matches
  the decision (CONFIRMED shape; GUESS likelihood — ids are unguessable, tool retries are plausible).
  The schema cannot detect it: no terminal-state invariant and no frozen snapshot of the grant target.
- **P1.** Add nullable `granted_origin text`, `granted_bot_id text`, `resolved_at timestamptz` (no
  default) and fill them in the *same* conditional UPDATE as `resolve` (additive store change). A replay
  that rewrites `origin`/`bot_id` then leaves the frozen columns intact, so the approved target survives.
  Add `CHECK (status <> 'pending' OR (granted_by IS NULL AND granted_origin IS NULL AND
  resolved_at IS NULL))` and `CHECK (status = 'pending' OR (granted_by IS NOT NULL AND
  granted_origin IS NOT NULL AND resolved_at IS NOT NULL))`, added NOT VALID → audit → VALIDATE.
  *Risk:* store change must land atomically with the migration or new terminal rows fail VALIDATE;
  refused rows currently also set `granted_by` (usecases.rs:9765-9775), so audit history before VALIDATE;
  keep the new columns out of the RPC view (REQ-060 "never plaintext"). No auto-repair trigger — it
  would hide the defect instead of fixing it.
- **P2.** Add `CHECK (octet_length(id) <= 256)`, `octet_length(origin) <= 2048`,
  `octet_length(display_name) <= 512` as NOT VALID → audit → VALIDATE. `id` is the bearer token and is
  returned by `bots.signIns`, so an unbounded id/origin is a storage and scan sink.
  *Risk:* caps are GUESS until real payload lengths are measured; too small rejects legitimate asks.

## F2. Expiry — a pending id is a capability with no deadline

- No `expires_at`, TTL, or age predicate exists (L1-18), and `list` has no staleness bound. A pending
  answer token stays live forever and stale rows accumulate; an old id can still grant long after the
  consent context is gone (CONFIRMED). RLS cannot express time, so today only app logic could gate it.
- **P3.** Add `expires_at timestamptz` (nullable, DEFAULT NULL → semantics unchanged) and a partial
  index `signin_requests_pending_age ON signin_requests (created_at) WHERE status='pending'` to bound
  a future stale scan; optionally `CHECK (expires_at IS NULL OR expires_at > created_at)` NOT VALID.
  *Risk:* the real gate (`resolve ... AND (expires_at IS NULL OR expires_at > now())`, typed `Expired`)
  is an app/store change that makes old ids fail → externally visible: ADR + `docs/PARITY.md` DEVIATION
  + owner approval. The cross-clock CHECK is unsafe while `record` binds caller timestamps but
  `resolve` uses DB `now()` (store :76,155) — align on one clock first. No DELETE/sweeper in a migration:
  destructive and FORCE RLS hides rows from a maintenance scan.

## F3. RLS — tenant scope is correct but fragile under a system context

- The policy trusts the row's own `space_id` and gives a blanket bypass to `boma.is_system()` (L28-29).
  (a) `resolve` looks up by PK then filters `space_id`, so reachability depends entirely on the
  transaction-scoped `boma.space_id` GUC; if it is missing, FORCE RLS hides the row and `resolve`
  returns `Conflict`, conflating "no context" with "already answered" (CONFIRMED from policy + store).
  (b) Under `is_system()` a write can re-tag a row to another space; RLS cannot tell maintenance from
  acting-for-a-user (CONFIRMED shape, GUESS abuse).
- **P4.** Keep `ENABLE`/`FORCE`/policy byte-identical; add integration tests: a non-system role cannot
  see or update another space's row, a no-space context yields no visibility, and `record` payload
  `space_id == context.space_id()` (usecases.rs:9737-9745) is asserted. *Risk:* none at SQL level — tests
  only. Do not add a new permissive policy or a `USING (true)` bypass.
- **P5.** Run every new `VALIDATE CONSTRAINT` and backfill under a BYPASSRLS / `is_system()` role, after
  an explicit cross-tenant audit query. *Risk:* under FORCE RLS a normal-context VALIDATE can scan a
  filtered/zero set and mark a constraint valid while dirty rows in other spaces remain — a silent
  integrity hole. Keep the `is_system()` escape hatch and FORCE semantics unchanged.
- *Gated (not additive):* a restrictive policy or trigger forbidding `space_id` changes after insert.
  Needs ADR + owner approval; could break legitimate system maintenance.

## F4. Access path — `list` has no supporting index

- `list` is `WHERE space_id=$1 ORDER BY created_at DESC LIMIT $2`; neither index (L20-21) serves that
  order, so it is a seq-scan-plus-sort per space that grows with history (CONFIRMED schema; GUESS size).
- **P6.** Add `CREATE INDEX CONCURRENTLY signin_requests_space_created ON signin_requests
  (space_id, created_at DESC, id)` to serve `list` and give a deterministic tiebreaker.
  *Risk:* sqlx `migrate!` wraps a migration in a transaction (infra/db.rs:20,72), so CONCURRENTLY cannot
  run there — verify `-- no-transaction` support or stage separately; confirm with EXPLAIN. If P3 lands,
  prefer a matching sweep index to avoid a second build.

## Non-goals, rollout, cross-cutting risks

- No edits to 0055, no drops/renames/type changes, no DELETE/sweeper, no RLS widening, no auto-repair.
  Store/port semantics (insert-only re-record, actor check, typed errors, limit clamp) belong to sibling
  reviews 0397/0398/0261/0262 — not re-proposed here.
- Wave A (additive, reversible): P2, P3 columns/index (NULL defaults), P4 tests, P5 validation discipline,
  P6 index after framework check. Wave B (gated): P1 frozen snapshot + CHECKs (coordinated store change),
  expiry gate (DEVIATION/ADR).
- R1: validate/backfill only under BYPASSRLS/is_system (F3). R2: hardening tests must carry REQ-060 in
  the test name — `tools/trace_check.py` counts declaration lines, not comments. R3: never log or return
  the id, origin, or actor in new error paths (REQ-060 "never plaintext"); clippy `-D warnings`, no
  unwrap/expect/panic outside tests. All likelihood/impact stays GUESS until these tests run.

## Evidence / bottom line

- CONFIRMED — read this session: `migrations/0055_signin_requests.sql:1-29`;
  `crates/infra/src/stores/signin.rs:77-84,137-175`; `crates/app/src/usecases.rs:9737-9775`;
  `crates/infra/src/db.rs:20,72`; `spec/01-requirements.md:500-510`; wave-18/0397,0398, wave-21/0456,
  wave-22/0482. GUESS — replay likelihood, table size, payload caps, abuse of `is_system()`.
- 0055 gets the hard part right (single-use CAS + fail-closed tenant RLS), but its schema keeps the
  approved target mutable, has no expiry, and can be scanned without a system role only under a narrow
  policy. P1–P3 harden replay, expiry and validation truthfulness; P2/P4/P6 are safe additive starts,
  while P1 and the expiry gate change behavior and need coordinated store work, ADRs and owner sign-off.
