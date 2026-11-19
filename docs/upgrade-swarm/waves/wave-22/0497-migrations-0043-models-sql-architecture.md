# Worker #497 — `migrations/0043_models.sql` — architecture proposal for v0.4.0
- Worker #497 · Lens: architecture (structure, boundaries, coupling, schema debt) · Target: v0.4.0.
- Status: PROPOSAL, READ-ONLY. No migration or product code changed and no tests run. CONFIRMED =
  read directly in this worktree this session; GUESS = inference or a claim not re-verified here.
- Constraint honored: **additive only**. 0043 is recorded and sqlx checks its checksum, so every
  change below ships as a NEW numbered migration (0060+, since 0059 is the current high-water mark)
  adding columns, tables, constraints, views or comments — no edit of 0043, no DROP / RENAME /
  ALTER TYPE / destructive row rewrite.

## Current structure (CONFIRMED unless marked)

- `model_credentials(space_id, provider, status, ciphertext bytea, nonce bytea, created_at,
  updated_at)`, PK `(space_id, provider)`, REQ-052, sealed under the space DEK "exactly like a bot
  secret" (header :1-6). `status` defaults to `'connected'` and its CHECK admits only that value — the header says failed connects store nothing and `invalid` is never stored.
- RLS: ENABLE + FORCE + `model_credentials_tenant_isolation` with the repeated
  `space_id = boma.current_space_id() OR boma.is_system()` predicate (:20-24), verbatim boilerplate
  from 0025/0026 and ~20 other tenant migrations (same finding as wave-21 #463/A1).
- `user_model_preferences(user_id PK → users, model_key, updated_at)` (:29-33): deliberately NOT
  tenant scoped because a preference is the person's across spaces (header :26-28); consequently no
  RLS and no user-scoping predicate.
- Structural gaps against the contract: spec/02:140 lists `ModelOAuthSession / UserModelCredential`
  (provider, sealed tokens, expiry) and spec/02:154 lists `SpaceModelPreference`,
  `SpaceModelRoleChoice`, `UserModelRoleChoice` (roles chat/helper/review/voice) — none exist in 0043
  apart from the single `model_key` preference. REQ-052 requires refreshable provider OAuth and a
  per-account credit cap; REQ-015 requires deployment / user BYOK / OAuth credential sources with
  usage attributed to the user. GUESS: DEC-09 (BYOK-only v1 with future interfaces) may defer the credit/OAuth surfaces — confirm scope with the owner before building beyond structure.
- No `format_version`, `key_id`, `purpose`/AAD, `expires_at`, `refresh_*`, `validated_at` or length CHECK exists: re-keying or domain separation can only be a big-bang rewrite, which additive-only forbids (shared finding with #463 bot_secrets and #475 mcp_credentials, ADR-020/DEC-10).
- Naming diverges from 0006 (`provider`/`model_key` vs `model_provider`/`model_id`); wave-20 #425 flagged the same bridge, and no `COMMENT ON` documents the equivalence. `status DEFAULT 'connected'` lets a plain INSERT claim a live validation the header says must precede storage — the invariant is a Rust convention, not a schema fact.

## Guiding principle for v0.4.0

Freeze 0043 byte-for-byte and add forward-only structure around it. Keep the sealed-credential
shape byte-compatible, keep the tenant predicate and the `is_system()` escape hatch intact, and make
every new capability an optional, defaulted/NULL-first addition so existing rows read identically.
Any storage-format or API-visible change is a DEVIATION for `docs/PARITY.md` with owner approval.

## Concrete upgrades (5, additive)

1. **U0 — Freeze 0043 + shape/immutability contract test, then a naming-bridge comment.** Never edit
   the file; add a fresh-PG test running `boma migrate`, asserting the exact two-table shape and that
   a hand-edited checksum is rejected on an already-migrated DB. Add `COMMENT ON COLUMN` in a new
   migration stating `provider`/`model_key` name the same catalogue entries as `model_provider`/
   `model_id` in 0006 (old names kept for public-interface compatibility).
   *Risk:* a shape assertion must be updated in the same commit as U1-U4; the equivalence is GUESS
   until the adapters confirm it. *Mitigation:* assert against one shared expected-shape constant,
   regenerate `.sqlx`, mark the comment GUESS in `docs/STATE.md` if unconfirmed. Test name `req_052_0043_migration_is_immutable_and_shape_stable`.

2. **U1 — Self-describing envelope columns, NULL-first.** New migration adds nullable
   `credential_format smallint DEFAULT 0`, `credential_key_id bytea`, `credential_purpose text` to
   `model_credentials` (and to the OAuth table from U2), with a version CHECK ∈ {0,1}; app writes v1
   with AAD binding version ‖ key_id ‖ purpose ‖ space_id ‖ provider, reads v0 for legacy rows.
   *Risk:* persisted format change → DEVIATION entry + approval; wrong AAD makes data unreadable;
   dual-read doubles the test matrix. *Mitigation:* land test-first with a legacy round-trip golden
   and coordinate with #463/#475 so all DEC-10 tables share one format rule, not three.

3. **U2 — Credential provenance and OAuth refresh seam (coupling/debt).** PK `(space_id, provider)`
   cannot hold a deployment default, a user BYOK key and a provider OAuth login for the same
   provider at once, and stores no owner, refresh token or expiry (spec/02:140; REQ-015/052). Add a
   new RLS-protected `model_user_credentials(space_id, user_id, provider, source IN
   ('byok','oauth'), sealed ciphertext/nonce, expires_at, ...)` for per-person credentials, plus
   nullable `expires_at`, `refreshed_at`, `scope` on `model_credentials` for OAuth/space-level use;
   leave the existing PK untouched and use U1's envelope.
   *Risk:* two tables of similar shape can drift; resolver precedence changes are client-visible
   (REQ-015 forbids silently using another model), and new fields must not leak into the catalogue
   (golden `models-catalog.json` says no keys/account ids). *Mitigation:* one shared sealing helper,
   a parity test that pinned bot choice still wins and that usage is attributed to `user_id`, and a
   names-only read surface (U4). GUESS: exact OAuth/credit scope under DEC-09.

4. **U3 — Role-based preference inputs from the data model.** spec/02:154 expects space and role
   preferences (chat/helper/review/voice); 0043 has one credential-less `model_key` per user. Add new
   additive `space_model_preferences(space_id, role, model_key)` and
   `user_model_role_preferences(space_id, user_id, role, model_key)` tables with a role CHECK and
   tenant RLS, leaving `user_model_preferences` as the account default.
   *Risk:* new resolver inputs can silently switch models, which REQ-015 forbids; rollout could be
   read as adding a feature outside DEC-09's scope. *Mitigation:* additive inputs only, precedence
   documented as bot-pinned > space > user > deployment with roles narrowing *within* a level, and a
   REQ-015 contract test of resolution order; GUESS whether v0.4.0 ships role choices at all.

5. **U4 — Integrity constraints and a names-only read surface; close the `validated_at` gap.**
   Add `CHECK (octet_length(nonce) = 12)` / `CHECK (octet_length(ciphertext) >= 16)` as NOT VALID,
   then VALIDATE only after an audit query; add nullable `validated_at` (legacy NULL = pre-existing
   connected) so a row cannot silently assert live validation without changing the status CHECK.
   Add a `security_invoker` view `model_credentials_public` exposing only
   `(space_id, provider, status, connected boolean)` and point list/catalog paths at it (REQ-052:
   catalogue and credential list never carry the secret).
   *Risk:* nonce/tag sizes must match `secret_box.rs` (GUESS: 12-byte nonces per #463); a view that
   fails to replicate RLS leaks across tenants; column grants interact with FORCE RLS and the system
   role. For `user_model_preferences`, no `boma.current_user_id()` helper exists today (0002 defines
   only `current_space_id`/`is_system`), so user scoping is GUESS and needs an infra/GUC change.
   *Mitigation:* confirm sizes and `security_invoker` on the target Postgres, audit that no list path
   selects `ciphertext`/`nonce`, and add the user helper only with a cross-space parity test.

## Cross-cutting risks / sequencing

- Order **U0 → U1 → (U2, U3) → U4**: freeze/test, envelope, provenance and preferences, then
  integrity/read boundary. Each lands as a separate new numbered file; U1-U4 only after their
  ADR/DEVIATION entries are approved. U2/U3 are independent after U1.
- Never edit 0043 or backfill its rows; keep the `boma.is_system()` escape hatch and FORCE RLS
  semantics. Coordinate with #476 (reliability lens), #463/#475 (DEC-10 envelope), #425 (0006
  naming bridge) and the REQ-052 resolver work: this proposal owns migration structure only.
- Gate each step: apply all migrations to a fresh DB and a seeded copy (row counts unchanged),
  regenerate `.sqlx`, replay the model-catalog golden and REQ-015/052 tests with ids in test names
  (`tools/trace_check.py`), `cargo fmt --check` and clippy `-D warnings`. No new dependency needed.

## Out of scope

`ModelCreditAccount / ModelCreditEntry / ModelCreditLot` and per-account credit caps (DEC-09 scope
decision), Rust resolver/OAuth verifier logic, revocation or purge jobs, and any in-place edit of
0043. Mark every implementation claim CONFIRMED/GUESS in `docs/STATE.md` before shipping.

## Evidence

- CONFIRMED this session: `0043_models.sql:1-33`; `0006_model.sql:7-13`; `0025_bot_secrets.sql:7-17`;
  `0026_space_keys.sql:7-13`; `0029_mcp_servers.sql:8-56`; `0031_mcp_credentials.sql:6-13`;
  `0044_money.sql:7-58`; `spec/01:120-127,423-429`; `spec/02:132-157`; `spec/06:106-124`;
  `spec/golden/models-catalog.json:1-33`; migrations listing (high-water mark 0059); wave-20 #425, wave-21 #463/#475 proposals.
- GUESS (re-verify before implementing): DEC-09 scope for OAuth/credits/role choices; 12-byte nonce
  and 16-byte tag convention; `security_invoker` support on the target Postgres; existence of a
  `boma.current_user_id()` helper; equivalence of `model_key` and `model_id` identifier spaces.
