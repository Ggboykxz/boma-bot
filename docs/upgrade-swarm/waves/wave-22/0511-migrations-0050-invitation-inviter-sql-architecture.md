# Wave 22 / Worker 0511 — `migrations/0050_invitation_inviter.sql` (LENS: architecture)

Proposal for v0.4.0. Additive only: every change below adds a column, index, table, view or
comment; nothing already applied is rewritten, dropped, back-filled or made non-nullable.

## Current structure (CONFIRMED — read this session)

`0050_invitation_inviter.sql` is a 7-line forward-only migration for REQ-042. Its entire body is:

```sql
ALTER TABLE invitations
    ADD COLUMN inviter_user_id text REFERENCES users (id) ON DELETE SET NULL;
```

Related surface read this session (CONFIRMED):
- `invitations` is created in `0002_m1.sql` (`id`, `space_id` FK spaces CASCADE, `email`,
  `token_hash bytea`, `role`, `expires_at`, `status default 'pending'`, `created_at`). It is
  `FORCE ROW LEVEL SECURITY` with one tenant policy keyed on `space_id`, plus indexes on
  `token_hash` and `space_id` only.
- `users` (`0002_m1.sql`) is global, explicitly *not* RLS-protected; its only name field is
  `name text NOT NULL` (GUESS: this is the "display name" the migration header refers to —
  REQ-042 does not name a field).
- REQ-042 (`spec/01-requirements.md:340-346`) says the token is the sole credential and the invite
  page offers only the space name and the inviter's display name; all other fields stay invisible.
- `0052_invite_accept_audit.sql` added `security.invite_accepted` to the audit kind set; there is
  no `invite.issued` actor event, so legacy rows cannot be back-filled from audit data.
- `0051_lowercase_users_email.sql` normalises `users.email` because the invitation flow compares
  caller and invited addresses byte-for-byte. Sibling additive style: `0047/0048` use
  `DEFAULT now()`, partial indexes and system-only RLS policies.

## Architectural assessment

This single FK is the only structural bridge from a tenant-scoped row (`invitations`, RLS by
`space_id`) to the global `users` table. The invite read path is anonymous and token-only, so that
bridge is also a privilege boundary: any widening of the projected fields becomes an account-data
leak. Three structural gaps follow. (1) `NULL` conflates "issued before this migration" with
"inviter account deleted" — `ON DELETE SET NULL` silently discards attribution. (2) There is no
index on the new column, so both `ON DELETE SET NULL` (FK child scan) and any future "invitations
issued by X" lookup fall back to a sequential scan. (3) Nothing records *what* was offered to the
invitee; reading `users.name` live means a rename or deletion changes the answer after the fact.
All upgrades below close these gaps additively, without changing the applied migration or the
public interface names.

## Concrete upgrades (additive only)

### 1. `COMMENT ON COLUMN` contract with explicit NULL semantics
Add `COMMENT ON COLUMN invitations.inviter_user_id` stating: "Set at issue time (REQ-042); `NULL`
means issued before 0050 or inviter deleted (`ON DELETE SET NULL`). The anonymous invite path may
project only `users.name` and the space name — never `id`, `email` or any other user field."
- **Risk**: low. Metadata only; risk is semantic drift if the wording later contradicts REQ-042.
  Mitigation: quote the requirement id and keep the projection allow-list verbatim.

### 2. Additive partial index for the FK and issuer lookups
`CREATE INDEX invitations_inviter_user_id_idx ON invitations (inviter_user_id) WHERE inviter_user_id IS NOT NULL;`
in a new numbered migration. The partial predicate keeps the index small; it covers FK delete
checks and issuer-scoped admin queries without touching existing indexes.
- **Risk**: medium. SQLx applies each migration file in a transaction, so `CREATE INDEX
  CONCURRENTLY` is unavailable there; the build takes an `ACCESS EXCLUSIVE` lock. Mitigation: ship
  as a separate file, keep it partial, and if online build is required use the runner's
  `--no-transaction` option (confirm support before relying on it). GUESS until confirmed.

### 3. Additive immutable display-name snapshot column
Add `inviter_display_name text` (nullable, no default). At issue time the writer copies `users.name`
into it; the invite read projects the snapshot instead of joining `users` live. This enforces
least privilege at read time and preserves "who invited you" after deletion/rename.
- **Risk**: medium. Duplicates a value; a rename no longer changes the invite page (arguably
  correct, but a behaviour nuance — review as a possible DEVIATION). The migration only adds the
  column; the writer/read change lives in the app layer and is out of this file's scope. Mark the
  projection switch GUESS until the adapter is read.

### 4. Additive append-only issuance ledger (optional, stronger than item 3)
`CREATE TABLE invitation_issuance (invitation_id text PRIMARY KEY REFERENCES invitations (id) ON
DELETE CASCADE, inviter_user_id text, inviter_display_name text, issued_at timestamptz NOT NULL
DEFAULT now())` with `FORCE ROW LEVEL SECURITY` and a system-only policy, mirroring `0048`. Gives
deletion-proof attribution and an audit-grade answer to "who issued this token".
- **Risk**: medium. Requires a writer change and creates a second source of truth; if item 3 is
  adopted this may be redundant — choose one (ledger for audit, snapshot for cost). Must not store
  the invited address or token in the ledger, per REQ-042's no-leak rule.

### 5. Additive narrow read projection for the anonymous invite path
Expose exactly the two permitted fields through an app-layer read port; if SQL is preferred, a
`security_barrier` view with a pinned `search_path`, `REVOKE ... FROM PUBLIC` and a grant to a
dedicated invite role. No other column becomes reachable to an anonymous caller.
- **Risk**: high if implemented as a SQL `SECURITY DEFINER` view — it can bypass RLS and is a
  privilege-escalation surface. Mitigation: prefer the app port; if a view is used, pin
  `search_path`, mark `security_barrier`, grant minimally, and test under an anonymous role
  asserting that email/hash/id are unreadable.

## Additive-only guarantee

Items 1-5 add only comments, an index, columns, a table or a view/grant. No `DROP`, no
`ALTER COLUMN ... TYPE`, no destructive `UPDATE`, no back-fill of existing rows and no tightening
of nullability. Every future change stays in a new numbered file (e.g. `0060_invitation_indexes.sql`,
`0061_invitation_inviter_contract.sql`) to preserve the applied-migration hash chain; `0050` itself
is not edited.

## Verification before merge

- Apply all migrations to a fresh DB and to a copy seeded with the current schema; assert row
  counts and existing column values are unchanged after each new file.
- For item 2, confirm with `EXPLAIN` that issuer lookups use the new index and that deleting a
  user no longer seq-scans `invitations`.
- For item 5, run an anonymous-role probe that attempts to read `email`, `token_hash` and `id` and
  confirm each is refused; record counts in `docs/STATE.md`.
- Mark every claim CONFIRMED or GUESS per workspace rules; GUESS items above are the display-name
  field mapping, the concurrent-index option and the projection switch.

Status: proposal only — no file outside this report was modified.
