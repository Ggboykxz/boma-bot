# Worker #364 — `crates/infra/src/stores/models.rs` — LENS: reliability — v0.4.0 proposal

- **Status:** PROPOSAL (read-only review; no source changed). CONFIRMED = verified by reading this
  238-line file plus `ports.rs`, `migrations/0043_models.sql`, `migrations/0026_space_keys.sql`,
  `space_keys.rs`, `secret_box.rs`, `tenant.rs`, `local_models.rs`, `domain/model.rs`, `usecases.rs`
  and `crates/infra/tests/m1.rs`; GUESS = needs a test/oracle run.
- **Scope:** `PostgresModelStore`, the `ModelStore` adapter for BYOK credentials and the person's
  model preference (REQ-052, REQ-015), and its seam with live local-model discovery (REQ-075).

## Summary

The adapter is small and its SQL is parameterised, RLS-backed and conflict-safe at the row level.
Reliability rests on three untested assumptions: that callers always validate input, that a
`connected` row is actually usable, and that the inline DEK bootstrap is equivalent to the shared
`space_keys::load_or_create_dek` copy. Nothing here detects a stale key, a stale preference, or a
misclassified client error.

## Failure modes

### Dedup / last-write-wins (D)

- **D1 (MED, CONFIRMED):** `put_credential` (90–105) upserts on `(space_id, provider)` with no ordering
  token. Two concurrent connects race and the last committed statement wins even if its key is older
  (e.g. a retried connect arriving after a rotation). No idempotency key, no re-validation of the
  stored value. GUESS: invisible in the UI because only `provider`, `status`, `created_at` are returned.
- **D2 (LOW, CONFIRMED):** on conflict, `created_at` is preserved but `updated_at = now()` (server);
  on first insert both columns come from the caller's `$5::timestamptz` (102) — a caller can backdate
  the "connected since" value. No ordering signal is exposed for reconciliation.

### Stale discovery / stale state (S)

- **S1 (HIGH, CONFIRMED):** `has_credential` (164–176) and `list_credentials` (137–155) answer purely
  from row existence with `status = 'connected'`. The store never opens the ciphertext, so a key
  sealed under a rotated master key, a DEK wrapped by an old master key, or a tampered ciphertext
  still reports "connected". `models_catalog` (usecases.rs:3539–3544) merges that state into the UI.
  Runtime impact GUESS until a rotation/corruption test is run.
- **S2 (MED, CONFIRMED):** local discovery (`local_models.rs:28–44`) is probed live and never cached,
  so there is no stale cache here — but nothing ties a stored `model_key` (184–196) to what is
  currently reachable. A preference for a loopback model whose server stopped stays in the row and
  `resolve()` (domain/model.rs:69–85) silently falls back for an unpinned choice. No read-time
  validity signal exists.
- **S3 (LOW, CONFIRMED):** `has_credential` wraps `SELECT EXISTS` in `Option<bool>` with
  `unwrap_or(false)` (166–176); EXISTS never returns NULL, so the fallback is dead code that blurs
  "no row" and "answer missing". `list_credentials` has no LIMIT (bounded in practice by provider
  count) and depends on `boma.rfc3339(...)` existing (138) — schema drift becomes a 503.

### Invalid configs / error classification (C)

- **C1 (MED, CONFIRMED):** the store trusts `NewModelCredential` as "already validated" (ports.rs:3737).
  Provider syntax/length and the live key check live only in `models_connect` (usecases.rs:3571–3616).
  Any other caller (e.g. an OAuth completion path) can insert an empty provider, an empty api_key or a
  garbage model_key; migration 0043 guards only `status` (line 12). No defence in depth.
- **C2 (MED, CONFIRMED):** `created_at` is a caller-supplied string bound as `$5::timestamptz` (102).
  A malformed value is a SQL error mapped by `tenant::unavailable` → `AppError::Unavailable`/503
  (error.rs:43–51): a client fault is reported as a retryable dependency failure and can drive retry
  storms. `set_preference` (199–237) stores arbitrary `model_key` text with no length/format CHECK
  (migration 0043:29–33).
- **C3 (MED, CONFIRMED):** `space_key` (26–68) duplicates `space_keys::load_or_create_dek`
  (space_keys.rs:23–62) verbatim — read, fresh DEK, `INSERT ... ON CONFLICT DO NOTHING`, re-read the
  survivor. Two crypto-critical copies must stay identical; the shared module already documents an
  inline copy "kept stable for the recorded path". The four statements are not an explicit
  transaction; GUESS a transient failure between INSERT and survivor SELECT returns 503 although a
  DEK was persisted (line 61–62). An unwrapable wrapped DEK permanently bricks credential writes for
  that space with no self-heal or alert.
- **C4 (LOW, CONFIRMED):** `forget_credential` (120) deletes by `provider` only, relying entirely on
  RLS (migration 0043:20–24; isolation asserted in m1.rs:3222–3250); the `space_id` intent is implicit.
  System-context contract is asymmetric: `preference()` returns `Ok(None)` (182) while
  `set_preference` returns `NotFound` (206) — GUESS: intentional.

## Concrete hardening

- **P0 — one DEK path:** replace inline `space_key` with `crate::space_keys::load_or_create_dek` (or
  one shared helper both callers use), preserving the recorded path through delegation. Add an
  N-concurrent-writers test (one DEK survives) and a corrupt-DEK test. Risk: touching the recorded
  crypto path — keep semantics byte-equivalent; no data migration.
- **P0 — validate at the boundary:** parse `created_at` into a typed RFC3339 value before binding;
  re-check provider syntax/len (lowercase alnum/hyphen, ≤32, mirroring `models_connect`), non-empty
  api_key and model_key; return `AppError::Rule`, never `Unavailable`, for malformed input. Risk:
  audit all direct callers first — stricter checks could reject a value accepted today (DEVIATION →
  PARITY note); GUESS until that audit runs.
- **P1 — detect stale "connected":** open the ciphertext under the space DEK inside
  `has_credential`/`list_credentials` (or add a verify method to the port); distinguish unopenable
  credentials and emit `tracing::warn!` + a metric. Risk: a new status needs a migration (0043 CHECK
  allows only `connected`) and changes the catalog wire shape — DEVIATION, operator approval needed;
  ship warn+metric (no wire change) as the low-risk first increment.
- **P1 — dedup with ordering:** add a version/`updated_at` guard to the upsert
  (`WHERE EXCLUDED.updated_at >= model_credentials.updated_at`) and generate timestamps server-side
  so a delayed writer cannot clobber a newer key; replace SELECT-after-INSERT in `space_key` with
  `INSERT ... RETURNING` plus a conflict re-read. Risk: concurrent-connect semantics change — pin
  with a test and record in `docs/PARITY.md`.
- **P1 — stale preference:** expose a read-time validity signal for `model_key` against the reachable
  set, preferably in the `models_preference` usecase (it owns that set) rather than injecting config
  into the store. Risk: port-boundary change if done here; keep the store pure.
- **P2 — hygiene and coverage:** explicit `space_id` in the DELETE; never delete `space_keys` on
  disconnect (the DEK is shared with bots/secrets); tracing spans keyed by provider/space only, never
  key bytes; tests for malformed timestamp → Rule, tampered/rotated key → not silently connected,
  concurrent put dedup, and the System-context contract (RLS coverage already exists).

## Risks

- DEVIATION exposure: status values, upsert ordering and validation hardening are externally visible;
  record in `docs/PARITY.md` and wait for approval (AGENTS.md).
- Do not remove or rotate the shared space DEK from this store; `space_keys` serves every secret family.
- Any SQL edit needs `cargo sqlx prepare` metadata refreshed, then `cargo fmt --check`, clippy
  `-D warnings` and the integration suite green before claiming success.

## Suggested acceptance-test names

- `req_052_malformed_created_at_is_a_rule_error_not_unavailable`
- `req_052_a_credential_under_a_rotated_master_key_is_not_reported_connected`
- `req_052_concurrent_puts_settle_on_one_credential`
- `req_052_direct_store_rejects_empty_provider_and_key`
- `req_015_a_stale_discovered_model_preference_is_flagged`

## Decisions needed (GUESS → operator/spec)

1. Should `connected` become a typed status with an `invalid`/`unverified` state (migration + PARITY)?
2. Is last-write-wins on concurrent connects acceptable, or must a rotation win deterministically?
3. Is the System-context read/write asymmetry intentional?
