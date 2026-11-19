# 0402 — `crates/infra/src/stores/spaces.rs` — reliability lens — v0.4.0 proposal

Worker #402 · module `crates/infra/src/stores/spaces.rs` (259 lines) · lens: reliability (orphan
spaces, key provisioning, cascade deletes, error classification, clock/lock behavior) · READ-ONLY
review of `rebuild-workspace/.worktrees/before-m5` (v0.3.0-rc5). Spec anchors: REQ-042/043/044/046,
ADR-002. CONFIRMED = read this session; runtime likelihood/impact = GUESS. Proposal only; no code.

## 1. Current behavior (CONFIRMED)

- Eight `SpaceStore` methods. Each is one SQL statement (except `create`) in `tenant::as_system`
  (`boma.space_id=''`, closed-enum reason) or `tenant::in_space` (`boma.space_id=<id>`); settings are
  transaction-scoped via `set_config(...,true)` and reverted on commit/rollback (tenant.rs:75-140).
- `create` (:144-189): `in_space` transaction inserts `spaces` then the owner `space_members` row —
  the space/membership atomicity REQ-042 requires. No key row is written here.
- `add_member` (:54-93): `INSERT ... ON CONFLICT (space_id,user_id) DO NOTHING`; `rows_affected()==0`
  becomes `AppError::Conflict`. Reads (`name_of`, `spaces_of`, `member_ids_of`, `default_for`) run as
  system with Authentication/AccountExport reasons; `member_ids_of` maps any bad stored id to
  `Unavailable` (:138-142). `find`/`membership` are tenant-scoped JOINs.
- Schema: `space_members` PK (space_id,user_id), both FKs `ON DELETE CASCADE` (0002_m1.sql:133-139);
  `space_keys.space_id` and `bot_secrets` also cascade from space (0026:7-13, 0025:7-17). DEKs are
  minted **lazily** by `load_or_create_dek` (`ON CONFLICT DO NOTHING` + survivor re-read,
  space_keys.rs:23-62); `stores/secrets.rs:62-90` keeps a second inline copy "stable for the recorded path".
- `spaces_of`/`default_for` order by app-provided `created_at` (`default_for` tie-breaks by `id`, :246);
  all driver errors map to `Unavailable` (tenant.rs:153-156).

## 2. Failure modes

### 2.1 Orphan spaces (protection at create CONFIRMED; gap after member loss GUESS)
The transaction at :152-188 prevents a space with no members at creation. The invariant is not held
afterwards: this store has no remove/delete method, and `space_members.user_id ... ON DELETE CASCADE`
means deleting/deprovisioning the sole owner strips the last membership while the `spaces` row
survives. `find`, `membership`, `spaces_of` and `default_for` then hide it forever (no reaper, no
soft-delete state). A dangling/imported space id given to `add_member` fails at the FK and is reported
as `Unavailable`, indistinguishable from a DB outage.

### 2.2 Key provisioning is lazy and split across two paths (CONFIRMED)
`create` commits a usable-looking space with no `space_keys` row. A space can therefore be fully
created and populated, yet permanently unable to hold secrets if the master key is misconfigured at
first secret write — the failure surfaces much later and far from its cause. Two independent
load-or-create copies (shared helper vs inline in `secrets.rs`) both rely on `ON CONFLICT DO NOTHING`
+ survivor re-read (correct shape), but any drift between them can let a first-write race seal a
secret under a discarded DEK, orphaning ciphertext; `open_for_space` also mints on a read. Verify the
scheme stays byte-identical before dedup.

### 2.3 Cascade deletes are broad and irreversible (CONFIRMED shape)
One `DELETE FROM spaces` cascades memberships, bots, threads, `space_keys`, `bot_secrets` and every
other child (0002_m1.sql, 0025, 0026). For a large tenant that is an unbounded, lock-holding cascade
on one transaction (latency/cancellation risk); destroying the `space_keys` row makes every secret
cryptographically unrecoverable — no soft delete, no undo. There is no "last owner" guard, so a
well-intentioned user deletion can silently orphan a whole tenant (ties to 2.1). No partial-unique
index enforces one default space per organization, so `default_for` resolves ties by id, not intent.

### 2.4 Error classification and idempotency (CONFIRMED)
- `add_member`'s `Conflict` conflates "already a member — idempotent retry succeeded" with a real
  conflict, and cannot upgrade a role. An invite acceptance retried after a committed insert is
  reported as a failure: the DB op is idempotent but the observable one is not.
- `member_ids_of` turns one corrupt/dangling user id into whole-list `Unavailable` (REQ-044 export
  poisoning). FK makes this unlikely in normal operation, but restores/imports can bypass it.
- Every driver error — FK violation, unique violation, lock timeout, corrupt row — is `Unavailable`
  (a transient class clients retry). Permanent corruption never heals and retries amplify load.
- No structured tenant fields or statement timeout: a big-cascade lock wait hangs a request with no
  context. App `created_at`/`now` vs DB `now()` elsewhere can skew `spaces_of`/`default_for` ordering.

## 3. Concrete hardening for v0.4.0 (ordered by value/risk)

- **A. Provision the DEK in `create` (medium-high).** Pass `SecretBox` into the store and insert a
  wrapped key in the same transaction (or verify presence before returning success), so a created
  space is always secret-ready. *Risk:* a secrets outage now fails space creation — decide if
  acceptable (ADR + REQ-046 sign-off). Lighter: keep lazy mint but let `create_space` provision
  idempotently.
- **B. Last-owner guard + memberless-space reaper (medium).** Add `member_count`/`delete` support in
  this store; forbid removing the final owner (domain rule) and periodically flag/purge zero-member
  spaces. *Risk:* deletion policy is a REQ-044/045 question — do not guess; STATE + ADR.
- **C. Bounded, explicit space deletion (medium-high).** Replace a single cascading `DELETE` with
  staged, batched child deletes in one transaction, or soft-delete + async purge; add a partial
  unique index on `(organization_id) WHERE is_default`. *Risk:* migration + externally visible
  delete semantics — DEVIATION in `docs/PARITY.md`, owner approval.
- **D. Idempotent `add_member` (low-medium).** On conflict, `SELECT` the row: same role → `Ok(())`;
  different role → typed `RoleMismatch` (or explicit promotion path). *Risk:* retry behavior becomes
  observable as success instead of `Conflict` — note as DEVIATION if considered API-visible.
- **E. Typed errors, skip-and-warn reads (low).** Distinguish `NotFound`/`Conflict`/`DataCorruption`
  from `Unavailable`; make `member_ids_of` skip+`tracing::warn!` a bad id (or fail with typed
  corruption) instead of poisoning the export. Add `space_id`/`user_id` as structured log fields and
  a `SET LOCAL statement_timeout` for system-wide scans. *Risk:* new `AppError` variants ripple into
  API mapping (additive).
- **F. Unify DEK mint (low-medium).** Replace the inline copy in `stores/secrets.rs` with the shared
  helper, behind a golden ciphertext-compatibility test. *Risk:* the copy is deliberately "stable for
  the recorded path" — verify byte-for-byte scheme first; cross-module, coordinate with #398/secret
  owners.
- **G. One clock (low).** Validate app timestamp strings as UTC RFC3339 at the boundary or use DB
  `now()` for ordering columns. *Risk:* stored timestamp shape change — ADR if externally visible.

## 4. Risks / non-goals / gates

- `unsafe_code` forbidden; clippy `-D warnings`; no `unwrap`/`expect`/`panic` outside tests. Keep every
  method in its `as_system`/`in_space` transaction; never move `boma.space_id` out of tenant.rs
  (ADR-002). Do not rotate/rekey or log key material, wrapped or plain, in new paths.
- A/B/C and F/G change visible behavior or schema — record in `STATE.md` + `PARITY.md`, wait for
  owner approval; D/E are local and reversible. Sequence E+D -> G+F -> A -> B -> C.
- `tools/trace_check.py` counts declarations, not comments: name tests with REQ ids. Impact is GUESS
  until the integration tests run.

## 5. Suggested tests (integration, real PG)

- `req_042_failed_owner_insert_leaves_no_orphan_space`
- `req_042_add_member_retry_is_idempotent` and `req_042_role_upgrade_is_explicit`
- `req_042_memberless_space_is_flagged` (documents current orphan behavior until B lands)
- `req_046_created_space_can_store_a_secret` and `req_046_concurrent_first_seal_shares_one_dek`
- `req_044_member_ids_of_skips_corrupt_row_or_reports_typed_error`
- `req_043_space_delete_cascades_memberships_and_keys` and `req_044_delete_is_bounded`
- `req_042_created_at_ordering_survives_drifted_client_clock`

## Bottom line

`create` gets the hard invariant right — space and first membership commit together. Gaps start
afterwards: DEKs are minted lazily by two duplicated paths, one cascade can erase a tenant
irreversibly or orphan it via last-owner deletion, and `Conflict`/`Unavailable` hide benign retries
and permanent corruption alike. E+D+G+F are reversible wins; A/B/C need ADRs and owner sign-off.
Impact is GUESS until the integration tests run.
