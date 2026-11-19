# v0.4.0 reliability proposal — `crates/infra/src/stores/mcp.rs` (worker #354, lens: reliability)

Scope: the Postgres adapter for MCP server records and their sealed credentials (REQ-033).
Read in this session; claims about what the code *does* are CONFIRMED by inspection, claims about
runtime impact are GUESS unless a test is named. All listed hardening is proposal-only.

## What already protects us (CONFIRMED)

- Every method runs inside `tenant::with_tenant`, which opens a transaction and commits only on
  `Ok`; the `replace_tools` DELETE + per-tool INSERT loop is therefore atomic and rolls back on a
  failed insert. Do not "fix" it by adding a second transaction.
- Credentials are sealed with AES-256-GCM nonce-per-seal under the space DEK; listings never carry
  ciphertext; `reveal_bearer` / `reveal_client_secret` are the only read paths.
- `remove` already reports `rows_affected() > 0`, and `create` maps unique violations to
  `AppError::Conflict` without leaving partial rows.

## Failure modes

### 1. Secret handling

- **S1 — ephemeral key silently seals unrecoverable secrets (CONFIRMED code, GUESS impact).**
  `SecretBox::from_env` falls back to a per-process key with only a `tracing::warn`. This store
  never checks `stable_key()`, so `create` / `store_bearer` / `store_oauth_session` will happily
  seal under a key that changes on the next restart. `load_or_create_dek` unwraps the *existing*
  row and returns `Unavailable` on auth failure — it does not re-wrap — so all previously stored
  bearer tokens, client secrets and PKCE verifiers become permanently unreadable. GUESS: an
  ephemeral-key deployment loses every MCP credential at each restart, with no store-level signal.
- **S2 — key mismatch is indistinguishable from corruption (CONFIRMED).** `unwrap_data_key`,
  `open_with_data_key` and a bad nonce/ciphertext all collapse to `AppError::Unavailable`; the
  caller cannot tell "master key rotated, DEKs need re-wrapping" from "DB is down". Operators will
  debug the wrong layer.
- **S3 — DEK chosen from the row, not the context (CONFIRMED).** `reveal_bearer`,
  `reveal_client_secret`, `store_bearer`, `store_oauth_session` and `take_oauth_session` read
  `space_id` from the fetched row and pass it to `open_for_space` / `seal_for_space`. Tenant
  isolation depends entirely on RLS (`boma.space_id`); if a row ever carries a foreign `space_id`,
  the vault will unwrap the *wrong* DEK rather than refuse (GUESS: cross-space plaintext leak or
  auth failure depending on which key is wrapped).

### 2. Stale config

- **T1 — caller-supplied clock for OAuth expiry (CONFIRMED).** `take_oauth_session` compares
  `expires_at > $3::timestamptz` using a `now: &str` passed by the caller, not `now()` from the
  transaction. GUESS: a skewed or frozen caller clock accepts expired states or rejects valid
  ones; an unparseable string surfaces as `Unavailable` with no boundary validation.
- **T2 — free-form `auth_state` and duplicated `'connected'` literal (CONFIRMED).** `set_auth_state`
  accepts any `&str`; `'connected'` is hard-coded in `store_bearer` and in `catalogue_tools`'s
  `WHERE s.auth_state = 'connected'`. GUESS: a typo or a future rename silently drops tools from
  the catalogue while the record still looks connected.
- **T3 — `tool_count` can diverge from `mcp_server_tools` (CONFIRMED).** `replace_tools` never
  updates `tool_count`; it is set only by a separate `set_auth_state` call. GUESS: a refresh that
  skips `set_auth_state` leaves a stale count in every listing/summary.
- **T4 — silent no-op writes (CONFIRMED).** `set_auth_state` and `store_bearer` ignore
  `rows_affected`, so an unknown id or an id hidden by RLS returns `Ok(())` while nothing changed.
  `replace_tools` on a missing server deletes 0 rows and inserts 0 tools, also returning `Ok(())`.
  Callers believe the config was applied (GUESS: stale UI state that never reconciles).

### 3. Duplicate upserts

- **D1 — last-writer-wins OAuth sessions (CONFIRMED).** `store_oauth_session` uses
  `ON CONFLICT (server_id) DO UPDATE`. Two concurrent authorization attempts for one server
  overwrite each other's `state`, `sealed_verifier` and `expires_at`; the first callback then
  fails the state check in `take_oauth_session` (GUESS: intermittent, load-dependent login
  failures). There is no uniqueness check on `state`, so a collision clobbers another flow.
- **D2 — destructive read with litter cleanup (CONFIRMED).** `take_oauth_session` DELETEs with
  `RETURNING`, then on a miss DELETEs again by `(server_id, state)`. A concurrent `store_oauth_session`
  can be erased by the cleanup path. The transaction keeps the decrypt failure safe (rollback
  restores the row), but the second DELETE is not conditional on the row still being expired.
- **D3 — conflict cause is opaque (CONFIRMED).** `create` maps *any* unique violation to
  `Conflict`; a future unique index on `(space_id, name)` would read as a duplicate id to clients.

## Concrete hardening (with risks)

1. **H-S1:** in `create`, `store_bearer` and `store_oauth_session`, reject writes when
   `!secrets.stable_key()` with a distinct typed error (never a warn-only path) — keep the current
   ephemeral behaviour only under a `cfg(test)`/dev flag. Risk: breaks local/dev smoke tests that
   currently rely on the fallback; needs a documented dev opt-in and the `.env.example` updated.
2. **H-S2:** add a typed `AppError::KeyMismatch` (or dedicated variant) for DEK-unwrap/auth failures,
   logged at `error` without key material; leave DB errors as `Unavailable`. Risk: new public error
   variant is an externally visible API change — record in `docs/PARITY.md` (DEVIATION) before ship.
3. **H-S3:** after fetching a row, assert the row's `space_id` equals the member context's space
   before sealing/unsealing; keep the RLS defence in depth. Risk: system contexts legitimately
   cross tenants — the check must be skipped only for `TenantContext::System` and justified in a test.
4. **H-T1:** replace caller `now` with SQL `now()` (transaction timestamp) for the expiry predicate,
   and drop the `&str` time parameter from the port if callers no longer need it. Risk: port
   signature change ripples through app/api crates and their tests; time-freezing tests must inject
   a clock instead of a string.
5. **H-T2/T3:** introduce a closed `AuthState` newtype/enum and use one shared constant for the
   connected value; have `replace_tools` set `tool_count = <inserted rows>` in the same transaction.
   Risk: stricter parsing can reject previously accepted arbitrary state strings (DEVIATION check).
6. **H-T4:** check `rows_affected()` in `set_auth_state`, `store_bearer` and `replace_tools` and
   return `NotFound` on zero. Risk: callers that currently rely on the silent success will start
   seeing errors — audit all call sites first.
7. **H-D1/D2:** key OAuth sessions on `(server_id, state)` and keep at most one *live* session per
   server via a partial unique index; make the litter cleanup `... AND expires_at <= now()` so it
   cannot delete a fresh session. Risk: schema migration + a constrained callback flow; must not
   weaken CSRF state verification.
8. **H-D3:** inspect the database constraint name on unique violation and map duplicate-id vs
   duplicate-name to distinct errors. Risk: couples the adapter to migration naming — use a stable
   named constraint.

## Suggested REQ-tagged tests

`req_033_s1_ephemeral_key_refuses_to_seal`, `req_033_s2_rotated_master_key_reports_key_mismatch`,
`req_033_t1_expired_oauth_state_rejected_using_db_clock`,
`req_033_t3_replace_tools_updates_tool_count_atomically`,
`req_033_t4_set_auth_state_missing_id_returns_not_found`,
`req_033_d1_concurrent_oauth_sessions_do_not_clobber`,
`req_033_d2_litter_cleanup_preserves_fresh_session`, `req_033_replace_tools_rolls_back_on_failed_insert`.
