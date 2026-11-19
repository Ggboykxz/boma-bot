# Wave 22 / Worker 0495 — `migrations/0042_messaging.sql` (architecture) — v0.4.0 proposal

Additive only: every item below adds a NEW numbered migration (or an object inside one); nothing in
0042 is edited, dropped, re-typed or destructively back-filled — 0042 sits in the applied sqlx hash
chain. This report is the only file written; no product code changed, no tests run.
CONFIRMED = read from source this session; GUESS = inference to verify. Scope: the DDL of
`0042_messaging.sql` (78 lines), read alongside `0002_m1.sql:141-166`, `0031_mcp_credentials.sql`,
`docs/adr/011-channels.md`, `spec/05-known-defects.md` (D-05, D-09), and sibling proposals 0465
(0026 space keys, architecture) and 0360 (`stores/messaging.rs`, reliability), so this stays on the
DDL side and does not duplicate their Rust-side items.

## 1. Current structure (CONFIRMED)

- Three blocks: (a) `ALTER TABLE threads` adds `kind` (`bot|external`, NOT NULL DEFAULT `bot`) plus
  nullable `external_platform`/`external_workspace_id`/`external_conversation_id`, and a partial
  unique index `threads_external_identity` on those three columns `WHERE kind='external'`;
  (b) `messaging_installations` (17 columns, sealed token/webhook pairs, `status active|removed`,
  `reply_url`/`allow_private`), a **total** unique index `(platform, workspace_id)`, an index
  `(space_id, bot_id)`, and inline ENABLE/FORCE RLS + tenant policy; (c) `messaging_install_states`
  (`state` PK, FKs, `expires_at`) with the same inline RLS pair.
- No `COMMENT ON` anywhere in the file (only leading `--` prose); no `IF NOT EXISTS`; no `search_path`
  pin — consistent with migrations running before serving.
- FKs cascade from both new tables to `spaces`, `bots`, `users` (`installed_by` and `user_id`), while
  the header promises a removed installation keeps thread history. That guarantee rests on there
  being **no** FK from `threads` back to an installation — there is none today.

## 2. Architecture assessment

- The three `external_*` columns are the whole external-thread contract, yet nothing enforces the
  all-or-nothing shape the partial index assumes; PostgreSQL treats NULLs as distinct in that index,
  so a half-filled row silently escapes deduplication.
- The deployment-wide unique index covers `removed` rows, making reinstall structurally impossible.
  Worker 0360's H2 cannot be fixed by merely adding a partial index while the total one still exists,
  and dropping it is forbidden by the additive rule.
- Credentials are untyped `bytea` with no version, length or pairing invariant. Unlike 0026, there is
  no discriminator for a sealed-format change, so two formats cannot coexist during a rolling rekey.
- `messaging_install_states` has a 10-minute TTL in prose only: expired rows are never pruned by DDL
  and `expires_at` is unindexed (0360 §2.3).
- The RLS policy text is the second verbatim copy of the 0002/0026 predicate; 0465 item 5 proposes a
  shared helper — 0042's copies should be folded into that later work, never edited in place.

## 3. Concrete upgrades for v0.4.0 (additive only)

**U1 — Make the contract legible in the catalog (metadata, do first).** New
`006N_messaging_comments.sql`: `COMMENT ON TABLE`/`COMMENT ON COLUMN` for both tables and the four
`threads` columns, stating the thread-kind rule, the external identity tuple, one installation per
workspace deployment-wide, reply-at-run-end with no retry queue (ADR-011), AES-256-GCM + space-DEK
sealing (REQ-046/ADR-020), and `state` single-use/10-minute TTL. Risk: **LOW** — pure metadata; only
real risk is wording drifting from REQ-048/ADR-011, so quote ids verbatim and share text with 0465.

**U2 — Enforce the identity invariant and record ownership.** New `006O_messaging_thread_identity.sql`:
`ALTER TABLE threads ADD COLUMN messaging_installation_id text REFERENCES messaging_installations(id)
ON DELETE SET NULL` (nullable, no backfill here) plus an index, and a `CHECK` added `NOT VALID`:
`(kind='external' AND all three external_* IS NOT NULL) OR (kind='bot' AND all three IS NULL)`,
validated only after an audit query. Risk: **MEDIUM-HIGH** — a partial row fails validation (audit
first); a plain FK permits a cross-tenant link, so prefer a composite FK on `(space_id, id)`; binding
an id changes dedupe semantics, so land the column unused and route any behavior change through
`docs/PARITY.md`. This is the DDL complement of 0360 H3.

**U3 — Credential shape and version invariants.** New `006P_messaging_credential_invariants.sql`:
`credential_version smallint NOT NULL DEFAULT 0`, `CHECK (octet_length(token_nonce)=12)`,
`CHECK ((webhook_ciphertext IS NULL) = (webhook_nonce IS NULL))`, `CHECK (reply_url IS NULL OR
reply_url ~ '^https://')`, each `NOT VALID` then validated after audit. Risk: **MEDIUM** — GUESS the
12-byte nonce; confirm `SecretBox` output before pinning it; legacy `http://` or unpaired rows must be
documented, never deleted; pick a version that cannot collide with 0465's `space_keys.key_version`.
D-05's private-host allowance stays app-layer + audit, not a schema decision.

**U4 — Reinstall without breaking the unique index.** New `006Q_messaging_install_history.sql`:
`messaging_installation_reinstalls (id PK, installation_id FK ON DELETE CASCADE, platform,
workspace_id, activated_at, reason text)` plus a non-unique index `(platform, workspace_id, status)`.
The store reactivates a `removed` row in place with rotated credentials (same row ⇒ the existing
unique index stays satisfied); history records activations, and `COMMENT ON POLICY` states that the
`is_system()` bypass is for ingress only (D-09). Risk: **MEDIUM** — needs a store change (0360 H2)
and a PARITY check; a cascade delete bypasses `status='removed'` and this table without a trigger
(**GUESS**, verify by test). Do **not** drop or replace the existing unique index: destructive and
hash-chain breaking.

**U5 — Bounded install-state at the schema level.** New `006R_messaging_state_cleanup.sql`:
`CREATE INDEX messaging_install_states_expires_at ON messaging_install_states (expires_at)`,
`CHECK (expires_at > created_at) NOT VALID` then validate, and a comment recording the TTL and the
cleanup-job obligation. Risk: **LOW-MEDIUM** — DDL alone never deletes rows; the index only makes a
periodic `DELETE ... WHERE expires_at < now()` cheap (0360 H5), and that job is an app change; audit
existing rows before validation.

## 4. Ordering, invariants, non-goals

U1 → U2/U3 (audit-gated) → U4 (PARITY-gated) → U5, then fold both inline RLS blocks into 0465's shared
helper in a later migration. Invariants: 0042 stays byte-for-byte unchanged; no DROP/RENAME/ALTER
TYPE/DELETE; every new file applies to a fresh DB and a 0042-seeded copy with identical credential
bytes, row counts and thread identities; `.sqlx` offline metadata refreshed. Non-goals: no new
platform, no retry queue, no wire/API change (that is a DEVIATION needing PARITY + owner approval).

## 5. Verification before merge

- Apply each new migration to a fresh DB and a 0042-seeded copy; assert unchanged rows/bytes and
  idempotent `boma migrate`.
- Record U2/U3 audit counts in `docs/STATE.md`; confirm the nonce-size and helper-name GUESSes.
- Re-run the REQ-048 messaging/ingress goldens and integration tests plus 0360's `req_048_*` list;
  add a cross-tenant FK rejection test for U2 and a duplicate-state test for U5.
- Reconfirm the RLS enumeration gate still sees `messaging_installations` and `messaging_install_states`.

Line budget: <120. Status: proposal only — READ-ONLY worker; no file outside this report modified.
