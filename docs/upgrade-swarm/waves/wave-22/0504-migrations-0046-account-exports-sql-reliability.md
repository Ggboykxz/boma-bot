# Wave 22 / worker 504 — `migrations/0046_account_exports.sql` — reliability proposal (v0.4.0)

Status: PROPOSAL, additive only. No file under `migrations/` is edited in place; every change below
is a later, forward-only migration (plus application work called out explicitly). Evidence: the
migration itself, its neighbours (`0048_password_resets.sql`, `0036_artifacts.sql`,
`0025_bot_secrets.sql`, `0002_m1.sql`), `spec/01-requirements.md` REQ-044/REQ-046,
`spec/04-nonfunctional.md` (secrets write-only; exports carry no secret material) and
`spec/golden/export-start.json`. Source facts CONFIRMED by reading; no runtime test was run here.

## 1. What exists today

`account_exports` is a user-scoped, non-tenant job row: `id` (text PK), `user_id` FK with ON DELETE
CASCADE, `status` in pending/ready/failed, `payload_bytes`, `skipped text[]`, `created_at`,
`updated_at`, one `(user_id, created_at DESC)` index, RLS ENABLE + FORCE and a single system-only
policy. The archive bytes are not described by the table at all. `boma.is_system()` is true for any
non-empty `boma.system_reason` setting (0002_m1.sql:38); there is no `boma.current_user_id()`
helper, so all ownership enforcement lives in the application.

## 2. Failure modes

### 2.1 Secret storage / archive confidentiality
- FM-1 No `storage_key`/content hash. The table says `status='ready'` but records neither where the
  zip lives nor a digest. A truncated, overwritten or wrong-user blob still reads as `ready`.
- FM-2 Undefined download capability. `id` is the only handle. If a download route trusts `id`
  alone, `id` is a bearer secret; audit logs recording ids then become a secret store. The golden
  fixture (`exp_golden`) sets no format/entropy requirement, so short or sequential ids are possible.
- FM-3 No-secret-in-export invariant is unenforceable by this table. `bot_secrets` holds AES-GCM
  ciphertext (0025_bot_secrets.sql); if a builder serialises raw columns instead of the documented
  layout, secret material lands in an object. No DB marker records that redaction ran.
- FM-4 `user_id ON DELETE CASCADE` removes the row, not the out-of-band object. A GDPR deletion
  (REQ-044) can leave a complete account snapshot in storage forever — the worst kind of orphan.

### 2.2 Expiry / lifecycle
- FM-5 No `expires_at`, no TTL. A `ready` archive stays downloadable indefinitely: stale PII plus
  growing storage cost.
- FM-6 No `started_at`/lease. A worker crash leaves `pending` forever; a sweeper cannot tell
  "running" from "abandoned", so no retry and no `failed` terminal state.
- FM-7 `updated_at` is app-maintained (no trigger anywhere in this migration set). A code path that
  forgets to bump it makes any lag-based watchdog silently mis-measure.
- FM-8 No `CHECK (payload_bytes >= 0)`. A buggy/signed counter is accepted, skewing quota reports.
- FM-9 Status transitions are unconstrained: `ready -> pending` is legal, so a stale worker can
  resurrect a finished export and overwrite a good blob.

### 2.3 RLS / ownership boundary
- FM-10 System-only policy means every read/write must carry `boma.system_reason`. A cleanup or
  reporting job that forgets it sees zero rows — a silent no-op, not an error. FORCE RLS also binds
  the table owner, so manual psql maintenance is blind by default.
- FM-11 `boma.is_system()` checks a non-empty setting, not a validated reason. Any code able to set
  the GUC reads every account's export; the closed enum is an application convention only.
- FM-12 With no owner policy, one missed ownership check on any download/listing route leaks a whole
  account. It is a one-mistake boundary with no database backstop.

## 3. Additive hardening (later migrations)

H1 (FM-1, FM-4): `ADD COLUMN storage_key text`, `ADD COLUMN content_hash text`, plus
`CONSTRAINT account_exports_ready_has_blob CHECK (status <> 'ready' OR
(storage_key IS NOT NULL AND content_hash IS NOT NULL)) NOT VALID` → backfill → `VALIDATE`.
Keep the object in the private bucket and verify the digest on download. Add a sweeper that deletes
blobs for rows whose user is gone (FM-4) before dropping metadata, emitting an audit event.

H2 (FM-2, FM-12): `ADD COLUMN download_token_hash bytea`, unique partial index
`... ON account_exports (download_token_hash) WHERE download_token_hash IS NOT NULL`. Store only the
hash, as 0048_password_resets.sql does; hand the raw token to the client once. Add
`CONSTRAINT account_exports_id_entropy CHECK (length(id) >= 24) NOT VALID` so new ids cannot be
guessable; never validate it against legacy rows.

H3 (FM-5, FM-6): `ADD COLUMN expires_at timestamptz`, `ADD COLUMN started_at timestamptz`. Backfill
`expires_at = created_at + interval '24 hours'` for legacy rows, then `SET NOT NULL`. Index
`(status, expires_at)` — NOT a partial predicate on `now()`: `now()` is STABLE, not IMMUTABLE, so
`WHERE expires_at > now()` is rejected by CREATE INDEX. Sweeper: pending with `started_at` older
than the lease → `failed`; ready/failed past `expires_at` → delete row + blob.

H4 (FM-7..FM-9): `ADD COLUMN finished_at timestamptz`; `CHECK NOT VALID` that `finished_at IS NULL`
iff `status='pending'`, plus `CHECK (payload_bytes >= 0) NOT VALID`. A trigger to sync `updated_at`
deserves its own ADR — it would be the first such trigger in this migration set.

H5 (FM-10..FM-12): add `SECURITY DEFINER` helper
`boma.account_export_for_user(p_id text, p_user_id text)` with `SET search_path = boma, public`,
selecting only `id = p_id AND user_id = p_user_id AND status='ready' AND expires_at > now()`,
granted solely to the app role; route download reads through it. Keep the existing policy intact.
This adds a DB-level ownership backstop without changing the system-only read model or bolting a
tenant policy onto a user-scoped table.

## 4. Risks / tradeoffs

- New columns take `ACCESS EXCLUSIVE` locks: short transactions, `lock_timeout`, maintenance window;
  `ADD ... NOT VALID` + `VALIDATE CONSTRAINT` avoids a long validation lock on a big table.
- Backfilling `expires_at` expires all legacy archives at once — acceptable, but announce it. A
  persistent DEFAULT using `now()` is a trap; make the application set the value explicitly.
- H2 changes the download contract (token instead of bare id): a DEVIATION for `spec/openapi.yaml`
  needing approval. H1/H3/H4 stay invisible when the application is updated in lockstep.
- H5 adds a privileged function: audit grants, pin `search_path`, and test that a member cannot pass
  another user's id. It does not fix FM-11 globally; that belongs to a separate ADR.
- Blob deletion is asynchronous: a deletion request is not complete until the blob is gone. Track
  the pending deletion in a queue/audit row, or REQ-044's "data is removed" is only aspirational.
- A shared `current_user_id()` helper would be cleaner than passing `p_user_id` to H5, but it is
  cross-cutting and out of scope here; record it as a proposal in docs/STATE.md.

## 5. Sequencing / questions

1. H1 + H3 columns in one migration (backfill, validate). 2. Sweeper and blob cleanup, then H4
constraints, then H5. 3. H2 only after the OpenAPI deviation is approved.

For the lead: is the download already session+ownership checked (if so, FM-2 is defence-in-depth)?
Where do archive bytes live today? Is a 24h TTL acceptable for REQ-044?
