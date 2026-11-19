# Worker 0450 — `migrations/0018_screen_capabilities.sql` — Reliability (v0.4.0 proposal)

Scope: the capability table, its one index and its RLS block (REQ-030), and the seams with
`0017_computers.sql`, `crates/infra/src/stores/screen.rs` and `crates/app/src/usecases.rs`.
Evidence tags follow AGENTS.md: CONFIRMED = read in this session, GUESS = needs a test/oracle run.
Auditor proposal; no code or schema changed. Every suggestion is **additive** (new columns,
constraints, indexes, `SET DEFAULT`, `COMMENT`) — no drops, no rewrites, no wire change unless
explicitly flagged as a DEVIATION.

## Current state (CONFIRMED)

0018 creates `screen_capabilities (token_hash bytea PK, space_id text FK spaces, computer_id text
FK computers, screen_generation integer >= 0, created_at timestamptz)` plus
`screen_capabilities_computer_idx (computer_id)`, then enables + FORCEs RLS with the repo-standard
space policy `space_id = current_space_id() OR is_system()` (0018:8-33). Helpers are STABLE GUC
readers that fail closed when unset (`0002_m1.sql:26-42`). `computers.screen_generation` is
`integer >= 0 DEFAULT 0` (0017:21). Issue binds a caller clock string and a raw hash
(`screen.rs:38-58`); resolve reads as **system** (`tenant::as_system`, `screen.rs:66-92`), then
re-reads the computer as system and checks `status = Running AND generation == capability`
(`usecases.rs:6306-6333`). Hashing is SHA-256 (`crypto.rs:111`), so a hash is 32 bytes.

## Failure modes

**FM-1 — `space_id` and `computer_id` are not tied together (CONFIRMED schema).**
Two independent FKs let a row claim space A while pointing at a computer in space B. Issue happens
to derive both from the same context (`usecases.rs:6275, 6283-6286`), but nothing in the DB enforces
it. Resolve looks the computer up by `computer_id` in a system context and never cross-checks
`capability.space_id` (`usecases.rs:6314-6327`), so a mismatched row returns a capability whose
reported space is wrong. GUESS: only reachable via a system-path mint bug or future call-site drift,
but the failure is silent and defeats the "space the computer belongs to" contract in the header.

**FM-2 — The read path bypasses the tenant policy entirely (CONFIRMED code path).**
`find_by_hash` runs as system, where `is_system()` is true, so `USING (... OR is_system())` is
unconditionally true: RLS gives zero protection on the only path that resolves capabilities.
Reliability consequence: resolution correctness rests on `tenant::as_system` setting
`boma.system_reason` inside the transaction. If the GUC is unset/reset between statements, the
SELECT returns zero rows and *every* valid capability is refused — a screen-wide 403 outage that
looks like a token bug. GUESS: no test in this tree asserts the GUC is set on this path.

**FM-3 — `token_hash` has no domain CHECK (CONFIRMED).**
The PK is a bare `bytea`. A system-path writer can store empty, truncated or oversized hashes.
Exact-byte matching prevents accidental collisions, but oversized keys bloat the PK index and a
degenerate empty key is a latent ambiguity. No generated/length guard exists in the migration.

**FM-4 — Generation is clamped in both directions (CONFIRMED code, LOW likelihood).**
The store clamps `u64 -> i32` with `unwrap_or(i32::MAX)` on write (`screen.rs:41`) and
`unwrap_or(u64::MAX)` on read (`screen.rs:84-85`). If a real generation ever exceeds `i32::MAX`,
it is written as `i32::MAX` and read back as a large `u64`; a much-later generation clamped the
same way compares equal and a stale capability is wrongly accepted. Reaching 2^31 boots is
implausible, so this is a monitoring item, not a v0.4.0 fix (widening is not additive).

**FM-5 — Unbounded growth, no expiry or revocation (CONFIRMED absence).**
Each `issue` is a fresh INSERT keyed by a new hash (`screen.rs:45-55`); there is no `expires_at`,
no `revoked_at`, no per-computer cardin cap and no index on `created_at`. A long-lived running
computer accumulates capabilities for the life of the deployment. Resolution is a fast PK lookup,
but storage, backup and vacuum cost grow without bound. Generation/status invalidation
(`usecases.rs:6328-6331`) retires capabilities logically, never physically.

**FM-6 — Two clocks, no DB default (CONFIRMED).**
`created_at` is `NOT NULL` with no default; the store binds `clock.now()` as text
(`screen.rs:42,54`), distinct from DB `now()`. Any future TTL/sweep ordered on `created_at` can
skew across replicas. Fresh migration starts empty, so a default is safe today.

**FM-7 — DDL availability (GUESS).**
A future `ALTER TABLE` / `ADD CONSTRAINT` takes ACCESS EXCLUSIVE with no `lock_timeout` and no
`IF NOT EXISTS`; a long resolution query blocks the deploy. Low frequency, broad blast radius.

## Concrete hardening (additive, with risks)

1. **Tie the tenant to the computer (FM-1).** `ALTER TABLE computers ADD CONSTRAINT
   computers_id_space_key UNIQUE (id, space_id); ALTER TABLE screen_capabilities ADD CONSTRAINT
   screen_capabilities_computer_space_fkey FOREIGN KEY (computer_id, space_id) REFERENCES computers
   (id, space_id) ON DELETE CASCADE;` *Risk:* existing mismatched rows would violate the FK —
   verify `COUNT(*) WHERE ... <> computers.space_id = 0` first. The unique constraint is additive
   and safe (`id` is already PK).
2. **Bound the hash (FM-3).** `ADD CONSTRAINT screen_capabilities_token_hash_len CHECK
   (octet_length(token_hash) BETWEEN 16 AND 64)` — accepts SHA-256 (32) with digest headroom.
   *Risk:* a strict `= 32` is cleaner but pins the algorithm; range avoids an ADR on hash rotation.
3. **Fail closed on the read path (FM-2).** Keep `FORCE RLS`, but add a regression test asserting
   `as_system` sets the GUC and that a system read with the reason unset returns zero rows (already
   fail-closed — prove it, do not assume it). Optional additive `COMMENT` on the policy stating the
   read path deliberately bypasses tenant scope; a `SECURITY DEFINER` resolver with hardened
   `search_path` is the durable fix but changes the access path → separate ADR.
4. **DB clock (FM-6).** `ALTER TABLE screen_capabilities ALTER COLUMN created_at SET DEFAULT now();`
   Additive, no effect on existing rows or clients that still supply a value. *Risk:* keep one clock
   — do not mix app and DB values in the same ordering column.
5. **Enable reclamation (FM-5).** Add nullable `expires_at timestamptz` and `revoked_at
   timestamptz` plus indexes on `expires_at` and `(computer_id, screen_generation)` for a bounded
   sweep. *Risk:* enforcing TTL/revocation in `resolve_screen_capability` is externally visible
   (a previously valid capability dies) → ADR + `docs/PARITY.md` + operator sign-off. Ship schema
   now, policy later.
6. **Bound the DDL (FM-7).** `SET LOCAL lock_timeout = '5s';` and `IF NOT EXISTS` in the companion
   migration. *Risk:* a busy deploy fails fast and must be retried; `IF NOT EXISTS` can hide a
   wrong-type column — verify no drift first.
7. **Monitor generation (FM-4).** Alert as `computers.screen_generation` approaches `i32::MAX`.
   Widening to `bigint` is a type change, not additive; deferred to its own ADR.
8. **Tests first, REQ-030 in every name (suggested).** `req_030_capability_space_matches_computer_space`,
   `req_030_rejects_out_of_range_token_hash`, `req_030_system_read_without_reason_fails_closed`,
   `req_030_created_at_defaults_to_db_clock`, `req_030_expired_capability_does_not_resolve`,
   plus the existing `conformance_a_stale_screen_capability_generation_is_refused`
   (`computer_conformance.rs:154`) as the regression gate.

## Decisions needed (GUESS → operator/spec)

1. Is the digest fixed to SHA-256 (32 bytes), or should the length CHECK stay a range (item 2)?
2. Are multiple capabilities per `(computer, generation)` legitimate? If not, a uniqueness target
   exists; if yes, TTL + sweep is the only bound.
3. Is a capability TTL acceptable (DEVIATION, item 5)? Proposed default ~15 min, nullable for
   existing rows.
4. Should resolution move to a `SECURITY DEFINER` function so the read path stops depending on a
   session GUC (item 3)?
5. Should generation widen to `bigint` in a follow-up ADR (FM-4)?

## Recommended order for v0.4.0

Ship 1, 2, 4, 6, 8 together — additive, no wire change. Treat 3 (resolver design), 5 (TTL/expiry
enforcement) and FM-4 widening as ADR-gated DEVIATION candidates. Keep the golden and contract
tests as the regression gate throughout.
