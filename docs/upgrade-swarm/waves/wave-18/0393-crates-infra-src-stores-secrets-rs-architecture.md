# v0.4.0 architecture proposal — `crates/infra/src/stores/secrets.rs`

Worker #393 · Lens: architecture · Target: v0.4.0 · Read-only review; no source changed.
Every statement is CONFIRMED by reading the 327-line module, `SecretBox`
(`crates/infra/src/secret_box.rs`), the `SecretStore` port and its DTOs
(`crates/app/src/ports.rs:2012-2096`), the sibling `destinations.rs`, `tenant.rs`, migrations
0025/0026/0002, unless marked GUESS. No externally visible behaviour change is proposed: any such
change is a DEVIATION per AGENTS.md and needs owner approval first.

## Current structure (as read)

- `PostgresSecretStore { pool: PgPool, secrets: SecretBox }` — thin pool+vault wrapper, `Clone`.
- Port impl: `put`, `list`, `delete`, `reveal` (REQ-046). Two free functions: public
  `rekey_space_keys` (offline master-key rotation) and `pub(crate) space_dek`, shared with the
  destinations store (REQ-065) so webhooks seal under the same per-space DEK.
- Envelope encryption: `space_keys` holds a per-space DEK wrapped under `BOMA_MASTER_KEY`; each
  `bot_secrets` row is sealed under that DEK. RLS (`boma.current_space_id() / is_system`) plus the
  `tenant::with_tenant` transaction is the tenant guard; the master key never leaves `SecretBox`.
- **DEK resolution is duplicated.** `put` (lines 61-97) inlines "read wrapped row or mint fresh
  (ON CONFLICT DO NOTHING, then re-select the survivor)". `space_dek` (lines 290-325) repeats the
  same algorithm verbatim. Same intent, two bodies — a drift hazard.
- The two copies run in **different connection contexts**. `put`'s DEK logic runs on the tenant
  transaction `conn` passed by `with_tenant` (RLS setting present). `space_dek` takes `&PgPool`
  and calls `pool.acquire()` itself; in `destinations.rs:81,226` it is called with the outer pool
  from *inside* a `tenant::in_space` closure, i.e. on a different connection without that
  transaction's `boma.space_id`. CONFIRMED: distinct connection sources. GUESS: under RLS the
  fresh connection sees no tenant (`current_space_id()` NULL, `is_system()` false), so its
  SELECT finds no DEK and its INSERT may fail the WITH CHECK policy — verify against a live test
  before calling it a defect; either way the split is an architectural seam.
- No row-decoding layer: positional tuples (`(Vec<u8>, Vec<u8>)`, 4-tuples) are matched inline and
  every failure is mapped through `tenant::unavailable`, conflating corrupt stored data with
  "store down". SQL fragments (`SELECT space_id FROM bots WHERE id = $1`,
  `SELECT wrapped_key, nonce FROM space_keys WHERE space_id = $1`) are copy-pasted across methods.
- `rekey_space_keys` takes `FOR UPDATE` over **all** `space_keys` rows in one system transaction
  and re-wraps them in a loop; `space_keys` has no key-id/version column, so there is no way to
  tell which master key wrapped a row, no staged/overlapping rotation, and no rollback path.
- DEK/master bytes are raw `[u8; 32]` (not `Zeroizing`/`secrecy::Secret`), so they are not wiped on
  drop. No `tracing` instrumentation in this module.

## Architecture read

The adapter is small, correct-looking, and faithful to the tenant/RLS pattern, but its seams are:
two DEK-resolution implementations with divergent connection semantics, no decode/error boundary,
rotation that is all-or-nothing and unversioned, non-zeroized key material, and no observability.
v0.4.0 should tighten these **inside the current port and schema-facing behaviour**.

## Concrete upgrades for v0.4.0 (with risks)

1. **One DEK resolver, one connection context.** Extract a private `resolve_dek(conn, space_id,
   &SecretBox)` that contains the read-or-mint-once algorithm, and make both `put` (passing its
   tenant `conn`) and `space_dek` use it. Change `space_dek` to accept an `&mut PgConnection`
   instead of `&PgPool`, and update `destinations.rs` callers to pass their existing `conn`, so
   DEK I/O happens under the same RLS/transaction as the caller. Risk: signature change to a
   `pub(crate)` helper touches destinations (REQ-065) — keep behaviour byte-identical and re-run
   its integration tests; first CONFIRM the current RLS outcome so a fix is not mislabelled.

2. **A single row-decode boundary and honest errors.** Introduce private `SpaceKeyRow` /
   `SecretRow` (`TryFrom` over named columns) and route all four methods through it; replace
   `unwrap`-free tuple matching and shared SQL `const` fragments so a column rename cannot drift.
   Map malformed ciphertext/nonce/length to one documented error via existing `AppError` variants
   rather than blanket `Unavailable`. Risk: a new `AppError` variant is API-visible — reuse an
   existing variant or record a DEVIATION; if SQL text changes, regenerate SQLx offline metadata
   (GUESS that CI pins it — verify).

3. **Versioned, staged master-key rotation.** Add a nullable `key_id`/`master_version` column to
   `space_keys` (new migration) so each wrapped DEK records which master wrapped it; make `rekey`
   process rows in bounded batches under short transactions instead of one `FOR UPDATE` over the
   whole table, allow old+new keys to coexist during a cutover, and verify by unwrapping a sample
   before committing each batch. Risk: schema change + externally visible CLI behaviour — log as
   a proposal in `docs/STATE.md` and seek owner approval before v0.4.0 scope; keep the existing
   `--old-key/--new-key` interface, default batch size, and `rotated_at` semantics.

4. **Zeroize key material at the boundary.** Return `Zeroizing<[u8; 32]>` from the resolver and
   keep DEK bytes in zeroizing wrappers through seal/open; do the same for `SecretBox::master`
   in a sister change. Risk: touches `SecretBox` and the destinations call site; AES-GCM ops need
   `ExposeSecret` at the last moment. Confirm clippy `all`/`pedantic` stay clean; no behaviour
   change expected.

5. **Non-sensitive observability.** Add `tracing` spans to `put/list/delete/reveal/rekey` carrying
   `space_id`, `bot_id`, `name`, and rekey batch counts only — never `value`, `ciphertext`, `nonce`,
   or wrapped bytes (AGENTS.md: no secrets in logs). Risk: low; discipline is to avoid logging whole
   `NewBotSecret`/row structs whose `Debug` includes the `SecretString` and cipher bytes.

## Sequencing and open questions

- 1, 2, 4, 5 are internal refactors: land with the existing REQ-046/065 integration tests green,
  each as a test-first step named with the REQ id, then `cargo fmt --check`, clippy `-D warnings`,
  tests, one Conventional Commit. 3 needs owner approval (schema + CLI surface) and may slip past
  v0.4.0; ship 1-5 first if scope must be cut.
- GUESS to verify: does `space_dek`'s pooled connection actually pass RLS today? Decision needed:
  is a new `AppError` variant acceptable, or must decode failures reuse an existing one?
