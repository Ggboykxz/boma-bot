# ADR-020 — Secrets for bots: envelope encryption with an environment master key (REQ-046)

Status: Accepted (2026-11-13, decision recorded by the rebuild)

## Context

`spec/01-requirements.md` REQ-046: bots can request, store, list and forget secrets; values are
encrypted at rest and never shown again; a member can inject one during a run. The spec's
non-functional file states the guarantee as "sealed at rest with AES-GCM, key per space wrapped by a
master key" and its decision record `DEC-10` asks which key architecture the rebuild uses:

- (a) single master key from the environment, AES-GCM per space,
- (b) envelope encryption with a key-encryption key in a KMS/HSM and rotation,
- (c) delegate to an external secret manager.

The DEC-10 recommendation is (b) with (a) as the default self-hosted profile, plus a documented
rotation procedure and a test that a rotated key still reads old ciphertext. Owner: engineering +
security; the decision blocks the plan because it is a schema decision. No one could be asked at the
time of this decision (the release owner directs: decide without asking, every milestone complete),
so this ADR records the rebuild's call.

## Alternatives

1. **Flat master key over every secret.** One `BOMA_MASTER_KEY` AES-256-GCM key seals every value.
   Simplest possible shape; but the spec explicitly promises a per-space key wrapped by a master
   key, losing that costs defense in depth (one key compromise opens every space) and makes
   rotation touch every secret row.
2. **Envelope with a KMS/HSM KEK (option (b)).** Strongest; but it couples a self-hosted,
   low-moving-parts deployment to a cloud KMS or HSM, violating criteria 2 (maturity for all
   targets), 5 (boring technology) and 7 (license/cost for a self-hosted default). Nothing in the
   spec requires a network call at every seal.
3. **Envelope with an environment master key (option (a), chosen).** Per-space random 256-bit
   data-encryption keys (DEKs), each wrapped (AES-256-GCM) under `BOMA_MASTER_KEY`; secrets sealed
   under their space's DEK. Rotation re-wraps the DEKs only (one statement per space), never the
   secrets — this is exactly the rotation procedure the recommendation asks for, and the test that
   proves it (`rekey_space_keys`, `crates/infra/tests/m1.rs::req_046_…_survives_a_rekey`) is the
   test the recommendation asks for.
4. **External secret manager.** Adds a whole second service and identity model to a single-binary
   deployment; rejected on criterion 5, and (c) is not the recommendation.

## Decision

Envelope encryption with a per-space DEK wrapped by an environment master key:

- Migration `0025_bot_secrets.sql` creates `space_keys` (space_id, wrapped_key, nonce, created_at,
  rotated_at) and `bot_secrets` (id, space_id, bot_id, name, ciphertext, nonce, created_at), with
  tenant row-level security on both (same pattern as every other tenant table).
- The master key comes from `BOMA_MASTER_KEY` (exactly 64 hex characters). In development and tests
  where the variable is unset, a per-process ephemeral key is generated and a warning logged; a
  production deployment must set it (`.env.example` documents this, and `SecretBox::from_env`
  refuses a malformed value at startup).
- Plaintext crosses the app/store boundary only in `SecretStore::put` (same process) and in
  `SecretStore::reveal` (the computer-shell executor only); there is no API path returning a value,
  no field in any reply, no log line. `list` answers names and timestamps.
- Rotation: `boma rekey` (implementation: `rekey_space_keys`, system tenant
  `SystemReason::DataMigration`) unwraps every DEK with the old master key and re-wraps it with the
  new one, setting `rotated_at`. Secret ciphertexts are never re-encrypted, so rotation is cheap and
  the requirement "a rotated key still reads old ciphertext" is covered by a test.
- Secrets reach a bot's tools exclusively as environment variables `BOMA_SECRET_<NAME>` of the
  computer-shell process (`computer_shell`); the value never enters a message, an event, a reply or
  the log.

This answers DEC-10 with (a) exactly as the recommendation's default self-hosted profile; a future
KMS-backed KEK (option (b)) can replace the `wrap_data_key`/`unwrap_data_key` primitives without a
schema change.

## Consequences

- Simpler than option (b): no new service, no network dependency at seal time, one binary.
- Rotation is O(number of spaces), not O(number of secrets); the doc `.env.example` and this ADR
  document the procedure.
- Losing `BOMA_MASTER_KEY` loses every secret (fatally, as with any env-key design); the rebuild
  does not do backups — operations documentation (M11 release notes) must repeat that instruction.
- The per-space DEK adds two columns and one migration to the tenant set; row-level security
  extends to the new tables like the rest of the schema.
- KMS remains available as a future swap of the two wrapping primitives; no decision is locked in.
