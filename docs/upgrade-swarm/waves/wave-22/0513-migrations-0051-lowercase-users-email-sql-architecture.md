# 0513 — migrations/0051_lowercase_users_email.sql — architecture (v0.4.0)

Worker #513 · module `migrations/0051_lowercase_users_email.sql` · lens: architecture · READ-ONLY
review of `rebuild-workspace/.worktrees/before-m5` at v0.3.0-era source. Proposals only; no code
changed. Line refs are against this worktree. Claims tagged CONFIRMED (read this session) or GUESS
(inferred); no tests were run (read-only). Boundaries consulted: the 7-line module itself,
`0002_m1.sql` (users L70–79, invitations L226–229), `0001_init.sql`, peer migrations
`0048_password_resets.sql`, `0049`, `0050`, `0052`, `0047_access_grants.sql`,
`crates/domain/src/account.rs` (`Email::new` L29–41), `crates/infra/src/stores/users.rs`
(`find_by_email` L53–63), `stores/mail.rs` (L218–225), `app/src/accounts.rs` (L133, L221–227),
prior swarm docs wave-14/0320 and wave-19/0410 (email canonicalization split).

## What it already does right (keep, do not re-propose)

- A one-shot data backfill in its own forward-only file, with the REQ id and the *why* in the
  header (L1–6); it never edits `0002_m1.sql` (CONFIRMED).
- It targets the real writer gap: new rows are normalized at `Email::new`
  (`account.rs:33` `trim().to_ascii_lowercase()`), old rows are not (CONFIRMED).
- Idempotent in effect: `lower(lower(x)) = lower(x)`, so a second run changes nothing (CONFIRMED).
- `users` is a global, non-RLS table (`0002_m1.sql:64–68`), so the backfill needs no BYPASSRLS
  dance — unlike the `FORCE ROW LEVEL SECURITY` cases in wave-20/21 (CONFIRMED).

## Architecture findings → concrete upgrades

### U1. The invariant lives in one runtime, not in the schema

- The canonical-email rule exists as application code only. `find_by_email` binds a value into
  `WHERE email = $1` (`users.rs:58`) and trusts `Email::new`; any raw-SQL/SCIM/maintenance writer
  can reintroduce mixed case, and 0051 will never run again (CONFIRMED shape, GUESS outcome).
  P1. In a NEW forward-only migration, add `CHECK (email = lower(email))` as `NOT VALID`, audit,
  then `VALIDATE CONSTRAINT users_email_is_lower`. The byte `UNIQUE` on `email`
  (`0002_m1.sql:72`) then becomes a true case-insensitive key with no extra column or index.
  *Risk:* `lower()` is Unicode-aware but `Email::new` lowercases ASCII only — `İ`/`ẞ` rows would
  be accepted by the domain and rejected by the DB (CONFIRMED divergence, see wave-19 F1). Either
  reject non-ASCII at the boundary (externally visible → DEVIATION + owner approval) or express
  the same ASCII rule in the CHECK. Do not widen the rule silently.

### U2. The backfill has no dedup or post-condition

- `UPDATE users SET email = lower(email)` (L7) has no preflight for case-variant twins:
  `A@x.com` and `a@x.com` both pass the byte `UNIQUE` today, so the update can abort on the
  constraint with an opaque error, or after a partial write (CONFIRMED shape, GUESS occurrence).
  P2. Re-run 0051's logic as an idempotent, self-verifying migration: guard with
  `WHERE email <> lower(email)`, add a preflight query that raises a clear exception listing the
  colliding ids, and a post-condition asserting zero remaining non-canonical rows.
  *Risk:* merging two accounts is a data decision (which `password_hash`, sessions, memberships
  win) — never auto-merge in SQL; gate on an ADR/owner choice. Keep 0051 unedited as history.

### U3. Three normalizers, one concept

- `Email::new` uses `to_ascii_lowercase`, 0051 uses SQL `lower`, and the mail membership check
  uses `lower(u.email) = $2` (`mail.rs:225`) while its input is `to_ascii_lowercase` (`L218`).
  Recovery (`accounts.rs:133`, L221–227), invitation acceptance (REQ-042) and mail membership
  therefore compare differently-cased values (CONFIRMED code, GUESS runtime outcome).
  P3. Define one canonicalization function in the DB (e.g. `boma.canonical_email`) and have every
  email-keyed column and lookup use it, with `Email::new` mirroring or calling the same rule; add a
  property test over ASCII case folding and the `İ`/`ẞ` edge cases.
  *Risk:* this is a cross-crate contract change; do not land before U1's rule is fixed, or the two
  layers will disagree faster. Preserve local-part behavior (RFC 5321 case-sensitivity) — changing
  it is a DEVIATION already flagged in wave-12/0240.

### U4. Canonicalization stops at `users`

- `invitations.email` (`0002_m1.sql:229`) and `access_grants.email` (`0047:7`) are plaintext email
  keys outside `users`, yet the header's guarantee is "caller address == invited address".
  P4. Additively extend the U1 rule to those two columns (`NOT VALID` → `VALIDATE` after audit),
  so both sides of the invitation check and the allow-list are canonical by construction.
  *Risk:* existing mixed-case rows block `VALIDATE`; audit and repair additively, never relax.
  `access_grants` and `password_reset_tokens` are `FORCE RLS` system-only tables
  (`0047:13–17`, `0048:18–22`), so run any validation as a BYPASSRLS/`is_system()` role.

### U5. The rule is undocumented as an ownership contract

- Nothing records *who* owns canonical email or that backfills are one-shot; wave-14 and wave-19
  independently proposed a second mechanism (`email_key` generated column), which would create
  two sources of truth (CONFIRMED in those docs).
  P5. Add an ADR + `docs/STATE.md` entry: canonical email = trim + ASCII-lower, enforced at
  `Email::new` and by the U1 CHECK; every writer must pass through one of them; 0051 is historical.
  Add REQ-040/042-named tests asserting `Person@Example.COM` and `person@example.com` resolve to
  one row and that both recovery and invitation lookup find it.
  *Risk:* docs alone enforce nothing — pair with U1/U4; reconcile with #320/#410 before building
  a parallel `email_key` column.

## Non-goals / rejected (additive only)

- No edits to 0051 or `0002_m1.sql`, no drops/renames/type changes, no weakening of `UNIQUE`.
  Every item is a new forward-only migration, function, constraint, doc or test.
- No automatic account merge, no trigger that repairs writes, no behavior change to sign-in,
  recovery or mail matching in v0.4.0 — those are DEVIATIONs needing owner approval.

## Rollout and cross-cutting risks

- Wave A (safe, additive): P2 guarded/verifying re-run, P1/P4 `NOT VALID` → `VALIDATE` under a
  system role, P5 REQ-tagged tests and ADR. Wave B (gated): U3 shared function + Rust alignment
  (DEVIATION if non-ASCII acceptance changes). Wave C (owner): any account merge or lookup change.
- R1. Preflight audits must run before every `VALIDATE`; dirty rows are a hard gate, not a reason
  to drop the constraint. R2. Coordinate with workers #320/#410 so only one canonical-key design
  ships. R3. Confirm the case-variant twin count in the current data (GUESS: likely zero) before
  choosing between a simple P2 re-run and a merge ADR.
