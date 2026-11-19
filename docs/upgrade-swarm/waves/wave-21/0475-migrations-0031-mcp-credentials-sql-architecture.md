# Worker #475 — `migrations/0031_mcp_credentials.sql` — architecture (v0.4.0)

- Worker #475 · Lens: architecture (structure, boundaries, coupling, debt) · Target: v0.4.0.
- Status: PROPOSAL, READ-ONLY. No migration or product code changed, no tests run. CONFIRMED =
  direct read this session; GUESS = inference or a claim from a sibling proposal not re-verified here.
- Module: `0031_mcp_credentials.sql` (13 lines), the REQ-033/REQ-046 additive extension to `mcp_servers`
  that adds seven nullable columns for the two credential kinds a server may hold (bearer token, OAuth
  client secret + endpoints).
- Read for context (CONFIRMED): `0029_mcp_servers.sql` (base table, `auth_state`, RLS, `mcp_oauth_sessions`),
  `0025_bot_secrets.sql` / `0026_space_keys.sql` (sealing envelope house style), `0032_mcp_server_tools.sql`
  (additive-extension style), `spec/01:271-277` (REQ-033), `spec/01:373-379` (REQ-046),
  `spec/06:116-125` (DEC-10), `spec/05` D-05 (per-space private-endpoint audit), sibling wave-21 #463
  (bot-secrets envelope) and #449 (migration-immutability lens).

## 1. Current structure (CONFIRMED)

- One `ALTER TABLE mcp_servers ADD COLUMN ...` of seven nullable columns, in three loose groups:
  `bearer_token bytea` + `bearer_token_nonce bytea`; `oauth_authorization_url text`, `oauth_token_url text`,
  `oauth_client_id text`; `sealed_client_secret bytea` + `sealed_client_secret_nonce bytea`.
- No `IF NOT EXISTS`, no CHECK, no index, no COMMENT, no new RLS policy and no GRANT/REVOKE. The columns
  inherit the FORCE RLS policy declared in `0029` (`mcp_servers_tenant_isolation`, `space_id`-based).
- Four architectural traits:
  1. **Credential kind is not representable in the schema.** The header says a server holds at most one
     kind "the use cases decide which"; no CHECK or discriminator enforces that. `mcp_servers.auth_state`
     (`none|oauth_pending|connected|error`, from `0029`) could play that role but is not tied to these
     columns, so the invariant lives only in Rust.
  2. **No self-describing envelope.** Two ciphertext/nonce pairs carry no `format_version`, `key_id` or
     `purpose`/AAD metadata, unlike the explicit `space_keys`/`bot_secrets` convention; re-keying or
     domain separation can only be a big-bang rewrite, which additive-only forbids.
  3. **Partial writes are representable.** Nothing requires a nonce iff its ciphertext exists, and no
     `octet_length` CHECK bounds the bytea; a token with a NULL or wrong-size nonce decrypts only at
     runtime. Plaintext-looking `text` columns sit beside ciphertext in the same row, so a `SELECT *`
     in a log or audit path can sweep secret material with ordinary metadata.
  4. **No lifecycle/audit seam.** No `updated_at`/`rotated_at`/`expires_at` on the credential set, no link
     to the `0030` audit events (D-05 audits private-endpoint use, not credential changes). Revocation or
     rotation is invisible to SQL and to an incident query.

## 2. Guiding principle for v0.4.0

`0031` is immutable: the header itself states recorded migrations are never edited, and sqlx verifies a
checksum, so editing its bytes breaks `boma migrate` on deployed databases. Every change ships as a NEW
additive file (`0060_...` onward, since `0059` is the current high-water mark) that only adds columns,
constraints, indexes, views or metadata — never DROP / RENAME / ALTER TYPE / row rewrite. Keep the seven
column names, types and nullability, the `0029` RLS expression and the sealing format byte-compatible; any
externally visible behavior change is a DEVIATION for `docs/PARITY.md` with owner approval.

## 3. Concrete upgrades (5), each with a risk

**U1 — Freeze `0031` and add a migration shape/contract test (do first).** Never edit the file; add a
fresh-PG test running `boma migrate`, asserting the exact `0031` shape (seven nullable columns with exact
types, no extra constraints) and that a hand-edited checksum is rejected on an already-migrated DB.
*Risk:* a shape assertion must be updated in the same commit as any new column from U2/U3. *Mitigation:*
assert semantically against one shared expected-shape constant and regenerate `.sqlx` offline metadata
whenever a migration lands.

**U2 — Enforce one-credential-kind with an additive CHECK + discriminator (coupling).** New migration:
`ADD COLUMN credential_kind text CHECK (credential_kind IN ('none','bearer','oauth'))`, a CHECK that the
matching group is populated and the other is NULL, `NOT VALID` first, then `VALIDATE` after an audit
query over existing rows. *Risk:* legacy rows that hold both groups (GUESS: possible from the "use cases
decide" wording) make VALIDATE fail, and a default ties the column to `auth_state` semantics that may
drift. *Mitigation:* default to NULL (unchecked legacy), document `auth_state` as authoritative, add a
parity test of `credential_kind` vs `auth_state`, and only then tighten — never a row rewrite.

**U3 — Self-describing envelope columns, nullable-first (structure).** New migration adds nullable
`credential_format smallint DEFAULT 0`, `credential_key_id bytea`, `credential_purpose text` to
`mcp_servers`, with a CHECK on version ∈ {0,1}; app writes v1 with AAD binding version, key id, purpose,
`space_id` and server id, reads v0 for legacy rows. *Risk:* a persisted storage-format change → DEVIATION
entry and owner approval; wrong AAD makes data unreadable; dual-read doubles the test matrix.
*Mitigation:* land test-first with a legacy round-trip golden and coordinate with #463 (same DEC-10
envelope) so the two tables share one format rule rather than two.

**U4 — Cryptographic-column integrity and a names-only read surface (boundary).** New migration adds
`CHECK (octet_length(bearer_token_nonce) = 12)` and the analogous checks for the secret pair, `NOT VALID`
then `VALIDATE`, plus a `security_invoker` view `mcp_servers_public` exposing metadata and the two URLs but
no ciphertext/nonce, with list queries pointed at it. *Risk:* the nonce/tag size assumption must match
`secret_box.rs` (GUESS: 12-byte nonces per #463); a `security_invoker` view must replicate RLS or it leaks
cross-tenant data, and column grants interact with FORCE RLS and the system role. *Mitigation:* confirm sizes
and `security_invoker` on the target Postgres, audit that no list path still selects the bytea columns,
and leave the sealed columns readable only on the narrow reveal path.

**U5 — Rotation/audit metadata without deletion (lifecycle).** New migration adds nullable
`bearer_expires_at`, `credential_rotated_at` and a generation counter, plus a partial index on
`credential_rotated_at`; an audit event on credential write/rotate is an app change documented in the
migration header, not a table rewrite. *Risk:* TTL/generation semantics duplicate the Rust validity rule
and the `0030` audit contract; an aggressive expiry view could hide a still-usable credential. *Mitigation:*
leave the columns NULL/unused in v0.4.0 (structure only), state that purge belongs to a scheduled job with
an ADR, and add a parity test between the SQL view of credential state and the store resolver.

## 4. Cross-cutting risks / sequencing

- Order **U1 → U2 → U3 → U4 → U5**: freeze/test, then the kind invariant, envelope, integrity/read boundary,
  lifecycle. Each lands as a separate additive file; U2-U5 only after their ADR/DEVIATION entries are approved.
- Coordinate with #476 (reliability lens on this same file), #463/ADR-020 (envelope and nonce sizes), and the
  REQ-033 store work: this proposal owns only migration structure; resolver logic, probe/tool flows and D-05
  private-endpoint auditing stay in those briefs.
- Freeze the seven column names/types, the `0029` RLS predicate and the sealing format. Regenerate `.sqlx`,
  run `boma migrate` on a fresh DB in CI, and replay the REQ-033/REQ-046 tests. No new dependency; gate with
  `cargo fmt --check`, clippy `-D warnings`, golden and contract suites. Mark every claim CONFIRMED/GUESS.

## 5. Out of scope

`mcp_oauth_sessions` (its verifier/sealer and expiry), the `ScreenCapability`-style stores and resolvers,
`0030` audit-event emission, any in-place edit of `0031`, and any data backfill or purge are excluded — each
belongs to another brief or is forbidden by the checksum invariant.

## 6. Evidence & suggested tests

- CONFIRMED this session: `0031` (:1-13), `0029` (:8-56), `0025`, `0026`, `0032`, `spec/01:271-277`,
  `spec/01:373-379`, `spec/06:116-125`, wave-21 #463/#449.
- GUESS (re-verify before implementing): 12-byte nonce convention, current high-water migration `0059`,
  legacy rows holding both credential groups, `security_invoker` behavior on the embedded Postgres.
- Suggested names: `req_033_0031_migration_is_immutable_and_shape_stable` (U1),
  `req_033_credential_kind_matches_auth_state` (U2), `req_046_mcp_credential_round_trip_v0_v1` (U3).
