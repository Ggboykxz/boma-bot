# 0492 — `migrations/0040_bot_mail.sql` — reliability (v0.4.0)

Worker #492 · module `migrations/0040_bot_mail.sql` · lens: reliability · READ-ONLY review of
`rebuild-workspace/.worktrees/before-m5` (v0.3.0-era tree). Proposals only; no source changed.
Claims tagged CONFIRMED (read this session) or GUESS (inferred); no tests were run (read-only).
**Additive only:** every item adds a column, constraint, index, function, policy or comment in a
*new* forward-only migration; nothing in 0040 is edited, dropped or destructively rewritten.

## Current structure (CONFIRMED — read this session)

- `bot_mail_rules`: `bot_id text PK REFERENCES bots(id) ON DELETE CASCADE`, `space_id text NOT NULL
  REFERENCES spaces(id) ON DELETE CASCADE`, `policy text NOT NULL DEFAULT 'hold' CHECK IN
  ('hold','drop')`, `allowed_senders jsonb NOT NULL DEFAULT '[]'`, `updated_at ... DEFAULT now()`.
- `held_mail`: `id text PK`, `space_id`/`bot_id` NOT NULL with CASCADE FKs, `sender text NOT NULL`,
  `subject`/`body text NOT NULL DEFAULT ''`, `reason text NOT NULL`, `created_at ... DEFAULT now()`.
  Index `(space_id, bot_id, created_at DESC)`.
- Both tables `ENABLE` + `FORCE ROW LEVEL SECURITY` with the standard tenant policy
  `USING/WITH CHECK (space_id = boma.current_space_id() OR boma.is_system())` — matches 0030/0038/0042.
- Boundaries consulted: `bots(id, space_id, ...)` in `0002_m1.sql:141-153` (CONFIRMED); REQ-047
  "Inbound e-mail per bot (SHOULD)" in `spec/01-requirements.md:383-389`; `spec/TRACE.md:64`
  (relay bearer + 25 MiB cap in `email-inbound.ts`); `0051_lowercase_users_email.sql` (addresses
  compared byte-for-byte, lowercased at write); `0041_routines_email_senders.sql` (`email_senders
  text[]`, overlaps the allowlist).

## Failure modes

**Address validation.**
- `allowed_senders jsonb` has no shape check: a scalar, object, or array of non-strings is accepted
  (CONFIRMED), which can break a `Vec<String>` deserializer on read and silently disable verification.
- No case normalization. Accounts are lowercased byte-for-byte (0051), but allowlist entries and
  `held_mail.sender` are not, so `Alice@Example.com` vs `alice@example.com` fails verification —
  a verified owner is wrongly held/dropped (CONFIRMED from 0051; GUESS that relay preserves case).
- `sender`, `subject`, `body` are unbounded `text`. The 25 MiB cap is app-side only (TRACE.md:64);
  there is no DB backstop against a relay bug or a direct system-role write (CONFIRMED).
- `reason text NOT NULL` is free-form: no vocabulary, so "why held" cannot be grouped or audited.

**RLS.**
- The relay POSTs with a relay bearer, not a member session (TRACE.md:64). Such a request has no
  `current_space_id()`, so it can read the rule and insert a hold only if its DB context sets
  `boma.is_system()`. If that flag is unset, RLS fails closed and inbound mail is silently lost —
  a notification-free mail drop (CONFIRMED policy; GUESS on the relay's DB context).
- `boma.is_system()` is session state. With pooled connections, a failure to `RESET`/`SET LOCAL`
  could leak the system bypass into a later tenant request (GUESS — needs adapter verification).
- `FORCE RLS` applies to the owner too: constraint validation or backfill maintenance run without
  the system flag can skip or be blocked by violating rows (CONFIRMED pattern, cf. worker #456).

**Constraints / coherence.**
- `space_id` and `bot_id` are independent FKs. Nothing ties them to the same bot's space: a system
  path can write `space_id=A, bot_id=<bot in B>`, and RLS then lets A hold/read B's bot mail — a
  cross-tenant leak via mismatched FK pair (CONFIRMED schema; bots.space_id in 0002).
- `held_mail.id` is a bare text PK with no external message id / idempotency key. Relay webhooks
  retry on failure, so a redelivered message creates a second held row (or second run) (GUESS on
  relay retry behavior).
- No resolution state, `resolved_at`, `expires_at` or TTL: "held for the member to release or drop"
  has no in-DB lifecycle, so holds accumulate unbounded with no sweeper-friendly index (CONFIRMED).
- `updated_at` on `bot_mail_rules` has no trigger — only as good as the writer (CONFIRMED).

## Concrete hardening for v0.4.0 (additive, with risks)

1. **Allowlist shape + lowercase check.** Add immutable SQL fn `boma.is_email_allowlist(jsonb)`
   (`jsonb_typeof = 'array'`, every element `jsonb_typeof = 'string'` and `= lower(value)`), then
   named `CHECK (boma.is_email_allowlist(allowed_senders))`. Add `NOT VALID`, audit, then VALIDATE.
   - Risk: medium — mixed-case legacy entries block VALIDATE. Mitigation: additive normalization
     UPDATE of the allowlist values only, recorded in docs/STATE.md; never drop entries.
2. **Bounded text.** Add named CHECKs: `length(sender) BETWEEN 3 AND 320`, `sender = lower(sender)`,
   `length(subject) <= 998` (RFC 5322 line limit), `length(body) <= 26214400` (mirrors the 25 MiB
   relay cap), `length(reason) BETWEEN 1 AND 200`. Stage `NOT VALID` → audit → VALIDATE.
   - Risk: medium — a mixed-case or over-cap legacy row blocks validation; audit first, keep the
     body cap at or above the app cap, and record any offender as DEVIATION rather than truncating.
3. **Tenant-coherent composite FK.** Add `UNIQUE (id, space_id)` on `bots` (FK target), then on both
   tables add `(bot_id, space_id) REFERENCES bots(id, space_id) ON DELETE CASCADE`, keeping the old
   FKs. Backfill-proof: audit that every existing row's bot matches its space before VALIDATE.
   - Risk: medium — UNIQUE on a hot table must be built CONCURRENTLY; validation/maintenance needs
     the system/BYPASSRLS role because of FORCE RLS. Use one transaction with a rollback gate.
4. **Relay-path RLS regression test (no policy change).** Add a test that the inbound-mail path, with
   `is_system()` set via `SET LOCAL`, can read a rule and insert a hold under FORCE RLS, and that a
   non-system role with no space context is denied both. Assert session flags are reset after use.
   - Risk: high if the relay currently relies on the table-owner bypass — FORCE RLS will deny it;
     fix the connection context, never add a non-tenant SELECT/UPDATE carve-out to weaken RLS.
5. **Idempotency.** Add `external_message_id text` and partial `UNIQUE (space_id, bot_id,
   external_message_id) WHERE external_message_id IS NOT NULL`, populated from the relay message id.
   - Risk: low/medium — legacy rows stay NULL and are excluded; a duplicate provider id collides,
     which is the intended dedupe. Requires the relay to expose a stable id (GUESS).
6. **Retention / lifecycle support.** Add `expires_at timestamptz` (nullable), `resolved_at
   timestamptz`, `resolution text` (nullable), `updated_at timestamptz NOT NULL DEFAULT now()` on
   `held_mail`, plus `CREATE INDEX held_mail_expiry ON held_mail (expires_at) WHERE resolved_at IS
   NULL AND expires_at IS NOT NULL`. A sweeper that marks `resolved_at` is a separate, opt-in
   proposal — ship columns + index only in v0.4.0.
   - Risk: low for columns/index; behavior change if the sweeper or a resolution CHECK is enabled —
     needs ADR + docs/PARITY.md. Keep off by default to avoid a DEVIATION.
7. **Documented reason vocabulary.** `COMMENT ON COLUMN held_mail.reason` listing expected values
   (`unverified_sender`, `no_rule`, `invalid_address`, ...) and the `policy` semantics; optionally a
   named CHECK once the set is confirmed against the adapter.
   - Risk: low for the comment; a CHECK is GUESS and could block a legitimate new reason — defer.
8. **Lookup index (optional).** `CREATE INDEX CONCURRENTLY held_mail_sender ON held_mail
   (space_id, bot_id, sender)` if member/release flows look up holds by sender; the existing
   `(space_id, bot_id, created_at DESC)` index cannot serve that predicate (CONFIRMED).
   - Risk: low write cost; `CONCURRENTLY` cannot run inside the migration transaction — stage it.

## Additive-only guarantee

Items 1/2/3 add functions/constraints, 5/6 add columns, 6/8 add indexes, 4 adds a test, 7 a comment.
No `DROP`, no `ALTER COLUMN ... TYPE`, no RLS weakening, no destructive UPDATE of user data (item 1's
normalization is a narrow, logged data fix). Ship as new numbered files (e.g.
`0060_bot_mail_contract.sql`, `0061_bot_mail_tenant_fk.sql`, `0062_bot_mail_retention.sql`), never by
editing 0040, preserving the applied-migration hash chain.

## Verification before merge (owner to run; not run in this read-only pass)

- Apply to a fresh DB and a seeded copy; assert existing rows unchanged after each new migration.
- Run the audit queries from items 1/2/3; record offender/mismatch counts in docs/STATE.md.
- Exercise REQ-047: verified owner → run; unverified stranger → hold/drop per policy; relay with
  `is_system()` → rule read + hold insert succeeds under FORCE RLS; without it → denied.
- Replay the retry case for item 5 (two deliveries of one provider id → one held row).

Status: proposal only — no file outside this report was modified.
