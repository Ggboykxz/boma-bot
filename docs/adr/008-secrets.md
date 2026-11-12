# ADR-008 — Secrets: envelope encryption with a per-space data key and a rotatable master key

Status: proposed (awaiting owner go) — this is DEC-10

## Context

Bots hold connector credentials, model keys, voice keys and per-bot secrets (REQ-046). The spec
requires that no procedure ever returns plaintext, that logs and exports never contain secret
material, and that a key rotation still reads old ciphertext. `04-nonfunctional.md` requires a
documented guarantee before any secret is stored.

## Alternatives

1. **Envelope encryption: a random data key per space, wrapped by a master key from the environment
   (or a KMS adapter); AES-256-GCM per secret with a bound AAD.**
2. External secret manager only (Vault, cloud KMS) with no local ciphertext.
3. Encrypt everything with one global key from the environment.
4. Rely on database-level encryption and access control, store plaintext columns.

## Scoring

| Criterion | 1 envelope | 2 external only | 3 single key | 4 plaintext |
|---|---|---|---|---|
| 1 fits the spec | 1 | 3 (self-host must work without Vault) | 2 (rotation is a full re-encrypt) | 5 |
| 2 maturity | 1 | 1 | 1 | 1 |
| 3 security | 1 | 1 | 3 (one key compromises all tenants) | 5 |
| 4 performance | 1 | 3 (network call per decrypt) | 1 | 1 |
| 5 simplicity | 2 | 5 | 1 | 1 |
| 7 licence | 1 | varies | 1 | 1 |

## Decision

Option 1, using `aes-gcm` and `ring`/`rustcrypto` primitives behind a `SecretBox` port.

- `space_data_keys(space_id, wrapped_key, key_version)`; the master key comes from
  `BOMA_MASTER_KEY` (32 bytes, base64) and can be a KMS-backed adapter in a later milestone.
- Every ciphertext row stores `key_version`, a random 96-bit nonce and the AAD
  `space_id || table || column`, so a ciphertext moved to another tenant fails to decrypt.
- Decryption never crosses a log line: the plaintext type is `SecretString` (`secrecy`), `Debug` is
  redacted, and a lint bans `expose_secret` outside the adapter.
- Rotation: write a new wrapped key with a new version, then lazily re-wrap on use; a background job
  rewrites the remainder. A test decrypts a v1 ciphertext after rotating to v2.

## Consequences

- A leaked database dump without the master key reveals no secret, which is the threat that matters
  for a self-hosted product.
- A leaked master key plus a dump reveals every space's secrets; the README states this plainly and
  documents rotating the master key.
- `POST /rpc/*` responses and audit exports are tested to contain no plaintext; V-02 (dev mailbox) is
  not implemented at all.
