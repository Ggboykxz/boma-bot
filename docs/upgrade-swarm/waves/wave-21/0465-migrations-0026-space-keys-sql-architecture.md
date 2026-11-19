# Wave 21 / Worker 0465 — `migrations/0026_space_keys.sql` (architecture) — v0.4.0 proposal

Additive only. Every change below *adds* a new numbered migration, column, constraint, function or
comment; nothing already applied is edited, dropped, re-typed or destructively back-filled. This
report is the only file written. Claims are **CONFIRMED** by direct reading this session unless
marked **GUESS**. Scope: the DDL structure of `0026_space_keys.sql`, read alongside
`migrations/0025_bot_secrets.sql`, `docs/adr/020-secrets.md`, `docs/STATE.md` (the 0025/0026
incident), and sibling worker 0315's architecture review of `crates/infra/src/space_keys.rs`.

## Current structure (CONFIRMED)

`0026_space_keys.sql` is 30 lines, forward-only (no `UP`/`DOWN`, matching the repo convention). It
does two things:

1. `CREATE TABLE space_keys` — `space_id text PRIMARY KEY REFERENCES spaces(id) ON DELETE CASCADE`,
   `wrapped_key bytea NOT NULL`, `nonce bytea NOT NULL`, `created_at timestamptz NOT NULL DEFAULT
   now()`, `rotated_at timestamptz` (nullable). No `IF NOT EXISTS`.
2. A `DO $$ … FOREACH tenant_table IN ARRAY ARRAY['space_keys'] LOOP` block that runs `ENABLE ROW
   LEVEL SECURITY`, `FORCE ROW LEVEL SECURITY`, and creates policy `<table>_tenant_isolation` with
   `USING`/`WITH CHECK (space_id = boma.current_space_id() OR boma.is_system())`.

The header cites REQ-046 / DEC-10 / `docs/adr/020-secrets.md`; that ADR is the real one (ADR-019 is
`testing.md`, so `0025_bot_secrets.sql`'s inline reference to `docs/adr/019-secrets.md` is a stale
cross-reference, not a second ADR). STATE.md confirms 0026 was split out of 0025 after 0025 had
already been recorded (`VersionMismatch(25)`); the file is therefore part of an applied hash chain
and must never be edited in place.

## Architectural assessment (CONFIRMED)

The table is the single source of truth for the per-space DEK envelope. Three structural gaps matter
for v0.4.0 and are all closeable additively:

- **The schema says nothing about the ciphertext format.** `wrapped_key` and `nonce` are untyped
  `bytea`: no length check, no version discriminator, no record of which wrapping key (KEK) or
  associated data produced the row. ADR-020 promises a future KMS KEK swap "without a schema
  change", but with one unversioned wrapped form per row there is no way to hold old and new rows
  simultaneously during a rolling rekey — the swap is really an offline all-or-nothing rewrap.
- **The RLS block is a single-element loop** copy-pasted from 0025. It works, but it reads as a
  shared tenant-RLS fixture when it protects exactly one table; the policy text is duplicated
  verbatim in a growing set of migrations. Drift in one copy silently changes isolation semantics.
- **Auditability is thin.** `rotated_at` is the only post-create signal, with no constraint tying it
  to `created_at`, no rekey counter, and no `COMMENT ON` recording that the plaintext DEK never
  persists and that rotation re-wraps only the DEK (the REQ-046 guarantee).

Worker 0315 covers the Rust-side duplication and error taxonomy; this proposal stays on the DDL.

## Concrete upgrades for v0.4.0 (additive only)

### 1. Schema-level ciphertext invariants via `CHECK … NOT VALID`
New `006N_space_key_constraints.sql`: `CHECK (octet_length(wrapped_key) = 48)` (32-byte DEK + 16-byte
GCM tag), `CHECK (octet_length(nonce) = 12)`, and `CHECK (rotated_at IS NULL OR rotated_at >=
created_at)`, each added `NOT VALID` then `VALIDATE CONSTRAINT` only after an audit query.
- **Risk: MEDIUM.** A legacy row with a different wrapped length (e.g. a pre-envelope import) would
  fail validation. Mitigation: audit first; if any offending rows exist, document them in
  `docs/STATE.md` and scope the check with a guard rather than deleting data. Do not assume 48/12
  without confirming the `SecretBox` output size — mark the constants **GUESS** until confirmed.

### 2. Wrapping-key version discriminator
New `006O_space_key_version.sql`: add `key_version smallint NOT NULL DEFAULT 0` (reader dispatch
on version). This is the DDL half of worker 0315's item 3; without it the AAD migration (0315 item 4)
cannot keep legacy rows readable during a rolling rekey.
- **Risk: MEDIUM-HIGH.** Every Rust `SELECT` of `(wrapped_key, nonce)` must add the column and
  rotation must bump it atomically; a missed caller reads version-0 bytes as version-1. Mitigation:
  land the shared row type consolidation (0315 item 1) first so `-D warnings` catches tuple
  mismatches; choose the version number to not collide with the AAD-format version.

### 3. Documented AAD context column
In the same new migration as item 2, add `aad_context text` (nullable, records the space-scoped AAD
scheme used to wrap the row, e.g. `'space_id:v1'`) so a reader can tell a legacy no-AAD row from an
AAD-bound one without trial decryption. This is metadata only; it does not bind the key by itself.
- **Risk: LOW-MEDIUM.** If `NULL` is left ambiguous (never set vs legacy), the migration is useless.
  Mitigation: add a `COMMENT ON COLUMN` defining `NULL` = legacy unwrapped-without-AAD, and have the
  writer populate it in the same transaction as the rewrap.

### 4. Contract comments and corrected ADR cross-reference
New `006P_space_key_comments.sql`: `COMMENT ON TABLE space_keys` and `COMMENT ON COLUMN` for every
column, stating the envelope contract (AES-256-GCM, 256-bit DEK, plaintext never persists, rotation
re-wraps DEKs only per ADR-020) and that 0025/0026 are split by recorded-hash policy (STATE.md).
Also add a corrective comment pointing to `docs/adr/020-secrets.md` so the stale `019-secrets.md`
string in 0025 is not propagated; do **not** edit 0025 itself.
- **Risk: LOW.** Pure metadata. Risk is wording drifting from REQ-046/ADR-020 later; mitigation is to
  quote the requirement and decision ids verbatim and keep the text identical wherever repeated.

### 5. Reusable tenant-RLS helper (new object, future migrations only)
New `006Q_tenant_rls_helper.sql`: `CREATE OR REPLACE FUNCTION boma.enable_tenant_rls(p_table text)`
that encapsulates `ENABLE`/`FORCE` plus the policy, **marked `IMMUTABLE`-appropriate, `SECURITY
DEFINER` with `SET search_path = pg_catalog, boma` and strict identifier quoting via `format('%I')`.
Future migrations call it; 0026 and 0025 are left untouched. Rationale: one isolation definition to
audit instead of a growing set of copy-pasted DO blocks.
- **Risk: MEDIUM-HIGH.** A `SECURITY DEFINER` helper that runs DDL is a privilege-escalation surface;
  a missing `search_path` pin or an unescaped identifier could let a caller target the wrong table.
  Mitigation: pin `search_path`, use `%I`, `REVOKE EXECUTE` from `PUBLIC`, and add a regression test
  that a non-owner cannot invoke it. **GUESS:** confirm the repo has no existing helper with this
  name before adding it.

## Additive-only guarantee and sequencing

Order 1 → 3/2 → 4 → 5: prove the format constants, then add version/AAD metadata so the format is
distinguishable, then document the contract, then (optionally) factor the RLS boilerplate. Items 1–4
add columns/constraints/metadata; item 5 adds a new function. No `DROP`, no `ALTER COLUMN … TYPE`, no
destructive `UPDATE`, no rewrite of 0025 or 0026 — preserving the applied-migration hash chain
(STATE.md's hard lesson). Every new numbered file must be applied to both a fresh DB and a copy
seeded with migrations through 0026.

## Verification before merge

- Apply all new migrations against a fresh DB and against a 0026-seeded copy; assert `space_keys`
  row counts and existing `wrapped_key`/`nonce` bytes are unchanged after each.
- Run the audit queries of item 1 and record counts in `docs/STATE.md`.
- Re-run `req_046_a_secret_round_trips_through_postgres_and_survives_a_rekey` and the rekey test
  after items 2/3; confirm mixed-version rows both decrypt.
- Reconfirm the **GUESS** items (wrapped length, helper name) and mark CONFIRMED or revise.
- Keep every claim marked per workspace rules; no secrets, no new external behavior, no edits
  outside this report.

Status: proposal only — no file outside this report was modified.
