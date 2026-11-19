# 0525 — `migrations/0057_notification_destinations.sql` — architecture (v0.4.0)

- Worker #525 · Lens: architecture · Target: v0.4.0 · Read-only review; no code was changed.
- Module: `migrations/0057_notification_destinations.sql` (25 lines) at `before-m5`
  (v0.3.0-rc5). Additive-only scope for every proposal below.
- Read for context (CONFIRMED by direct reads): `migrations/0025_bot_secrets.sql`,
  `migrations/0026_space_keys.sql`, `migrations/0037_pages.sql` and `0042_messaging.sql`,
  `migrations/0056_notifications.sql`, `migrations/0058_notification_destination_flags.sql`,
  `crates/infra/src/stores/destinations.rs`, `crates/infra/src/secret_box.rs`,
  `docs/adr/028-notification-destinations.md`, `spec/01-requirements.md:610-618`,
  `docs/superpowers/plans/2026-11-15-m15.2-notification-destinations.md:100-131`, and wave-15
  `0341-crates-infra-src-stores-destinations-rs-architecture.md`.

## Current structure (CONFIRMED)

0057 is one additive `CREATE TABLE` plus one index, RLS enable/force, and a single `FOR ALL`
policy. No pre-existing object is touched. Columns: `id text PRIMARY KEY` (no default; the
domain supplies it), `space_id` with `ON DELETE CASCADE`, a closed `kind` CHECK, `label` with
only a `btrim <> ''` CHECK (named `notification_destinations_label_not_empty`), `ciphertext`
and `nonce` as unchecked `bytea`, `created_by text NOT NULL` with **no FK**, `created_at` with
a `now()` default. Index `(space_id, kind)`; RLS predicate
`space_id = boma.current_space_id() OR boma.is_system()` for both USING and WITH CHECK —
byte-identical to 0056's three tables.

Structural observations:
- The 2-line header names REQ-065 but, unlike 0025/0026, documents neither the AEAD scheme,
  the wrapping key, nor the "never returned by a read path" contract; there is no
  `COMMENT ON TABLE/COLUMN` anywhere.
- The `kind` CHECK is inline/unnamed while the label CHECK is named — inconsistent constraint
  naming, and no `COMMENT` binds the closed value set to `NotificationDestinationKind`.
- `nonce`/`ciphertext` have no shape checks although `SecretBox` always emits a 12-byte nonce
  and a ciphertext with a 16-byte GCM tag (`secret_box.rs:92-100`).
- `created_by` is the only `created_by`/`user_id` column in this family without an FK:
  0037 `pages.created_by`, 0042 `messaging.installed_by` and 0014 `routines.user_id` all
  use `REFERENCES users (id) ON DELETE CASCADE`.
- The `(space_id, kind)` index serves `list()`'s WHERE but not its `ORDER BY created_at DESC`
  (`destinations.rs:148-163`); no `updated_at` exists, so rotation is delete+create only.

## Guiding principle for v0.4.0

Additive only: every change ships in a NEW migration using `COMMENT`, `ADD COLUMN ... DEFAULT`,
`ADD CONSTRAINT ... NOT VALID` + `VALIDATE CONSTRAINT`, or `CREATE INDEX`. Never edit 0057,
drop/rename a column, change a type, replace the RLS policy, backfill existing rows, or alter
the closed `kind` values. Freeze the table/column names, the RLS predicate, the wire view and
the `destination_slack_webhook`/`destination_teams_webhook` flags (0058). A visible change is
a DEVIATION for `docs/PARITY.md` (AGENTS.md). GUESS marks inference.

## Concrete upgrades (5), each with a risk

1. **Make the schema self-documenting.** New migration with `COMMENT ON TABLE
   notification_destinations`, `COMMENT ON COLUMN ...ciphertext/nonce/kind/label/created_by`,
   and `COMMENT ON CONSTRAINT notification_destinations_label_not_empty`. Record: REQ-065 /
   ADR-028; AES-256-GCM under the space DEK from `space_keys`; the webhook is never returned
   by RPC, Debug, logs or exports; `kind` mirrors `NotificationDestinationKind`; space-wide
   blast radius means Owner/Admin-only management.
   - *Risk:* near-zero (comments are inert). GUESS: the inline kind constraint is named
     `notification_destinations_kind_check`; verify with `\d+ notification_destinations`
     before any `COMMENT ON CONSTRAINT`, or omit that one line. No test/wire change.

2. **Constrain the sealed material's shape.** Add `CONSTRAINT
   notification_destinations_seal_shape CHECK (octet_length(nonce) = 12 AND
   octet_length(ciphertext) >= 16) NOT VALID`, then `VALIDATE CONSTRAINT` in the same
   migration. This makes a corrupt/tampered blob fail at the boundary instead of surfacing as
   a decryption error during delivery.
   - *Risk:* a legacy row with a non-12-byte nonce or a too-short blob blocks `VALIDATE`.
     *Mitigation:* run a read-only `SELECT id FROM notification_destinations WHERE ...`
     audit first; GUESS no such rows exist because only `SecretBox` writes this table. Keep
     checks generous (do not pin an upper bound on ciphertext length).

3. **Give `created_by` referential integrity.** Add `FOREIGN KEY (created_by) REFERENCES
   users (id) ON DELETE ... NOT VALID` + `VALIDATE`, matching the 0037/0042/0014 family.
   - *Risk:* ON DELETE choice is a real semantic decision. `CASCADE` follows precedent but
     deleting one Owner deletes a space-wide channel for everyone (ADR-028 blast radius);
     `RESTRICT` blocks legitimate user deletion; `SET NULL` needs a NOT NULL drop (not
     additive). *Mitigation:* prefer `RESTRICT`, audit for orphan `created_by` values before
     `VALIDATE`, and record the chosen policy in `docs/STATE.md`; no backfill.

4. **Make registration idempotent / stop duplicate channels.** Add nullable
   `webhook_fingerprint bytea` holding an HMAC-SHA256 of the webhook under the space DEK, plus
   `UNIQUE (space_id, webhook_fingerprint)`. Cheaper alternative: plain `UNIQUE (space_id,
   kind, label)`.
   - *Risk:* a fingerprint is a deterministic derivative of a secret (new disclosure surface —
     needs an ADR), NULLs let all legacy rows bypass the unique index, and the column is dead
     weight until the store writes it. `UNIQUE (space_id, kind, label)` risks blocking
     `VALIDATE` on existing duplicates and forbids re-registering a rotated URL under the same
     label. *Mitigation:* land only with the REQ owner's approval and a matching store change;
     keep the existing `destinationsCreate` semantics unchanged until then.

5. **Lifecycle + query alignment.** Add `updated_at timestamptz NOT NULL DEFAULT now()` (an
   audit stamp and the seam for in-place rotation instead of delete+create) and a covering
   `CREATE INDEX notification_destinations_listing ON notification_destinations
   (space_id, kind, created_at DESC)` to match `list()`'s filter + order.
   - *Risk:* an unwired `updated_at` is misleading until a store `update`/`rotate` port lands;
     the extra index is write amplification for a small table (GUESS: negligible) and the
     `ORDER BY` win needs an `EXPLAIN ANALYZE` on a seeded DB and a store query change to be
     realized. *Mitigation:* keep both additive and inert, or defer #5 to the slice that adds
     rotation; no `sqlx prepare` needed unless the query text changes.

## Cross-cutting risks / sequencing

- Order 1 → 2/3 → 4 → 5: comments (free), then constraints needing a data audit, then the
  uniqueness column, then lifecycle/index. Each step is independently revertable and additive.
- RLS stays byte-identical; the seal is still opened only through `destination_webhook()`.
  Verify FORCE RLS still covers `space_keys` on the tenant connection (wave-15 U2 rekey risk).
- Gate: `cargo fmt --check`, `clippy -D warnings`, the `req_065_*` tests in
  `crates/infra/tests/notifications.rs`, opaque-webhook and cross-space RLS assertions, and a
  migration up/down on a seeded DB. No new dependency; no spec edit (context is REQ-065).

## Out of scope for this module

Editing 0057/0058, any backfill, type changes, replacing the RLS policy, changing the `kind`
value set or the sealed format, the store's kind conversion/error semantics (wave-15), and the
delivery/cap orchestration. Those belong to other workers; this proposal only strengthens the
0057 table structure additively.
