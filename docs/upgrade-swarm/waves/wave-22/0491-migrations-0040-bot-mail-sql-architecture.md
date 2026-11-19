# v0.4.0 architecture proposal — `migrations/0040_bot_mail.sql` (worker #491, lens: architecture)

Status: proposal only (read-only review). **Additive only**: every item below is a NEW migration or NEW
objects; nothing in 0040 (or 0041/0042) is dropped, renamed, rewritten or backfilled destructively.
Evidence: CONFIRMED from `migrations/0040_bot_mail.sql`, `0025_bot_secrets.sql`, `0030_audit_events.sql`,
`0041_routines_email_senders.sql`, `spec/01-requirements.md` (REQ-047), `spec/openapi.yaml`
(`/api/v1/email/inbound`), `spec/TRACE.md`, and `crates/infra/src/stores/mail.rs`; GUESS where inferred.

## Current structure (CONFIRMED)

- Two tables. `bot_mail_rules`: `bot_id` PK + FK `bots(id)` CASCADE, `space_id` FK `spaces(id)` CASCADE,
  `policy text DEFAULT 'hold' CHECK IN ('hold','drop')`, `allowed_senders jsonb DEFAULT '[]'`,
  `updated_at DEFAULT now()`. `held_mail`: `id text PK` (no default), `space_id`, `bot_id`, `sender`,
  `subject`, `body`, `reason` (free text), `created_at DEFAULT now()`, and one index
  `(space_id, bot_id, created_at DESC)`.
- Both tables ENABLE + FORCE RLS with `<table>_tenant_isolation` using
  `space_id = boma.current_space_id() OR boma.is_system()`. Unlike 0025/0030, 0040 spells the policy
  out directly instead of the `DO $$ FOREACH` helper — a convention divergence, not a defect.
- The store (CONFIRMED: `crates/infra/src/stores/mail.rs`) reads the rule by `bot_id` only, upserts it,
  inserts held rows, lists `WHERE bot_id=$1 ORDER BY created_at DESC LIMIT 50` with a hard-coded cap,
  and `take_held` is a destructive `DELETE ... RETURNING`. `sender_is_member` lowercases the
  **candidate** (`lower(u.email) = $2`) but the stored allowlist entries are compared verbatim.
- `0041` adds `routines.email_senders text[]`; the spec deliberately overlaps it with the bot allowlist
  (address in bot rule → starts mail work; address only in routine → fires that routine). That
  arbitration lives in app code (GUESS: no DB object enforces it). The `boma+{bot_id}@domain` address is
  implicit — only the `to` query param is persisted in the API contract, never in a column.

### Structural tensions (CONFIRMED unless noted)

1. `space_id` is trusted from the writer; nothing proves it agrees with the referenced bot's space.
   `hold()` explicitly trusts the relay's `held.space_id` under a system context (mail.rs:111-117).
2. The allowlist is untyped `jsonb`, read with `as_array().filter_map(as_str)`; mixed case, duplicate
   entries or a non-string element silently narrow or expand who is trusted.
3. `held_mail.id` is caller-supplied with no uniqueness beyond the PK: a retried relay POST (503 in the
   API contract tells relays to retry) can enqueue duplicates. There is no retention/expiry — held mail
   lives forever until someone takes it. `reason` has no vocabulary.
4. `take_held` deletes before the run is created; a failure between the two loses the message, and there
   is no `released`/`dropped` state or audit record (REQ-045's `audit_events.kind` only allows
   `mcp.private_call`, so mail decisions cannot be logged there without rewriting that CHECK).
5. `subject`/`body` are unbounded raw text held at rest (25 MB relay cap is enforced at the HTTP edge).
   GUESS: acceptable as message content, but it is PII that account export/deletion must cover.

## Upgrade 1 — Bind each mail row to its bot's tenant (new `0060_bot_mail_tenant_fk.sql`)

Add `ALTER TABLE bots ADD CONSTRAINT bots_id_space_key UNIQUE (id, space_id);` then composite FKs:
`bot_mail_rules (bot_id, space_id) → bots (id, space_id)` and
`held_mail (bot_id, space_id) → bots (id, space_id)`, both `ON DELETE CASCADE`. This makes the RLS
tenant key structurally consistent with the referenced bot and closes the system-context drift hole
in tension 1, without changing any existing single-column FK.

- **Risk: MEDIUM.** Any existing row with a mismatched `space_id` blocks deployment (safe failure). Ship
  a read-only pre-flight `SELECT ... WHERE r.space_id <> b.space_id` first; GUESS: all rows agree because
  `set_rule` derives `space_id` from the member context. Reuse the pattern from the 0010 proposal.

## Upgrade 2 — Type and normalize the allowlist (new `0061_bot_mail_sender_normalization.sql`)

Add an additive `CHECK` that every JSON element is a lowercase, non-empty, unique string, e.g. via a small
immutable `boma.normalized_sender_list(jsonb)` helper used in the constraint; also add
`allowed_senders_normalized text[] GENERATED ALWAYS AS (boma.normalized_sender_list(allowed_senders))
STORED` for indexable reads. Do NOT rewrite `allowed_senders` or the Rust read path in v0.4.0 — the
column stays the source of truth and the generator is additive.

- **Risk: MEDIUM/HIGH.** Existing mixed-case or duplicated entries would block the CHECK (pre-flight
  query required). GUESS: the original stores addresses as typed by the member, so casing may vary —
  if so, normalize on the *write* side in `set_rule` first and ship the CHECK in a later wave.
  A generated column needs an IMMUTABLE function; verify `cargo sqlx prepare` still works with it.

## Upgrade 3 — Dedupe and expire held messages (new `0062_held_mail_lifecycle.sql`)

Add nullable `message_id text` (the RFC 5322 `Message-ID` the relay can forward) plus
`UNIQUE (space_id, message_id)`; add nullable `expires_at timestamptz` with a partial index on
`created_at WHERE expires_at IS NULL` and a CHECK `expires_at IS NULL OR expires_at > created_at`.
Columns are nullable so existing rows are untouched; NULL message_ids keep today's behavior (many
NULLs are allowed by the unique index), so a relay that cannot supply an id still works.

- **Risk: LOW/MEDIUM.** Dedupe only helps if the relay actually sends `message_id` (GUESS: confirm the
  relay/webhook payload first; otherwise hash sender+subject+created_at in the app). Retention needs a
  sweeper job — out of scope here; a column with no sweeper is harmless but document it in
  `docs/STATE.md`. The unique index adds one insert lookup; negligible for a hold queue.

## Upgrade 4 — Give held mail a state vocabulary and an audit trail (new `0063_bot_mail_decisions.sql`)

Two additive objects: (a) nullable `state text` on `held_mail` with `CHECK (state IN ('held','released',
'dropped'))` and a `reason_kind text` CHECK over the known decision reasons, both defaulting NULL so
current rows and `take_held` keep working; (b) a new append-only `mail_decisions` table
(`id`, `space_id`, `bot_id`, `actor_user_id`, `sender`, `action`, `metadata jsonb`, `created_at`) with the
standard FORCE RLS policy and a `(space_id, created_at DESC)` index. This makes release/drop auditable
per REQ-045 without touching the `audit_events.kind` CHECK.

- **Risk: LOW for schema, MEDIUM for semantics.** Switching `take_held` from `DELETE` to a state update
  is a behavior change (a DEVIATION needing an ADR and owner approval), so leave the store as-is in
  v0.4.0 and treat the new columns as internal until the app writes them. Record sender only in the
  audit table — never `subject`/`body` — to keep PII out of logs.

## Upgrade 5 — Publish the data contract in the catalog (new `0064_bot_mail_comments.sql`)

`COMMENT ON TABLE`/`COMMENT ON COLUMN` for both tables: the 'hold' safe default, the deliberate overlap
with `routines.email_senders` (0041), that `space_id` is the RLS tenant key with an intentional
`boma.is_system()` bypass for the relay, and that `held_mail` is a quarantine queue, not a message store.
Purely descriptive; makes the non-obvious rules discoverable to SQLx offline metadata users.

- **Risk: NONE at runtime.** Keep wording in English per project convention.

## Out of scope (would break "additive only" or need owner approval)

- Rewriting the inline RLS statements into a shared helper, or narrowing `boma.is_system()`.
- Changing `take_held` to a transactional release (fixes tension 4 properly, but alters observable
  behavior). Track as a separate proposal tied to an ADR.
- Encrypting `subject`/`body` at rest, adding a `boma+{bot_id}` address registry, or backfilling any
  existing rows.

## Verification gate (for the implementing agent)

1. Apply each new migration on an empty DB and on a seeded DB; re-run to check idempotence. 2. Pre-flight
   drift/case queries for U1/U2 return 0 rows. 3. `EXPLAIN` the `held_list` query before/after U3.
4. Replay the REQ-047 golden/tests in `crates/infra/tests/m1.rs` and `tenancy.rs`; add a store test for a
   duplicate `message_id` and a cross-tenant held row. 5. `cargo sqlx prepare --check` and the migration
   harness; record any externally visible change in `docs/PARITY.md` with an ADR.
