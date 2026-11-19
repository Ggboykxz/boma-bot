# Wave 14 — Worker 0315 — `crates/infra/src/space_keys.rs` (architecture) — v0.4.0 proposal

Scope: read-only architecture review of `crates/infra/src/space_keys.rs` (93 lines), its sibling
`secret_box.rs`, `migrations/0026_space_keys.sql`, `docs/adr/020-secrets.md`, and every caller of
the `space_keys` table (`stores/secrets.rs`, `stores/models.rs`, `stores/messaging.rs`,
`stores/connectors.rs`, `stores/mcp.rs`). All claims CONFIRMED by direct reading unless marked
GUESS. No behavior change is proposed unless explicitly noted. Nothing else modified.

## Current structure (CONFIRMED)

The module owns the mechanics of per-space envelope encryption: `load_or_create_dek` selects the
wrapped DEK row, mints a fresh 256-bit DEK and inserts with `ON CONFLICT DO NOTHING` on first use,
then re-selects the survivor and unwraps it; `seal_for_space` / `open_for_space` are thin wrappers
that call it and delegate to `SecretBox`. It takes `&mut PgConnection` (caller owns transaction and
RLS context) and a `space_id: &str`. The table has exactly `space_id, wrapped_key, nonce,
created_at, rotated_at` and is protected by RLS
(`space_id = boma.current_space_id() OR boma.is_system()`). `lib.rs` declares `mod space_keys;`
(private), but `connectors.rs` and `mcp.rs` call `crate::space_keys::seal_for_space` directly.

The module doc says the secrets store keeps "its own inline copy kept stable for the recorded path".
The actual duplication is wider: **five minting copies** exist — this module,
`stores/secrets.rs::PostgresSecretStore::put` (l.58-97), `stores/secrets.rs::space_dek` (l.284-327),
`stores/models.rs::PostgresModelStore::space_key` (l.26-68), and
`stores/messaging.rs::PostgresMessagingStore::unwrap_or_fresh_in` (l.578-620) — plus two read-only
SELECT copies (`secrets.rs::reveal`, `messaging.rs::credential`) and a separate rotation loop
(`rekey_space_keys`). No test or gate enforces which copy is authoritative. The module itself has
no `#[cfg(test)]` block; idempotency and cross-space isolation are tested only indirectly in
`secret_box.rs` and `crates/infra/tests/m1.rs`.

## Architectural risks observed (CONFIRMED)

- **Drift across five copies.** The mint logic differs subtly: this module re-selects with
  `fetch_one`, the other four use `fetch_optional` + `AppError::Unavailable`; `space_dek` takes a
  `&PgPool` and acquires its own connection, the rest take `&mut PgConnection`. A rotation, AAD
  binding, or error-taxonomy fix applied here silently misses four callers.
- **Plain `&str` space ids.** A `boma_domain::SpaceId` newtype exists and `space_dek` already uses
  it; every other copy accepts arbitrary strings, so an invalid/missing space is representable at
  the key-management boundary and only caught by RLS or the FK.
- **No cryptographic binding to the space.** `wrap_data_key` seals the bare 32-byte DEK with no
  associated data. A wrapped row copied from space A into space B (possible under the system tenant
  used by `rekey_space_keys`) unwraps to the same DEK. Isolation currently rests entirely on RLS.
- **No key-version discriminator.** Rows carry no `kid`/`key_version`; `rotated_at` is an audit
  timestamp, not a routing key. ADR-020 claims a future KMS KEK can replace the two primitives
  "without a schema change", but with one wrapped form per row there is no way to read mixed old/new
  rows during a rolling rotation, so that swap is actually an offline all-or-nothing rewrap.
- **Collapsed error taxonomy.** A wrong master key, a tampered nonce, a malformed DEK, and a dead
  database all become `AppError::Unavailable`. The `rekey` flow must infer "wrong --old-key" from a
  generic failure (see wave-11 worker 0194), which is an operability defect at this boundary.
- **Plaintext DEK lifetime.** `load_or_create_dek` returns `[u8; 32]` by value with no `zeroize`
  wrapper; the unwrapped key outlives its use on the stack. GUESS: `zeroize` is already transitive
  via `secrecy`, but that must be checked before adding a dependency.

## Proposed upgrades for v0.4.0

1. **One owner for the mint primitive.** Make `space_keys.rs::load_or_create_dek` the single
   implementation and migrate the four minting copies plus the two read-only SELECTs to call it;
   delete the inline blocks and keep the "recorded path" comment as a pointer, not a second copy.
   Add a module-level test for first-use mint, concurrent `ON CONFLICT` winner, and unwrap failure.
   *Risk: LOW-MEDIUM* — wide but mechanical, internal `pub(crate)` API only, no wire/data change;
   compile between callers because `space_dek` must acquire a connection before delegating.

2. **Unify the connection contract and type the id.** Make every entry point take
   `&mut PgConnection` and `space_id: &SpaceId` (or a `SpaceKeyScope` newtype), converting at the
   few callers that currently hold a `String` (messaging sealed credentials, connectors, MCP).
   Split the unwrap failure into a typed variant so a wrong master key is distinguishable from a DB
   failure. *Risk: MEDIUM* — touches five stores and their error assertions; keep the existing
   `AppError::Unavailable` string where `docs/PARITY.md` requires it and record any change.

3. **Add a wrapping-key version column in a new migration.** New `00NN_space_key_versions.sql`
   adds `key_version smallint NOT NULL DEFAULT 0` (never edit `0026` — see `docs/STATE.md` on the
   0025/0026 split); readers dispatch on the version, so legacy rows stay readable and a rolling
   KEK change can leave mixed rows in place. Extend `rekey_space_keys` to bump the version instead
   of assuming one KEK. *Risk: MEDIUM-HIGH* — every SELECT of `wrapped_key, nonce` must select the
   new column; rotation and read paths must agree atomically, and `-D warnings` will surface missed
   callers only if the tuple type changes, so change the shared row type in step 1 first.

4. **Bind the DEK to its space with AEAD associated data.** Pass `space_id.as_bytes()` as AAD to
   `wrap_data_key`/`unwrap_data_key` (extend the primitive with an AAD-taking variant, keep a
   deprecated no-AAD variant for legacy rows). Ship a one-time system-tenant migration that unwraps
   version-0 rows and re-wraps them as version 1 with AAD, reusing the `rekey` machinery and
   leaving secret ciphertexts untouched. *Risk: MEDIUM-HIGH* — migration ordering is critical: run
   a master-key rotation and the AAD migration in separate maintenance windows, with a verified
   read-back of both versions before deleting the legacy path; a mistake here makes every space in
   the batch unreadable. No secret is re-encrypted, so the REQ-046 rotation test still applies.

5. **Bound plaintext-key lifetime and measure before caching.** Return
   `Zeroizing<[u8; 32]>` (or an equivalent sealed newtype) from the DEK loader and zeroize the
   intermediate in `rekey_space_keys`. Add a `criterion` benchmark of `seal_for_space` /
   `open_for_space` with a real Postgres first; only then consider a short-TTL cache keyed by
   `(space_id, wrapping-key fingerprint)`. **Defer** the cache for v0.4.0 unless the benchmark
   shows it is the dominant cost. *Risk: LOW-MEDIUM for the hygiene part* (verify `zeroize` in the
   lockfile, add via `cargo add`, record license in `docs/DEPENDENCIES.md`); *HIGH for the cache* —
   stale DEKs after rekey, plaintext keys resident in memory, and cross-replica invalidation are
   unsolved, which is why it is explicitly out of scope here.

## Sequencing and gates

Order 1 → 2 → 3 → 4 → 5: consolidate structure, then types/errors, then the version column that
makes step 4's migration distinguishable and rollback-able, then AAD, then hygiene and measurement.
Each step: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, then the
`req_046_*` tests including `crates/infra/tests/m1.rs::req_046_…_survives_a_rekey`. Keep every
rustdoc on public items and REQ-ids in test names. No migration edits to already-shipped files, no
secrets in logs, and any externally visible error-string change recorded in `docs/PARITY.md` before
shipping; mark the GUESS items above as CONFIRMED or revise the proposal once verified.
