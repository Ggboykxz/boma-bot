# 0480 — migrations/0034_connector_installs.sql — reliability (v0.4.0)

Worker #480 · module `migrations/0034_connector_installs.sql` · lens: reliability · READ-ONLY review of
`rebuild-workspace/.worktrees/before-m5` at v0.3.0-era source (workspace says `0.3.0-rc5`). Proposal
only; no product code, schema or spec edited here. Line refs are against this worktree. Claims tagged
CONFIRMED (read this session) or GUESS (inferred); no tests were run (read-only). Boundaries consulted:
migrations 0025 (`bot_secrets`), 0026 (`space_keys`), 0031 (`mcp_credentials`), 0035
(`plugin_installs`), `spec/01-requirements.md` REQ-032 (L263–269), `spec/02-data-model.md` (L132,
L142), `docs/adr/020-secrets.md`, and wave-14 brief 0314 (`secret_box.rs` reliability).

## What it already does right (keep, don't re-propose)

- Composite PK `(space_id, connector_id)` makes a "once per space" install idempotent at the storage
  layer — a duplicate install cannot create two credential sets (L16) (CONFIRMED).
- `ON DELETE CASCADE` from `spaces` means a deleted space cannot leave orphaned sealed credentials
  behind (L8) (CONFIRMED).
- RLS `ENABLE` + `FORCE` and a single `ALL` policy with **both** `USING` and `WITH CHECK`:
  a tenant can neither read nor write another tenant's row (L19–23) (CONFIRMED).
- `status` CHECK limits the switch to `connected`/`revoked`; the header's contract (a revoked install
  contributes no tools) is enforceable by the listing read path (L6, L13) (CONFIRMED).

## Failure modes → concrete hardening (additive only)

### F1. Secret storage — no binding of ciphertext to its row

- `sealed_credential` has no AAD binding to `space_id`/`connector_id` (L11–12). GCM authenticates the
  bytes but not the row identity: if a write bug or a system-context bypass swaps two rows, each
  ciphertext still opens under its (per-space) DEK and a space can inherit another space's
  connector credential. This mirrors wave-14 F3/P8 for `secret_box.rs` (GUESS that no caller supplies
  AAD today, since the schema has nowhere to carry it). P1. Add a follow-up migration (never edit 0034)
  with a `credential_version smallint NOT NULL DEFAULT 0` and, on new writes, AAD =
  `space_id || connector_id`; reads try versioned AAD first, then legacy version 0 during a transition.
  Risk: format change on the credential path — an ADR, a re-encryption migration for existing rows
  and REQ-032/REQ-046 tests are required; no code here should silently change open semantics.
- Nonce/DEK hygiene is inherited from ADR-020, not local to this table: a 96-bit CSPRNG nonce has a
  birthday bound and `space_keys` carries no key fingerprint (migration 0026 L7–13; wave-14 F1/F2).
  P2. Add a proptest sealing N=10k connector credentials under one DEK and asserting distinct nonces;
  document the per-key seal budget. P3. Track the DEK fingerprint centrally (in a later `space_keys`
  migration), not in 0034. Risk: none for P2 (test only); P3 is a separate migration + ADR.

### F2. Secret storage — representable half-written credential states

- Both credential columns are nullable with no conditional CHECK (L11–12), and `auth_kind='api_key'`
  does not require them. A row with `auth_kind='api_key'` and NULL ciphertext/nonce, or ciphertext
  without a nonce, is legal; the failure surfaces only at reveal time as a decryption error, not at
  write time. `status='revoked'` also keeps the old credential bytes at rest, so a later re-enable
  would silently reuse a credential that was meant to be dead (CONFIRMED on the schema; GUESS on the
  store code's invariants). P4. Add a later `ALTER TABLE ... ADD CONSTRAINT` enforcing
  `(auth_kind='none' AND sealed_credential IS NULL AND sealed_credential_nonce IS NULL) OR
  (auth_kind='api_key' AND both are present)`; audit/backfill dirty rows before applying. P5. On
  revoke, null both credential columns in the same transaction that flips `status`, and require a
  fresh credential on re-install. Risk: P4 can fail on existing rows — make the audit a gate; P5
  changes re-install UX (re-auth required), which is likely correct but must be checked against
  REQ-032's "connector removed → tool refused" rule before calling it parity-neutral.

### F3. RLS — the system bypass is a global OR

- `USING (space_id = current_space_id() OR is_system())` and the same `WITH CHECK` (L22–23). If a
  system-reason call ever runs without a space scope, `is_system()` alone admits every row, including
  cross-space ones; the same pattern is used by 0025/0026/0035, so it is not local to 0034 (CONFIRMED).
  P6. Add REQ-032-named integration tests that (a) a member of space A cannot SELECT, UPDATE or DELETE
  a space B install, (b) an INSERT with another space's `space_id` is rejected by `WITH CHECK`, and
  (c) a system-context read requires an explicit space (or document precisely why it must not).
  Risk: test-only, additive; if (c) fails, fixing it means changing the shared helper, an ADR +
  cross-migration regression pass — do not special-case 0034.

### F4. RLS — correctness depends on the helper functions, not on this table

- `current_space_id()` returning NULL outside a tenant context makes the predicate NULL (deny), which
  is fail-closed (CONFIRMED by SQL semantics; GUESS on the function body, not read this session). The
  table has no independent owner/schema guard, so a role with `BYPASSRLS` or a `SECURITY DEFINER`
  writer is outside its protection. P7. In tests, connect as the application role (not a superuser)
  when proving tenant isolation; assert `relforcerowsecurity = true` and the policy's `WITH CHECK`
  are present in a migration-contract test. Risk: none — diagnostic/test-only. Note: no `GRANT`
  statements exist here (GUESS: privileges come from the shared role setup); do not add grants in 0034.

### F5. State — narrow status enum vs. the spec's connection lifecycle

- `status` allows only `connected`/`revoked` (L13), while `spec/02-data-model.md` L132 lists a
  Connection as `pending/connected/revoked/error`. An in-flight OAuth install (pending) or a failed
  one (error) cannot be represented, so callers are pushed to insert a `connected` row with NULL
  credentials — exactly the half-written state of F2 (CONFIRMED on the mismatch; GUESS on how the
  store currently models pending installs). P8. Add a later migration widening the CHECK to include
  `pending` and `error` (additive state values), plus a `last_error_code text` column if operators need
  a non-secret reason; keep the "revoked contributes no tools" rule, and decide explicitly whether
  `pending`/`error` contribute tools. Risk: widening the accepted state set is a **DEVIATION** if it
  changes any client-visible listing — record it in `docs/PARITY.md` with a REQ-032 test before code.
- `updated_at` has a DEFAULT but no trigger (L15); as in 0040/0042/0059 it is left to application
  code, so a direct system write can leave it stale (CONFIRMED; consistent with the rest of the
  schema). P9. Do not add a trigger here alone — if freshness ever matters, propose one trigger
  function for all `updated_at` tables in a single ADR. Risk: none now; a lone trigger would diverge
  from the established pattern.

## Explicit non-goals / rejected

- Never edit recorded migration 0034: every change above is a new, additive migration or a code/test
  change. Never weaken RLS, drop the PK, or widen `FORCE` off. No new feature outside REQ-032 —
  proposals beyond it go to `docs/STATE.md`. No upstream content enters (swarm charter).

## Rollout (v0.4.0) and cross-cutting risks

- Wave A (internal, cheap): P2 nonce proptest, P6/P7 tenant-isolation contract tests, P9 note-only.
  Wave B (gated): P4 conditional CHECK + P5 revoke-wipes-credential, ADR + REQ-032 tests; P8 status
  widening with `docs/PARITY.md`. Wave C (format, separate approval): P1 AAD/versioned envelope with a
  rehearsed re-encryption drill.
- R1. P1/P5 touch stored credential bytes: the acceptance gate is a backup + re-encrypt/re-auth drill,
  not a unit test; keep the master key and old envelope readable through the transition. R2. P5/P8 can
  change the install listing surface → contract test against `spec/openapi.yaml` and a DEVIATION entry.
  R3. P4's backfill must run before the constraint in the same additive migration window. R4. No log or
  error path in any proposal may print ciphertext, nonce, plaintext or key bytes (ADR-020).
