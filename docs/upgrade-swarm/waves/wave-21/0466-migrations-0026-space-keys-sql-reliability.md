# 0466 — `migrations/0026_space_keys.sql` — reliability (v0.4.0)

Worker #466 · module `migrations/0026_space_keys.sql` · lens: reliability · READ-ONLY review of the
`before-m5` tree. Proposals only; no code changed. **Additive only:** every proposal below is a new
statement in a NEW later migration (e.g. `0060_space_keys_hardening.sql`); `0026` is never edited,
so applied SQLx checksums stay valid. **CONFIRMED** = read this session; **GUESS** = needs a runtime
test. Anchors: `0026:7-13,15-30`; `secret_box.rs:88-102,105-115,137-151`;
`stores/secrets.rs:228-274,284-327`; `stores/destinations.rs:81,226`; `tenant.rs:107-143`;
`0002_m1.sql:27-42`; ADR-020; REQ-046 / DEC-10; wave-14 #0316 (same table, Rust lens).

## Ground truth (CONFIRMED)

- `0026:7-13` creates `space_keys(space_id text PK REFERENCES spaces(id) ON DELETE CASCADE,
  wrapped_key bytea NOT NULL, nonce bytea NOT NULL, created_at timestamptz NOT NULL DEFAULT now(),
  rotated_at timestamptz NULL)`. No length CHECK on either blob, no version/generation column,
  no CHECK on `rotated_at`, no index beyond the PK.
- `0026:15-30` ENABLE + FORCE RLS and ONE default-PERMISSIVE policy `space_keys_tenant_isolation`
  with `USING`/`WITH CHECK (space_id = boma.current_space_id() OR boma.is_system())`.
- `secret_box.rs`: GCM nonce is 12 bytes (`:92`), unwrap requires exactly 32 plaintext bytes
  (`:150`), `open_under` requires a 12-byte nonce (`:109-111`); `wrap_data_key` seals a 32-byte
  DEK, so a valid `wrapped_key` is 48 bytes (32 + 16-byte GCM tag — 48 is inference, GUESS until
  measured from a real `wrap_data_key` sample).
- `rekey_space_keys` (`secrets.rs:228-274`) selects `FOR UPDATE` under a system tenant, rewraps
  every row and sets `rotated_at = now()`; no read-back verification, no generation marker.
- DEK mint (`secrets.rs:284-327`) and destination seals (`destinations.rs:81,226`) call `pool`
  directly, not `with_tenant`; `is_system()` trusts any non-empty GUC (`0002_m1.sql:38-42`, D-09).
- sqlx embeds migrations forward-only and checksummed (`db.rs:20,72-74`); a partial migration
  cannot commit (one transaction), so re-application is only via the `_sqlx_migrations` ledger.

## What is already right (keep, do not re-propose)

- `ON DELETE CASCADE` ties key lifetime to the space; `NOT NULL` on both blobs; PK-only lookup is
  correct for one row per space. `boma.current_space_id()` is `current_setting(..., true)` → NULL →
  fail-closed with no tenant (CONFIRMED). The `format('%I', ...)` DO block is identifier-safe.
- RLS is the sole tenant guard (no other key-specific authorization); keep both helpers as the
  policy contract rather than inlining `current_setting` (ADR-002 CI grep).

## Failure modes

### F1 — Key storage: blobs are unvalidated at rest (CONFIRMED)
A truncated `nonce`, an empty/oversized `wrapped_key`, or a hand-seeded row is accepted by the DB
and only fails at unwrap/open time, where it collapses to generic `Unavailable` (#0316 F3). There is
no early, database-level detection of corrupt key material and no self-describing format version.

### F2 — RLS: tenant connections may replace a space's DEK (CONFIRMED)
The single PERMISSIVE policy grants a member-scoped connection SELECT **and** INSERT/UPDATE/DELETE
on its own `space_keys` row. DEK writes are exclusively system operations (mint at
`secrets.rs:284+`, rekey at `:228+`), so member write access is unnecessary: a buggy or injected
tenant path can overwrite `wrapped_key`/`nonce`, permanently bricking every secret in that space —
there is no copy of the old DEK and no undo. `is_system()` accepts any non-empty reason (D-09), and
superuser/`BYPASSRLS` bypass is unasserted. GUESS: the bare-pool mint paths (above) only work in
tests because the test role is a superuser; under a least-privilege production role they fail
closed — RLS coverage needs an explicit least-privilege assertion, not superuser tests.

### F3 — Rotation: no way to tell old wraps from new (CONFIRMED)
`rotated_at` is set but carries no master-key generation, so a deployment cannot do staged/dual
rotation, detect a row missed by the `FOR UPDATE` snapshot created after rekey started (#0316 F4),
or verify a partial rekey. Re-running `boma rekey` with the wrong `--old-key` fails mid-loop and,
without read-back verification, still reports a count; on restart a mixed wrap set is
indistinguishable from a bad master key. No CHECK that `rotated_at >= created_at`.

## Concrete additive hardening (all in a NEW migration; GUESS — not implemented)

- **P1 (F1):** `ADD CONSTRAINT ... CHECK (octet_length(nonce) = 12)` and `... CHECK
  (octet_length(wrapped_key) = 48)`, added `NOT VALID` then `VALIDATE CONSTRAINT` after auditing
  existing rows (no row locks on live traffic during the check). *Risks:* one legacy/hand-seeded row
  of a different length blocks VALIDATE; confirm the 48-byte constant from a real wrap first
  (test-only today) or the constraint rejects valid wraps — internal schema change, no wire change.
- **P2 (F1):** `COMMENT ON COLUMN` for `wrapped_key`/`nonce`/`rotated_at` recording the exact
  wrapping format and that the space id is the only DEK identity, so a future version bump has a
  documented place to land. *Risks:* none; comments are inert.
- **P3 (F1):** `CHECK (rotated_at IS NULL OR rotated_at >= created_at)` — cheap defense against a
  corrupt/impossible rekey timestamp. *Risks:* NTP skew on the rekey host could reject a rotation
  write; benign and loggable, document it.
- **P4 (F2):** add a **RESTRICTIVE** policy `space_keys_system_write` for INSERT/UPDATE/DELETE
  requiring `boma.is_system()`. PostgreSQL ANDs RESTRICTIVE with the existing PERMISSIVE policy, so
  member reads stay allowed and member DEK writes are denied **without editing 0026**. *Risks:*
  any legitimate tenant-scoped mint path breaks — audit all call sites first (the bare-pool paths
  above are GUESS); if none write under a member context, make SELECT system-only too. This is an
  internal authorization change; if no client path writes DEKs there is no wire DEVIATION, but
  confirm with a grep before shipping.
- **P5 (F2):** `REVOKE INSERT, UPDATE, DELETE ON space_keys FROM <client_role>` (privileges first,
  RLS backstop) and a CI/startup assertion that the pool role is neither superuser nor `BYPASSRLS`.
  *Risks:* ops/maintenance queries must then carry `boma.system_reason`; document the break-glass;
  a missed reason means fail-closed outage, never a leak.
- **P6 (F3):** add `key_generation integer NOT NULL DEFAULT 0` (additive column) and have
  `rekey_space_keys` increment it with `rotated_at` (Rust coordination). A generation tag enables
  staged rotation, detection of rows a rekey snapshot missed, and read-back verification. *Risks:*
  schema + Rust must ship together; default `0` keeps existing rows valid; needs an ADR/decision on
  whether dual-read is in scope — observable only via ops tooling, not the API.
- **P7 (F3):** document an ops audit query `SELECT space_id FROM space_keys WHERE rotated_at IS NULL
  OR rotated_at < $cutoff` and place post-rekey read-back verification in `secrets.rs` (#0316 P0),
  cross-referenced here. *Risks:* a timestamp cutoff is approximate under clock skew; treat it as a
  hint, not proof — P6 is the reliable signal.
- **P8 (tests):** REQ-tagged tests in a NEW test file: fresh migrate + re-run with unchanged
  `_sqlx_migrations` checksums; malformed nonce/ciphertext rejected by the new CHECKs; a tenant
  connection cannot UPDATE/DELETE a DEK (P4); rekey vs concurrent new-space creation leaves the new
  row readable (P6); CI check that every `space_id` table has ENABLE + FORCE + a policy. Never log
  key material, space ids or ciphertext. *Risks:* extra testcontainers time; P4 changes behavior
  any test pinning today's tenant-write permission.

## Verification (proposed; NOT run — read-only)

`cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test -p boma-infra`; `sqlx migrate run`
from empty then a re-run asserting checksums unchanged; a least-privilege-role test for P4/P5.
All claims are CONFIRMED by source reading only; no command, DB or test was executed in this pass.

## Constraints / parity

Never edit `0026` or any applied migration; additive DDL only. Keep fail-closed RLS and the
transaction-local tenant contract. No API/wire/data-format change; P4/P6 are internal, but record
any externally visible authorization change in `docs/PARITY.md` with approval before shipping.
No secrets, ids or PII in logs or docs. Open questions for `docs/STATE.md`: confirm the 48-byte
wrap constant, choose restrictive policy vs full system-only, and decide whether dual-read rotation
(P6) is in v0.4.0 scope.
