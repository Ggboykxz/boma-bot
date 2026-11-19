# 0476 — migrations/0031_mcp_credentials.sql — reliability (v0.4.0)

Worker #476 · module `migrations/0031_mcp_credentials.sql` · lens: reliability · READ-ONLY review of
`rebuild-workspace/.worktrees/before-m5` at v0.3.0-era source (workspace says `0.3.0-rc5`). Proposals
only; no code or migration changed, no tests run. CONFIRMED = read this session; GUESS = inferred from
SQL + store code. Line refs are against this worktree.

Boundaries consulted (CONFIRMED unless tagged): `migrations/0029_mcp_servers.sql` (parent table, RLS
L25-29, `mcp_oauth_sessions` L32-56), `0025_bot_secrets.sql` L1-6, `0026_space_keys.sql` L1-6,
`0032_mcp_server_tools.sql`, `crates/infra/src/stores/mcp.rs` (seal-on-insert L104-149, reveal
L508-540, `store_bearer` L542-579), `docs/adr/020-secrets.md` (envelope + rekey L32-38, L55-58),
`spec/01-requirements.md` REQ-033 L271-277 / REQ-046 L373-379, `spec/04-nonfunctional.md` L83,
`spec/05-known-defects.md` D-02 L17 / D-05 L20. Runtime behaviors are GUESS (no tests run).

## What it already does right (keep, do not re-propose)

- Seven nullable columns on the parent (`0031` L6-13): a server holds at most one credential set;
  plaintext never stored (CONFIRMED). RLS inherited from `mcp_servers` (`0029` L25-29) — no new
  policy is needed for columns on the same tenant row (CONFIRMED).
- Ciphertext and nonce are written together in a single INSERT/UPDATE (`mcp.rs` L120-149, L563-574):
  no DB-level half-rotation under the current writer (CONFIRMED).
- Reveal is fail-closed — it requires BOTH `Some(ciphertext)` and `Some(nonce)` (`mcp.rs` L526) and
  returns `None` otherwise; a missing/partial credential does not crash the reader (CONFIRMED).
- Rekey rewraps DEKs only, never secret rows (ADR-020 L55-58): these columns need no rewrite on a
  master-key rotation — do not add per-credential re-encryption (CONFIRMED).

## Failure modes → concrete hardening

### F1. Ciphertext/nonce can diverge → silent, confusing auth failure

- Nothing ties `bearer_token`↔`bearer_token_nonce` or `sealed_client_secret`↔
  `sealed_client_secret_nonce`: a partial write or manual repair can leave ciphertext with a NULL
  nonce (or vice versa). Reveal then silently returns `None` (mcp.rs L526): a previously working
  server degrades to unauthenticated and the MCP endpoint 401s with no schema-level warning
  (CONFIRMED shape, GUESS on cause). P1. In a NEW migration (never edit 0031) ADD CHECKs:
  `(bearer_token IS NULL) = (bearer_token_nonce IS NULL)` and the same for the client-secret pair,
  plus `octet_length(nonce) = 12` for a GCM nonce when present (GUESS on nonce size from ADR-020).
  P2. Add `NOT VALID` first, then a read-only audit query for half-written rows, then `VALIDATE`.
  Risk: dirty rows block VALIDATE — repair by additive rewrite of the offending rows, never by
  relaxing the rule; under `FORCE RLS` the validation scan must run as a BYPASSRLS/is_system role or
  it sees only one tenant's rows (same trap as wave-21 #456 R1) (CONFIRMED mechanism).

### F2. `auth_state` can disagree with stored credentials

- `auth_state` vocabulary is pinned in 0029 L15 ('none'|'oauth_pending'|'connected'|'error'), but
  0031 adds no constraint tying it to the new columns: `connected` with no bearer and no OAuth
  session, or a bearer set on a `none` server, are both representable (CONFIRMED). `store_bearer`
  forces `connected` even for an OAuth-configured server (mcp.rs L566, GUESS on intent) — state and
  usable credential can drift, so health/ready probes and the UI misreport connectivity. P3. Add a
  CHECK (NOT VALID → VALIDATE as system) requiring `connected` ⇒ at least one credential set
  (bearer pair OR a row in `mcp_oauth_sessions`), and `bearer_token IS NULL` when `auth_state='none'`.
  Risk: OAuth-connected servers rely on the session table, not these columns, so the rule must
  consult `mcp_oauth_sessions` — cross-table CHECKs are not allowed in a CHECK constraint; use a
  BEFORE INSERT/UPDATE trigger or enforce at the store boundary + a test. Prefer the store/test path
  in v0.4.0 and keep the trigger as a gated Wave B item (DEVIATION if it changes accepted writes).

### F3. OAuth config columns are unbounded/uncoherent and split across RLS tables

- `oauth_authorization_url`, `oauth_token_url`, `oauth_client_id` (L9-11) are plain `text` with no
  length bound or all-or-nothing rule: an oversized/malformed value bloats storage and fails at
  runtime exchange parse; a partial config (token URL but no client id) strands an OAuth attempt in
  `oauth_pending` (CONFIRMED shape, GUESS on runtime effect). P4. Add length CHECKs (GUESS: URLs
  ≤ 2048, client_id ≤ 512) and a `((oauth_authorization_url IS NULL) = (oauth_token_url IS NULL))`
  coherence CHECK, NOT VALID → VALIDATE. P5. The exchange reads `sealed_client_secret` here and
  `sealed_verifier` from `mcp_oauth_sessions` (0029 L35) under two RLS policies — add an integration
  test that both resolve under one tenant context, and a read-only diagnostic VIEW joining them
  (data only, never authoritative, no new policy). Risk: VIEW can drift from Rust flow; bounds could
  reject an existing long-but-valid URL — audit max lengths before VALIDATE (GUESS on current data).

### F4. No rotation/staleness metadata; DEK cutover is all-or-nothing

- Credentials carry no `expires_at`/`rotated_at` (CONFIRMED), so an expired bearer token is only
  discovered by a failing outbound call — there is no partial index to drive a sweep and no way to
  report staleness. P6. Add nullable `bearer_expires_at timestamptz`, `credential_rotated_at
  timestamptz` and a partial index on `bearer_expires_at WHERE bearer_expires_at IS NOT NULL`;
  structure only in v0.4.0, no sweeper behavior (a DEVIATION needs an ADR + docs/PARITY.md).
  P7. Add an optional nullable `dek_version` column so a future space-DEK compromise can dual-read
  old/new ciphertext instead of a stop-the-world re-seal (master-key rekey is unaffected — ADR-020).
  Risk: P7 changes the seal/unseal contract and needs its own ADR; ship the column unused in v0.4.0.
  R2. Losing `BOMA_MASTER_KEY` loses all DEKs and therefore every credential (ADR-020 L72) — a
  migration cannot fix this; keep the backup warning in ops docs, do not add a DB-resident KEK.

## Explicit non-goals / rejected (additive only)

- No edit to 0031/0029, no DROP/RENAME/ALTER TYPE/DELETE, no row rewrites except audited repair,
  no weakening of the inherited RLS or an `is_system()` write bypass, no `SELECT *` on these columns
  in list/export paths (ciphertext is unreadable but unbounded — `mcp.rs` summary query already uses
  explicit columns, CONFIRMED). No outbound-secret logic or sweeper behavior in v0.4.0.

## Rollout (v0.4.0) and cross-cutting risks

- Wave A (safe, additive): P2 audit query; P1/P4 CHECKs as NOT VALID; P6 columns + partial index;
  P5 RLS-consistency test + diagnostic VIEW; regenerate `.sqlx` offline metadata. Wave B (gated):
  P1/P4 VALIDATE as system role after backfill proof; P3 boundary enforcement + trigger. Wave C
  (needs ADR + owner approval): P7 dek_version contract, P6 sweeper default-on.
- R1. Validate every constraint under BYPASSRLS/is_system or FORCE RLS hides other tenants' rows.
  R3. P3/P6/P7 alter visible behavior → DEVIATION, ADR, docs/PARITY.md, REQ-033/046-named tests.
  R4. nonce size (12) and URL length caps are GUESS until confirmed against real stored values.

## Suggested tests / evidence

- `req_033_mcp_credential_ciphertext_and_nonce_are_paired` (P1),
  `req_033_mcp_rls_blocks_cross_tenant_credential_read` (inherited RLS),
  `req_046_mcp_credential_survives_a_rekey` (ADR-020 behavior already covered),
  `req_033_oauth_exchange_reads_secret_and_verifier_in_one_context` (P5).
- CONFIRMED: 0031 L1-13, 0029 L15/L25-56, 0025/0026 headers, mcp.rs L104-149/L508-579, ADR-020
  L32-38/L55-58/L72, spec REQ-033/046, spec/04 L83, spec/05 D-02/D-05. All runtime outcomes GUESS.
- Status: proposal only; READ-ONLY worker, no code changed and no tests run. Line budget: <120.
