# Worker #463 — `migrations/0025_bot_secrets.sql` — LENS: architecture — v0.4.0 proposal

- **Status:** PROPOSAL (read-only review; only this file was written). CONFIRMED = verified by reading files in this worktree; GUESS = needs test/oracle/load evidence.
- **Scope:** the SQL shape of the `bot_secrets` table and its tenant-RLS boilerplate for REQ-046 / DEC-10 (ADR-020). Related evidence: `migrations/0026_space_keys.sql`, `migrations/0031_mcp_credentials.sql`, `docs/adr/020-secrets.md`, wave-14 worker #313 (`secret_box.rs`).
- **Constraint honored:** additive only. Recorded migrations are never edited (see the header of `0031_mcp_credentials.sql`), so every upgrade below is a *new, forward-only migration* plus/app-adjacent change — no `DROP`, no rewrite of 0025/0026.

## Summary (current structure, CONFIRMED)

0025 creates one table `bot_secrets(id, space_id, bot_id, name, ciphertext bytea, nonce bytea, created_at)` with two `ON DELETE CASCADE` FKs, a `name` regex CHECK (`^[A-Z][A-Z0-9_]{0,63}$`) and `UNIQUE(space_id, bot_id, name)`. It then runs a `DO $$ … FOREACH … IN ARRAY ARRAY['bot_secrets']` block that enables + forces RLS and creates `<table>_tenant_isolation` with `USING/WITH CHECK (space_id = boma.current_space_id() OR boma.is_system())`. Encryption is AES-256-GCM with the nonce stored beside the ciphertext; the value is write-only by application convention, not by database enforcement.

## Architectural findings

- **A1 (HIGH, CONFIRMED):** the RLS `DO`/`FOREACH`/`format()` block is a one-element loop copy-pasted verbatim into `0026_space_keys.sql` and other tenant migrations. It is boilerplate masquerading as abstraction; the isolation predicate is duplicated in N places, so any hardening of tenant isolation must be applied by hand everywhere or the tables drift apart.
- **A2 (HIGH, CONFIRMED):** the schema is not self-describing. `ciphertext` + `nonce` carry no `version`, `key_id`, `purpose` or AAD columns. Combined with the untyped `SealedSecret` of worker #313, there is no way to add domain separation or a new envelope format incrementally — today's only migration path is a big-bang rewrite, which an additive-only policy forbids.
- **A3 (HIGH, CONFIRMED):** "write-only, never returned to a person" is an application promise only. Any DB role with table `SELECT` can read `ciphertext`; there is no column-level grant, no names-only view, no `REVOKE`. The guarantee has no second line of defence at the storage layer where the data actually sits.
- **A4 (MED, CONFIRMED):** not re-runnable and no structural length checks. `CREATE TABLE`/`CREATE POLICY` are unguarded (no `IF NOT EXISTS` / `DROP POLICY IF EXISTS`), and `ciphertext`/`nonce` have no `octet_length` CHECK, so a wrong-size nonce or a truncated GCM tag is only caught at decrypt time. Repo convention runs each migration once, so this is latent rather than active.
- **A5 (MED, CONFIRMED / partly GUESS):** ADR-020 says 0025 creates *both* `space_keys` and `bot_secrets`, but the tables are split across 0025 and 0026 — the doc and the migration numbering disagree (CONFIRMED). Also, no `updated_at`/generation/rotation marker on a secret, so in-place rotation is delete+insert with no history (GUESS: whether spec needs history).

## Concrete upgrades (v0.4.0, all additive)

1. **P0 — Shared tenant-isolation helper for future migrations.** Add a new migration defining `boma.tenant_isolation(space_id text)` (STABLE) and a helper that applies `ENABLE`/`FORCE` RLS plus an idempotent `DROP POLICY IF EXISTS …; CREATE POLICY … tenant_isolation(space_id)`. New tenant tables (and, in a later sweep, replacements for the duplicated policies) call it; leave 0025/0026 bytes untouched. *Risks:* the predicate must preserve the `boma.is_system()` escape hatch and FORCE semantics exactly, or system migrations break; a `SECURITY DEFINER` helper needs a pinned `search_path`; centralizing the predicate means one bug affects every tenant table — cover with an RLS test over all tables.

2. **P0 — Self-describing envelope columns (nullable-first).** New migration adds nullable `format_version smallint DEFAULT 0`, `key_id bytea`, `purpose text` to `bot_secrets` (and `space_keys`), with a CHECK that version ∈ {0,1}; app starts writing v1 with AES-GCM AAD = `version ‖ key_id ‖ purpose ‖ space_id ‖ bot_id ‖ name`, reads v0 for legacy rows, then a *later* migration backfills and sets `NOT NULL`. *Risks:* a persisted data-format change → DEVIATION entry in `docs/PARITY.md` and owner approval before shipping (AGENTS.md); dual-read doubles the test matrix; wrong AAD on first write makes data unreadable — land test-first with a legacy round-trip golden (coordinated with worker #313).

3. **P1 — DB-enforced write-only view.** New migration creates `bot_secret_names` as a `security_invoker`/`security_barrier` view exposing only `id, space_id, bot_id, name, created_at`, and revokes column `SELECT(ciphertext, nonce)` from the app/list role; list queries switch to the view; the reveal path keeps narrowly granted access. *Risks:* the view must replicate RLS or it leaks cross-tenant names — verify `security_invoker` behaviour on the target Postgres version; column grants interact with FORCE RLS and the system role; audit that no list endpoint still selects ciphertext.

4. **P1 — Crypto-column integrity constraints (NOT VALID first).** New migration adds `CHECK (octet_length(nonce) = 12)`, `CHECK (octet_length(ciphertext) >= 16)` on `bot_secrets`, and the analogous checks on `space_keys.wrapped_key`/`nonce`, added `NOT VALID` then `VALIDATE` after an audit query confirms existing rows conform. *Risks:* 12-byte nonce / 16-byte tag assumptions must match `secret_box.rs` (worker #313 reports 12-byte nonces — CONFIRMED there); validating a table with a stray row fails the migration, so gate on the audit; GUESS if any legacy writer used a different nonce size.

5. **P2 — Rotation/lifecycle metadata.** New migration adds `updated_at timestamptz`, a `generation` counter and `rotated_at` to `bot_secrets`, so a secret can be replaced with a bumped generation and an auditable timestamp instead of a blind delete+insert. *Risks:* requires app write-path changes to maintain the columns; keep defaults so existing rows migrate cleanly; if an audit trail is added, ensure it records the name/generation but never the plaintext value.

## Cross-cutting risks / notes

- "Additive only" here means forward-only migrations: never edit 0025 (or 0026), and every schema change ships as a new numbered file — consistent with `0031_mcp_credentials.sql`.
- Upgrades 2–4 change the observable storage contract or access surface: log DEVIATION entries in `docs/PARITY.md` and wait for owner approval before shipping (AGENTS.md).
- No claim of passing tests is made; findings are from source reading in this worktree (CONFIRMED) except items explicitly marked GUESS. Confirm nonce/tag sizes and `security_invoker` support against the real DB and `secret_box.rs` before implementing.
