# 0498 — migrations/0043_models.sql — reliability (v0.4.0)

Worker #498 · module `migrations/0043_models.sql` · lens: reliability · READ-ONLY review of
`rebuild-workspace/.worktrees/before-m5` (v0.3.0-era tree). Proposals only; no code changed.
Language: English. **Additive only**: every proposal is a new migration or an app/test change;
none edits 0043 in place (sqlx checksum validation would break every existing DB).

## Ground truth (CONFIRMED by reading the tree this session)

- 0043 creates two tables. `model_credentials`: `space_id` FK→spaces ON DELETE CASCADE,
  `provider text NOT NULL`, `status text NOT NULL DEFAULT 'connected' CHECK (status IN ('connected'))`,
  `ciphertext bytea NOT NULL`, `nonce bytea NOT NULL`, timestamps, `PRIMARY KEY (space_id, provider)`.
  It gets `ENABLE` + `FORCE ROW LEVEL SECURITY` and the standard policy
  `space_id = boma.current_space_id() OR boma.is_system()`.
- `user_model_preferences`: `user_id` PK FK→users ON DELETE CASCADE, `model_key text NOT NULL`,
  `updated_at`. **No RLS, no policy** — deliberately global/keyed by person across spaces, like
  `users`/`organizations`/`sessions` in 0002 (those also carry a COMMENT; these do not).
- `boma.current_space_id()` / `boma.is_system()` exist from 0002; **no** `boma.current_user_id()`.
- Store (`crates/infra/src/stores/models.rs`): `put_credential` rejects `TenantContext::System`,
  seals under the space DEK from `space_keys` (0026), and `ON CONFLICT (space_id, provider) DO UPDATE`
  ciphertext/nonce/updated_at. `forget_credential` does `DELETE ... WHERE provider = $1` with **no
  System guard**. Preference read/write filter by the actor's `user_id` with `ON CONFLICT (user_id)`.
- App boundary validates provider (trim/lowercase, 1..64 in `models_disconnect`) and `model_key`
  (must be in the reachable set, `models_set_preference`) — the DB enforces neither.
- Test `crates/infra/tests/m1.rs:3190+` proves cross-space invisibility with Member contexts only.

## What it already does right (keep — do not re-propose)

- PKs are the dedup keys and the app's `ON CONFLICT` targets match them exactly.
- The single-value `status` CHECK implements "invalid is never stored" (REQ-052). Keep it.
- `ON DELETE CASCADE` on both FKs; correct lifecycle. No FK from `provider` to a catalogue row:
  a provider is a key, not a credential row (same reasoning as the 0006 reliability review).
- `model_credentials` RLS uses the standard policy with `boma.is_system()` as the sole escape.

## Failure modes → concrete hardening

### F1. Constraints: unbounded/free-form columns (CONFIRMED gaps)

`provider`, `model_key`, `ciphertext`, `nonce` have only `NOT NULL`. A direct write or future store
bug can persist a multi-megabyte provider/model_key (echoed into `list_credentials`, logs, UI), an
empty `ciphertext`, or a wrong-size `nonce` — the last two pass `NOT NULL` but make the credential
permanently undecryptable at read time with no alert. Nothing checks `updated_at >= created_at`.

- **P1.** New `0060_models_column_bounds.sql`: `ADD CONSTRAINT ... CHECK (length(provider) BETWEEN
  1 AND 64)`, same for `model_key` (128), and `CHECK (octet_length(ciphertext) > 0)`. Add each
  `NOT VALID`, then `VALIDATE CONSTRAINT` in a later statement/waited step (validation takes only
  `SHARE UPDATE EXCLUSIVE`, so writers are not blocked). App bounds already guarantee current rows.
- **P2.** `CHECK (updated_at >= created_at)` (NOT VALID → VALIDATE). Guards app/DB clock inversion.
- **P3.** GUESS: `CHECK (octet_length(nonce) = 12)` if the GCM nonce is fixed 12 bytes. Confirm
  `SecretBox::seal_with_data_key` first (the test only asserts "non-empty"); a wrong constant
  rejects every write — ship P1–P2 alone until confirmed.
- **P4.** `COMMENT ON COLUMN` for both tables restating the REQ-052/REQ-015 contract (status is
  append-only-connected, preference is per person, NULL semantics), the on-call reliability doc.

### F2. RLS: correct today, fragile under global writes (CONFIRMED + 1 cross-cutting gap)

- **F2a.** Under `FORCE RLS`, any backfill/repair `UPDATE model_credentials` without a system reason
  touches **zero rows and reports success**, silently leaving half-migrated data.
  **P5.** Add a REQ-052 + REQ-043 regression test: a tenant connection cannot read/write another
  space's credential, and a `boma.system_reason`-scoped update does. Prove, do not assume.
  **P6.** Runbook note: credential backfills must run via `tenant::as_system`. Keep
  `boma.is_system()` as the only policy escape; never inline `current_setting`.
- **F2b.** `user_model_preferences` has **no RLS and no `boma.current_user_id()`**; isolation rests
  entirely on every store method filtering by the actor's `user_id`. A future method that omits the
  filter reads/writes anyone's preference. **P7.** Add `COMMENT ON TABLE` mirroring 0002's global-table
  comments ("application must filter by user_id") plus a test that a member context only reads/writes
  its own row. **P8 (GUESS, defer).** True per-user RLS needs a new `boma.current_user_id()` helper
  reading a `boma.user_id` setting set on every connection, plus a policy
  `USING (user_id = boma.current_user_id() OR boma.is_system())` — cross-cutting connection change,
  owner approval + DEVIATION review required; do not bundle with this wave.
- **F2c (cross-cutting, high priority).** `forget_credential` lacks `put_credential`'s System guard:
  under `TenantContext::System`, `DELETE ... WHERE provider = $1` deletes that provider across **all**
  spaces (RLS permits it). Current callers pass Member, but it is a latent cross-tenant delete.
  **P9.** Add an early `TenantContext::System => Ok(false)` in the store and a test. Not SQL in 0043 —
  flag to the owning module (additive app change).

### F3. Dedup / atomicity (CONFIRMED design, GUESS risks)

- PK-based dedup is sound. **P10.** Preserve `created_at` on conflict (already implicit — `DO UPDATE`
  never sets it); document that `updated_at = now()` is DB time while `created_at` is app time.
- **P11 (GUESS).** GCM nonce reuse under one DEK is catastrophic; add `UNIQUE (space_id, provider,
  nonce)` only after confirming `SecretBox` draws a fresh random nonce each connect (today's test
  only checks non-empty). Otherwise this index adds write cost for no protection.
- **P12 (GUESS, defer).** No DEK-version column: if `space_keys` ever rotates to a *new* key (not just
  a master re-wrap, 0026), existing ciphertext is undecryptable with no version marker. Add a nullable
  `key_version` column + app support in a dedicated wave, not here.
- Connect is validate-then-upsert with no idempotency key; a race is last-write-wins (acceptable for
  REQ-052 "failed connect stores nothing"). Document; do not add a key.

## Risks and trade-offs

- **R1.** P1/P2/P3 `VALIDATE` fails on any pre-existing bad row and blocks the deploy. Pre-flight
  `SELECT ... WHERE NOT (predicate) LIMIT 1` on staging; fix data in a separate additive migration,
  never by weakening the check.
- **R2.** P3/P11 depend on unconfirmed crypto details (GUESS). Ship P1–P2 + RLS tests first; tighten
  only after reading `SecretBox`.
- **R3.** P8 is not achievable in-migration and changes connection handling; defer with approval.
- **R4.** P9 is an app fix, not a migration; without it the cross-space delete remains latent.
- **R5.** All proposals are independently revertable (DROP CONSTRAINT/COMMENT/INDEX); no gate,
  lockfile or observable API/data-format change. Additive only.

## Suggested rollout for v0.4.0

Wave A (this doc): P1, P2, P4, P5, P6, P7, P9, P10, R1. Wave B (after crypto confirmation): P3,
P11; Wave C (owner approval): P8, P12. Verify with `cargo test -p boma-infra` (REQ-052/REQ-043/REQ-015)
and a `sqlx migrate` dry run on a populated copy; mark claims CONFIRMED only after that output exists.

_Scope note: line/section references are against `before-m5` as read this session. No CI, DB or test
command was executed in this READ-ONLY pass; crypto details and current bad rows remain GUESS._
