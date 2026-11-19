# 0521 — `migrations/0055_signin_requests.sql` — architecture (v0.4.0)

- Worker #521 · Lens: architecture · Target: v0.4.0 · Read-only review; no code was changed.
- Module: `migrations/0055_signin_requests.sql` (29 lines) at `before-m5` (v0.3.0-rc5).
- Read for context (CONFIRMED by direct reads): `crates/infra/src/stores/signin.rs`,
  `crates/app/src/usecases.rs` (REQ-060 sign-in path), `migrations/0002_m1.sql` (`users`, `threads`),
  `migrations/0048_password_resets.sql`, `migrations/0054_external_agents.sql`, `spec/01-requirements.md`
  REQ-060, `AGENTS.md`, and wave-22 `0483-migrations-0036-artifacts-sql-architecture.md` (house format).

## Current structure (CONFIRMED)

One table `signin_requests`: `id text PRIMARY KEY`; `space_id NOT NULL → spaces ON DELETE CASCADE`;
`bot_id NOT NULL → bots ON DELETE CASCADE`; plain-text `requested_by`, `granted_by` (no FK), nullable
`thread_id` (no FK), `origin`, `display_name`; `status text DEFAULT 'pending'` with CHECK
`IN ('pending','granted','refused')`; `created_at`/`updated_at timestamptz DEFAULT now()`. Two indexes:
`(space_id)` and `(space_id, status)`. RLS enabled **and forced**, one `FOR ALL` policy whose USING and
WITH CHECK are both `space_id = boma.current_space_id() OR boma.is_system()`.

The header calls the table "an audit trail of who granted which origin to which bot" and promises no
secrets on the RPC surface — the DDL stores none, which is correct. The store confirms three query
shapes: `record` uses `ON CONFLICT (id) DO UPDATE` that rewrites `space_id`, `bot_id`, `requested_by`,
`thread_id`, `origin`, `display_name`, `updated_at` but leaves `status`/`granted_by` untouched
(lines 75–84); `list` is `WHERE space_id = $1 ORDER BY created_at DESC LIMIT $2` (lines 122–124);
`resolve` is `UPDATE ... SET status, granted_by, updated_at WHERE id AND space_id AND status='pending'`
(lines 152–156) and sets `granted_by` for **both** grant and refusal (lines 150, 154).

## Guiding principle for v0.4.0

Additive only: every change below is `ADD CONSTRAINT ... NOT VALID` + `VALIDATE`, a `CREATE INDEX`, or a
new table/function/trigger with schema-qualified `boma.*` objects. No dropped or renamed columns, no
type change, no replaced RLS policy, no ORDER BY or wire-shape change. Preserve column names, the two
existing indexes, and the exact RLS predicate — a rename or behavior change here is a DEVIATION
requiring `docs/PARITY.md` (AGENTS.md). Claims about plan shape or row counts are GUESS (no EXPLAIN or
stats were run in this read-only review).

## Concrete upgrades (5), each with a risk

1. **Encode the grant-state invariant.** Add `CHECK (status <> 'pending' OR granted_by IS NULL) NOT VALID`
   and `CHECK (status = 'pending' OR granted_by IS NOT NULL) NOT VALID`, then `VALIDATE` after an audit
   `SELECT`. This makes "pending has no resolver, resolved has one" a database guarantee matching
   `resolve`. *Risk:* `granted_by` is a misnomer — refusal stores the refuser's id (signin.rs:150), and
   legacy rows or another writer may already be inconsistent, blocking validation. *Mitigation:* audit
   `SELECT id, status, granted_by FROM signin_requests WHERE (status='pending') <> (granted_by IS NULL)`
   first; keep constraints `NOT VALID` and record conflicts in `docs/STATE.md` rather than rewriting
   history. Do **not** rename `granted_by` to `resolved_by` — that is a DEVIATION.

2. **Protect resolved rows from the upsert's rewrite.** The `record` conflict path (signin.rs:77–84) can
   change `origin`, `bot_id`, `requested_by`, `thread_id` of an already granted/refused request while
   leaving its `status`, silently corrupting the audit trail the header promises. Add an additive
   `BEFORE UPDATE ON signin_requests` trigger calling `boma.signin_requests_guard_immutable()` that
   raises unless every identity column is unchanged once `OLD.status <> 'pending'`; allow an exact
   no-op so retries stay idempotent. *Risk:* a retry of a resolved id currently succeeds with mutated
   values; the trigger turns it into an error, an externally visible change. *Mitigation:* let the
   guard pass when `NEW.* IS NOT DISTINCT FROM OLD.*` on those columns, document the remaining change
   in `docs/PARITY.md`, and gate with a REQ-060 test that records → resolves → re-records the same id.

3. **Index the query the store actually runs.** `list` sorts by `created_at DESC` (signin.rs:123) but the
   only candidate index leads with `(space_id, status)` and cannot satisfy that ORDER BY. Add
   `(space_id, created_at DESC)`; add `(bot_id, status)` only if a bot-scoped lookup is confirmed by
   EXPLAIN (GUESS: none exists today). *Risk:* write amplification on an insert/update table and a
   planner that may already use RLS plus the primary key. *Mitigation:* run `EXPLAIN (ANALYZE, BUFFERS)`
   on the three store queries first, ship only indexes that change a plan, use plain `CREATE INDEX`
   (concurrent build cannot run inside a migration transaction), and keep each reversible via
   `DROP INDEX`.

4. **Close the safe referential gap, decide the unsafe ones explicitly.** `thread_id` is the only
   nullable provenance column with no FK; `threads(id)` exists (`0002_m1.sql:155`) and uses
   `ON DELETE CASCADE`. Add `thread_id REFERENCES threads(id) ON DELETE SET NULL NOT VALID` + validate
   and a `(thread_id)` index. Actor ids `requested_by`/`granted_by` are deliberately denormalized: an FK
   with `RESTRICT` would block user deletion, while `SET NULL` on `granted_by` would break upgrade 1 for
   granted rows. *Risk:* stale `thread_id` values block validation, and `SET NULL` changes today's
   dangling-thread behavior. *Mitigation:* audit orphans and null/backfill them before validating; for
   actor ids, keep them text, add an additive audit helper `boma.signin_actor_exists(id)` used by
   reporting, and raise the FK policy as a `docs/STATE.md` decision rather than guessing.

5. **Bound the audit row's free text.** `origin` has only a non-empty CHECK (line 17); `display_name`
   has none. Add `CHECK (length(btrim(display_name)) > 0) NOT VALID` and
   `CHECK (length(origin) <= 2048) NOT VALID`, then validate after an audit. *Risk:* legacy rows with an
   empty display name or a long origin block validation, and a 2048 cap may reject a legitimate
   deeply-nested URL. *Mitigation:* audit distinct lengths first, keep either constraint `NOT VALID`
   (new writes only) if real data disagrees, and avoid a scheme regex (`https?://`) — custom schemes
   are plausible for native sign-in targets and would be an over-constraint.

## Cross-cutting risks / sequencing

- Sequence 1 → 2 → 3 → 4 → 5: audit before `VALIDATE`; the immutability trigger after the invariant so
   both agree on "resolved"; indexes after EXPLAIN; bounds last. Each step is independently revertable.
- Freeze while doing this: column names (especially `granted_by`), the two existing indexes, the exact
   RLS predicate, and all `signin.rs` query strings / `ORDER BY created_at DESC`. The store's explicit
   column list means additive columns would not change the RPC response, but none are needed here.
- Keep RLS's system-context asymmetry exactly: `boma.is_system()` gets full access while the store
   always runs inside `tenant::in_space`; do not add a `RESTRICTIVE` policy that changes this.
- Concurrency is already correct: two `resolve` calls on one id serialize on the row lock and the second
   gets `Conflict` — do not "fix" it with a new lock object.
- Traceability: no new REQ id (context is REQ-060); `tools/trace_check.py` is unaffected. Gate with
   `cargo fmt --check`, `clippy -D warnings`, a REQ-060 pending → grant/refuse → re-record round-trip,
   and the existing `testkit` `list_signin_requests` test. No new dependency.

## Out of scope for this module

TTL/expiry of stale pending approval cards — useful, but REQ-060 says nothing about it, so it is a
proposal for owner approval, not a v0.4.0 change. Also out of scope: renaming `granted_by`, changing the
status CHECK domain, replacing the RLS policy, adding actor FKs, the recording-browser/vault code, and
the RPC/DTO shape. Those belong to other workers; this proposal only strengthens the `signin_requests`
table structure additively.
