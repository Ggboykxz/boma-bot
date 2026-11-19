# 0316 — `crates/infra/src/space_keys.rs` — reliability proposal (v0.4.0)

Worker #316 · lens: reliability · READ-ONLY review of `.worktrees/before-m5`. Proposals only; no code
changed. **CONFIRMED** = read in this tree this session; **GUESS** = inference needing a runtime test.
Anchors: `space_keys.rs:23-93`, `secret_box.rs:55-85,104-151`, `tenant.rs:107-143`,
`stores/secrets.rs:228-274,284-327`, `migrations/0026_space_keys.sql:23-24`, ADR-020, REQ-046.

## Scope note

Three functions: `load_or_create_dek` (`:23-62`), `seal_for_space` (`:69-77`), `open_for_space`
(`:85-93`). All take `&mut PgConnection` (a tenant transaction from `tenant::with_tenant`) and a raw
`space_id: &str`. This module mints/wraps DEKs but returns only a bare `SealedSecret`; the caller
chooses which row stores it. It shares the `space_keys` table with five inline copies (secrets,
models, messaging, connectors, mcp), so any behavior change here must stay byte-compatible with
those copies (ADR-020 "recorded path").

## What is already right (keep)

- The `ON CONFLICT (space_id) DO NOTHING` + survivor re-select (`:42-57`) settles concurrent first
  creators on one row; no split DEK from a pure insert race (CONFIRMED).
- RLS on `space_keys` (migration 0026:23) filters reads and checks writes by `boma.space_id`; in a
  member context a foreign `space_id` fails closed (CONFIRMED).
- `tenant::scope` sets `boma.space_id` with `is_local = true` and rolls back on error
  (`tenant.rs:118-142`), so a pool connection never carries a tenant to its next borrower (CONFIRMED).

## Failure-mode inventory

F1 — Cross-space leakage / wrong-space sealing (CONFIRMED). `space_id: &str` is caller-disciplined
and never checked against the transaction's `boma.space_id`. Under `as_system` RLS allows any space
(`boma.is_system()`), so a wrong argument loads or mints a foreign space's DEK. `seal_under_with_nonce`
(`secret_box.rs:88-102`) passes no AAD, so a `SealedSecret` carries no self-describing space: a value
sealed under space B's DEK and written into space A's row decrypts under B, not A — a silent
ownership error open time cannot detect. RLS limits row reads, but it cannot catch a mis-sealed value.

F2 — Key loss via ephemeral master (CONFIRMED). `load_or_create_dek` will mint and persist a DEK
wrapped under an ephemeral `BOMA_MASTER_KEY` (`secret_box.rs:55-61,74-79`) with no `stable_key()`
guard (`:83-85`). On restart the process generates a different master and every wrapped DEK fails to
unwrap — total, silent unavailability. Repeated `SecretBox` construction (5x in `commands.rs`, see
wave-11 #0194) can even split keys within one live process.

F3 — Error conflation (CONFIRMED). DB outage, wrong master key, GCM auth failure, bad nonce length
(`secret_box.rs:109-111`) and non-UTF8 plaintext (`:131-133`) all collapse to `AppError::Unavailable`
(`:33,51,57`). A retry policy cannot tell "retry later" from "operator must rekey"; a rotated key
surfaces as a fake database outage. No reason code is logged.

F4 — Rotation race / key loss (CONFIRMED). Read (`:29`) → unwrap (`:35`) and insert (`:42-51`) →
survivor-read (`:53`) span several statements with no row lock; under READ COMMITTED each sees a new
snapshot. A concurrent `rekey_space_keys` (`stores/secrets.rs:244-266`, `FOR UPDATE`) can commit a
new wrap between them → unwrap fails → transient `Unavailable` on legitimate traffic. Worse, a space
whose DEK is first created *after* rekey's snapshot is not in its `FOR UPDATE` set, so it is never
rewrapped; after the env var is swapped, that space's secrets are permanently unreadable. No advisory
lock, no `rotated_at` staleness check, no read-back verification in this path (CONFIRMED by reading
`rekey_space_keys`, which returns success without re-unwrapping).

F5 — DEK not zeroized (CONFIRMED). `load_or_create_dek` returns `[u8; 32]` by value (`:27`) held in
locals in all three callers and passed through `secret_box.rs`; copies remain in memory. No
`zeroize::Zeroizing` anywhere on this path.

F6 — No tests in this module (CONFIRMED). No `#[cfg(test)]` block; no coverage for concurrent
creation, foreign-space rejection, ephemeral-key refusal, or rotation interleavings. (GUESS: inline
copies may cover some paths indirectly, but not this module's invariants.)

## Concrete hardening (prioritized)

P0 — Fail fast on an unstable master in the create path. In `load_or_create_dek`, refuse to mint a
DEK unless `secrets.stable_key()` is true; require startup-time validation in `serve`/`worker` (ties
to wave-11 #0194 P1 single-vault + config gate) so a misconfigured deployment dies at boot rather
than persisting unrecoverable rows. Risk: changes first-seal behavior for dev/test without an env key
— keep `ephemeral()` for tests and gate on an explicit `allow_ephemeral` flag; no wire change.

P0 — Make the DEK load atomic. Replace the select/unwrap/insert/select sequence with a single
`INSERT ... ON CONFLICT (space_id) DO UPDATE ... RETURNING wrapped_key, nonce` (or a CTE) so the
read and the conflict survivor come from one snapshot; unwrap once. Add a short session advisory lock
shared with `rekey_space_keys` (or a `key_generation` column checked before unwrap) to serialize
rekey against creation. Risk: advisory lock adds contention on a hot path; a generation column is a
schema change (internal only, but record it); coordinate the rekey side with the `secrets.rs` worker.

P0 — Rekey read-back verification (cross-module, note here). After `rekey_space_keys`, re-select and
unwrap every row under the new master and fail the command if any row does not open; take the same
advisory lock so no new row commits mid-rekey. Risk: doubles a full scan (latency); a badly scoped
lock can block migrations — use a distinct, short-lived lock key, always released on drop.

P1 — Typed error + reason code. Add an internal `KeyUnwrapFailed`/`KeyUnavailable` variant distinct
from `Unavailable`, log a stable reason code (never key material), and keep the client-facing wire
error generic. Risk: touches `boma_app::AppError` and its wire mapping; must not change the external
error shape (record as DEVIATION only if it leaks).

P1 — Bind the space into the ciphertext. Use `SpaceId` instead of `&str` for the three parameters and
pass the space id as AEAD associated data (AAD) so a misrouted value cannot open under the wrong
space. Risk: AAD changes the AEAD input — existing sealed values would not authenticate, requiring a
re-seal migration and a DEVIATION gate; land this only with an explicit owner decision. The `SpaceId`
newtype change alone is internal and low-risk.

P1 — Zeroize key material. Return `zeroize::Zeroizing<[u8; 32]>` from `load_or_create_dek` and pass
`&*` into `seal_with_data_key`/`open_with_data_key`. Risk: internal type change only.

P2 — Tests. Add `#[cfg(test)]` (or `tests/`) with testcontainers: concurrent `load_or_create_dek` on
one space yields one DEK (REQ-046 in the name); foreign space under system context is rejected;
no create under an ephemeral key; unwrap with a rotated master yields the typed error; rekey races a
new-space creation and the new row still opens; truncated/corrupt nonce and ciphertext fail with the
reason code. Keep the inline copies byte-compatible (a shared golden vector).

## Open questions (route to docs/STATE.md)

- AAD binding invalidates existing ciphertexts — acceptable re-seal migration/DEVIATION, or defer?
- Is a `key_generation` column or a shared advisory lock the chosen serialization mechanism?
- Should `rekey_space_keys` verification live in `secrets.rs` or be centralized into this module?

Status: all findings CONFIRMED by source reading except the noted GUESS item; no runtime verification
performed (read-only worker).
