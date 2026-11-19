# 0314 — crates/infra/src/secret_box.rs — reliability (v0.4.0)

Worker #314 · module `crates/infra/src/secret_box.rs` · lens: reliability · READ-ONLY review of
`rebuild-workspace/.worktrees/before-m5` at v0.3.0-era source (workspace says `0.3.0-rc5`). Proposals
only; no code in this change. Line refs are against this worktree. Claims tagged CONFIRMED (read
this session) or GUESS (inferred); no tests were run (read-only). Boundaries consulted:
`crates/app/src/ports.rs` (`SealedSecret`, L2012–2019), `error.rs` (`Unavailable` retryable, L49–51),
`service.rs` (`dependency_unavailable`, L603–620), `stores/messaging.rs` L577–621, `migrations/0026`,
`crates/bin/src/commands.rs` (`rekey`, L372–404), ADR-020.

## What it already does right (keep, don't re-propose)

- AES-256-GCM with a fresh 96-bit CSPRNG nonce per seal; the nonce is persisted and length-checked
  on open (L88–115) — nonce reuse is not caused by deterministic generation (CONFIRMED).
- Envelope shape: master wraps the per-space DEK, secrets are sealed under the DEK; rotation
  re-wraps DEKs only and is covered by a round-trip test (L282–305, ADR-020) (CONFIRMED).
- `Debug` omits the master key (L30–37); `SealedSecret` carries ciphertext + nonce only
  (ports.rs L2012–2019). Ciphertext and nonce bit-flips are both refused (L241–253) (CONFIRMED).

## Failure modes → concrete hardening

### F1. Nonce reuse

- Random 96-bit nonces have a birthday bound: ~2^32 seals under one key give ~50% collision chance;
  one GCM reuse under the same key leaks plaintext XOR and enables GHASH forgery. The same helper
  serves both the master key (every wrap and every rekey) and each space DEK (L88–102, L137–142);
  the only test checks that two nonces differ (L231–239), not uniqueness at scale (analysis; GUESS
  on operational volume). P1. Document a per-key seal budget and add a proptest sealing N=10k values
  under one DEK asserting all nonces distinct. P2. For a real fix, move to a nonce-misuse-resistant
  AEAD (AES-GCM-SIV, 24-byte nonce) or per-seal HKDF subkeys — both change the stored format.
  Risk: P2 is a **DEVIATION** needing an ADR, a re-encryption migration and REQ-046 tests; do not
  fake it with a counter unless persisted state is added (new column + RLS).

### F2. Key rotation

- The wrapped row stores no key identifier (`space_keys`: wrapped_key, nonce, created_at, rotated_at —
  migration 0026 L7–13), so old-key and new-key DEKs are indistinguishable. `rekey` is a manual
  command whose operator must swap `BOMA_MASTER_KEY` after it exits (commands.rs L372–404); an
  interrupted run leaves a mixed state that cannot be resumed or zero-downtime-overlapped (CONFIRMED).
  P3. Store a key fingerprint (e.g. first 8 bytes of SHA-256 of the master key, or a configured
  version string) beside the wrapped key and accept a set of candidate master keys during a rotation
  window. P4. Make rekey idempotent/resumable: skip rows already on the new fingerprint, set
  `rotated_at`, batch to avoid one long transaction, test a half-finished rekey. Risk: schema + RLS
  migration; the runbook must retain the old key until every row is rewrapped; key-id adds a small
  offline-guessing surface (mitigated: high-entropy key, no plaintext leak).
- `stable_key()` is checked only by `rekey` (commands.rs L387); normal startup accepts a missing key
  with a warning (L56–61), so a deployment can seal real secrets under an ephemeral key and lose them
  all on restart (CONFIRMED). P5. Add a strict-startup flag: fail fast when a production profile is
  set and the key is absent, keep ephemeral for tests/dev explicitly. Risk: behavior change →
  **DEVIATION**, ADR + `.env.example` + docs; ship with the flag defaulting to today's behavior in
  v0.4.0 and flip the default only with approval.

### F3. Decryption failure

- Every failure — RNG, bad nonce length, GCM auth failure, wrong key, bad UTF-8, DEK length — maps to
  `AppError::Unavailable` (L93–97, L109–114, L130–133, L150, L167–169), which is `is_retryable()`
  (error.rs L49–51) and surfaces as `dependency_unavailable` (service.rs L612). A tampered or
  wrong-key secret therefore looks like a transient dependency blip: retries burn REQ-024 failure
  budgets, no security event is logged, and ops cannot distinguish corruption from DB/RNG trouble
  (CONFIRMED). P6. Add typed `SecretBoxError::{BadNonce, DecryptionFailed, WrongKey,
  InvalidPlaintext}`; reserve `Unavailable` for RNG/cipher-init failures; make auth failure
  non-retryable. P7. Emit a rate-limited structured `warn!` on GCM auth failure (space/bot id and
  request id, never ciphertext or plaintext). P8. No AAD binds a ciphertext to its space/bot/name
  (L95–97, L113): if wrapped rows are swapped across spaces (RLS bypass or write bug), GCM still
  authenticates and a space can receive another space's DEK — add AAD = space_id for wraps and
  space_id+bot_id+name for secrets (analysis; GUESS that no caller supplies it today). Risk: P6/P7
  may alter the stable error envelope → **DEVIATION** if reveal is ever client-visible; verify the
  shell-only reveal path (ADR-020 L59–61) first and keep the new error internal. P8 breaks all
  existing ciphertexts → ADR + re-encryption migration, or a versioned envelope that tries AAD then
  legacy during a transition; schedule last.

### F4. Secret leakage

- `master: [u8; 32]` is a raw array and `SecretBox` derives `Clone` (L24–28): key copies spread and
  nothing zeroizes on drop. `fresh_data_key()` and `unwrap_data_key()` return raw `[u8; 32]`
  (L145–165), copied into store locals with no zeroization (messaging.rs L589–619) (CONFIRMED).
  P9. Hold master and unwrapped DEKs in `secrecy::Secret<[u8; 32]>` / `Zeroizing`, drop `Clone` where
  possible (pass `&SecretBox`), and zeroize the intermediate plaintext buffer — including the
  `String::from_utf8` error path at L131–133, which drops the buffer without wiping it. P10. Add a
  regression test asserting `Debug` of the vault and of a sealed secret cannot contain the key or
  plaintext. Risk: ripples through every store and testkit call site (compile-time catch); `from_env`
  still briefly holds the env string (document residual exposure); zeroization cannot cover OS
  swapping or core dumps.

## Explicit non-goals / rejected

- Never introduce deterministic nonces, log key/plaintext/ciphertext, or retry authentication
  failures to "fix" reliability. Keep every public name and the wire format unless an ADR approves
  a change; no new feature outside REQ-046 (proposals go to docs/STATE.md).

## Rollout (v0.4.0) and cross-cutting risks

- Wave A (internal, cheap): P1 proptest, P8-error-path zeroization, P10 regression test, P5 flag
  with today's default. Wave B (gated): P6/P7 error classification, then P3/P4 key-id + resumable
  rekey — ADR + docs/PARITY.md + REQ-046-named tests. Wave C (format, separate approval): P8 AAD,
  then P2 nonce-misuse-resistant AEAD with a re-encryption migration.
- R1. P3 key fingerprints and P7 logs must never include key bytes. R2. P6 touches app + API error
  mapping → contract test against spec/openapi.yaml. R3. P2/P8 invalidate all stored ciphertexts;
  a rehearsed backup + re-encryption drill is the acceptance gate, not a unit test. R4. P5 strict
  mode can strand existing ephemeral-key dev deployments — gate on a flag, announce in release notes.
