# 0523 — `migrations/0056_notifications.sql` — architecture (v0.4.0)

- Worker #523 · Lens: architecture · Target: v0.4.0 · Read-only review; no code was changed.
- Module: `migrations/0056_notifications.sql` (72 lines) at `before-m5` (v0.3.0-rc5).
- Read for context (CONFIRMED by direct reads): `migrations/0057_notification_destinations.sql`,
  `migrations/0058_notification_destination_flags.sql`, `migrations/0002_m1.sql`,
  `crates/infra/src/stores/notifications.rs`, `crates/app/src/push.rs`,
  `crates/domain/src/notification.rs`, `crates/infra/tests/notifications.rs`,
  `spec/01-requirements.md` REQ-064/065, `spec/02-data-model.md:160-168`, `AGENTS.md`,
  and wave-22 `0483-migrations-0036-artifacts-sql-architecture.md` (house format).

## Current structure (CONFIRMED)

Three tables; later migrations only add columns to one of them (`0058`).
`notification_subscriptions`: `id text PK`; `space_id`/`user_id` → CASCADE; `endpoint`, `p256dh`,
`auth_secret` as plaintext `text` with a "not blank" CHECK; nullable `user_agent`; `fail_count int
DEFAULT 0`; `created_at`/`updated_at`; unique `(space_id, user_id, endpoint)` and
`(space_id, user_id)` indexes. `notification_policies`: PK `(space_id, user_id, bot_id)`,
`notify_waiting DEFAULT true`, `notify_completed DEFAULT false`, `daily_cap int DEFAULT 20 CHECK
(>= 0)`, timestamps (`0058` adds two destination flags). `notification_log`: `id text PK`,
`space_id`/`user_id`/`bot_id` → CASCADE, `kind CHECK IN ('waiting','completed')`, `sent_at`, and one
index `(space_id, user_id, bot_id, sent_at)`. All three have RLS enabled **and forced** with one
identical policy, `space_id = boma.current_space_id() OR boma.is_system()`.

Store shapes: `count_today` is a `COUNT(*)` with equality on the first three keys plus a UTC-day
range on `sent_at` (the existing index covers it); `note_failure` increments `fail_count` and, when
`permanent && count >= 3`, deletes the row — the "3 strikes" rule is hard-coded in
`push.rs:340-363`, not in the schema; `note_success` zeroes it. Destinations draw from the same
`count_today` budget (`push.rs:165-196`), so a duplicate log row steals capacity from every channel.
`updated_at` has `DEFAULT now()` but no trigger; the store sets it by hand.
`notification_subscriptions_by_owner (space_id, user_id)` is a strict left prefix of the unique
`(space_id, user_id, endpoint)` index, hence redundant for owner lookups.

## Guiding principle for v0.4.0

Additive only: new columns, indexes, STABLE functions, views, side tables and their own RLS
policies. No dropped/renamed column, no type change, no replaced policy, no ORDER BY or wire-shape
change. Preserve all three table names, the PKs, the existing unique/count indexes, the exact RLS
predicate, and the plaintext subscription columns (any visible change is a DEVIATION per AGENTS.md).
Claims about plan shape or data volume are GUESS (no EXPLAIN or stats were run in this read-only
review).

## Concrete upgrades (5), each with a risk

1. **Bound `notification_log` growth.** The log is append-only with no retention path even though
   the cap only needs today's rows. Add a `(sent_at)` index — BRIN is attractive for timestamp-order
   appends — plus a `notification_retention (space_id PK → spaces, log_days int DEFAULT 30)` table
   with RLS copied verbatim from the tenant policy, read by a pruning worker. *Risk:* the index is
   inert without that worker, and BRIN loses effectiveness if rows arrive out of order (GUESS).
   *Mitigation:* `EXPLAIN (ANALYZE, BUFFERS)` a prune query before shipping the index; land the
   worker in the same release or leave the table documented-but-unused; keep `DROP INDEX` trivial.

2. **Give the shared cap an idempotency key.** Add nullable `event_id text` to `notification_log`
   and a partial unique index `(space_id, user_id, bot_id, event_id) WHERE event_id IS NOT NULL`, so
   a retried send cannot spend the cap twice or double-count toward the destination budget. *Risk:*
   it only works if the app generates and binds a stable event id; without it duplicates remain,
   and making the column `NOT NULL` later is non-additive. *Mitigation:* add the field to
   `NotificationLog`/store API and the wire contract in the same release, generate the id from the
   triggering run+event (not from time), and gate with a double-insert test under REQ-064.

3. **Make the dead-subscription rule explicit and bounded.** Add `CHECK (fail_count >= 0) NOT VALID`
   + `VALIDATE` after an audit `SELECT`, and a `boma.notification_failure_outcome(fail_count int,
   permanent bool)` STABLE SQL helper that encodes "permanent AND count >= 3 → delete" in one place,
   called by `note_failure`. *Risk:* two copies of the threshold (helper + app) drift; moving it
   wholly into SQL changes the store query and is cross-module. *Mitigation:* make the helper the
   single source the store calls for the decision (the DELETE stays in the store transaction), pin
   `3` in `spec/01-requirements.md` REQ-064, and cover it with the existing fail-count tests.

4. **Seal the subscription keys additively.** `p256dh`/`auth_secret` are plaintext `text`; add a
   `notification_subscription_secrets (subscription_id PK → subscriptions ON DELETE CASCADE,
   ciphertext bytea, nonce bytea, created_at)` side table with cloned RLS, plus a dual-write path so
   new subscriptions get a sealed copy. *Risk:* this is defense-in-depth only — plaintext columns
   stay until a coordinated non-additive drop (DEVIATION), and dual writes can diverge. *Mitigation:*
   weigh against AGENTS.md criteria (simplicity/maturity) first; if adopted, seal with the existing
   per-space AES-GCM scheme, verify ciphertext before reads, and keep the plaintext columns frozen
   until an owner-approved follow-up. No new dependency.

5. **Stop policy/index duplication without touching live objects.** Add `boma.can_access_space(text)
   RETURNS boolean STABLE` (SECURITY INVOKER) and use it only in NEW objects (upsert helper view
   `notification_health` over the three tables, plus upgrades 1 and 4), and freeze
   `notification_subscriptions_by_owner` as documented redundancy rather than dropping it.
   *Risk:* existing policies keep the duplicated literal, so drift is merely contained, not fixed;
   a full consolidation replaces policies (DEVIATION). A SECURITY DEFINER helper would bypass tenant
   isolation — never do that. *Mitigation:* keep the helper STABLE/INVOKER, test it under both
   member and `boma.is_system()` contexts, and leave the redundant index but add no further
   left-prefix duplicates.

## Cross-cutting risks / sequencing

- Sequence 1 → 2 → 3 → 4 → 5: indexes/retention first, cap protection and failure rule next,
  secrets and shared predicate last. Every step is individually revertable and leaves the public
  surface untouched.
- Freeze while doing this: column names, all PKs/unique/index definitions, the RLS predicate, and
  the exact strings in `notifications.rs` count/failure paths.
- `boma.is_system()` is a tenant-wide read/write bypass on all three tables by design; additive
  views must inherit that asymmetry, not widen it. FORCE RLS means the owner is subject to policy.
- Cascades from `spaces`/`users`/`bots` erase audit-ish log rows on delete; changing them is a
  behavior change, so it is recorded as a future DEVIATION, not a v0.4.0 fix.
- Traceability: no new REQ id (context is REQ-064/065); `tools/trace_check.py` is unaffected.
  Gate with `cargo fmt --check`, `clippy -D warnings`, `crates/infra/tests/notifications.rs`,
  REQ-064 daily-cap/fail-count tests, and goldens. No new dependency.

## Out of scope for this module

Backfilling or pruning existing rows, dropping the redundant index or plaintext columns, changing
`ON DELETE` actions, converting `kind` to an enum, adding a DB `updated_at` trigger (it would
override app-supplied values), the VAPID sender (`push.rs`), the wire/RPC DTOs, and destination
tables (`0057`/`0058`). Those belong to other workers; this proposal only strengthens the
notification schema additively.
