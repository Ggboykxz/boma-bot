# 0162 · crates/app/src/accounts.rs · LENS=reliability · v0.4.0 proposal

## Scope note

`Accounts` orchestrates sign-up, sign-in and session minting; the free functions add password-recovery
issue/consume. Trusted rules live in `boma_domain::Email`/`validate_password`, IO in the `ports`
(hashers, token generator, stores). This reviews this module and the adapters it calls directly
(`infra::crypto`, `infra::stores::accounts`, `testkit::memory`, `usecases::ensure_local_owner`,
migrations 0002/0047/0051). CONFIRMED as read; runtime impact is GUESS until the tests under
"Success criteria" exist. Focus: the duplicate-owner race, recovery/hash write order, and the
ASCII-only email normalization.

## Current behavior (as read)

- `sign_up` normalizes via `Email::new`, validates the password, trims the name, hashes, then writes
  user+org+space+membership in one transaction and grants a session. No address pre-check; the
  store's UNIQUE constraint is the only guard (Postgres: `email text NOT NULL UNIQUE`, 0002:72).
- `sign_in` uses a decoy `hash` when the address is unknown (L139), but the `deactivated_at` branch
  (L143-146) returns with no hash/verify; a wrong password runs `verify` (L147-156).
- `grant_session` mints a 256-bit token, stores only its SHA-256 hash, returns the clear token.
- `request_password_reset` lowercases/trims again, emits a token + audit row, and uses a decoy hash
  for the unknown-address branch (L231); the response is byte-identical either way.
- `reset_password` validates the password, then `consume`s the token (expiry inside the statement),
  `revoke_all_for_user`, `hash`es the new password, `set_password`, deletes sessions, re-reads the
  user, audits, and grants a session — the steps after `consume` are **not** one transaction.
- `ensure_local_owner` (usecases.rs:160-212) is check-then-create for the fixed sentinel
  `local-owner@local.invalid`; on `AppError::Conflict` it re-reads by address once (L205).
- `Email::new` (domain/account.rs:29) uses `trim().to_ascii_lowercase()`; migration 0051 backfilled
  stored rows with Postgres `lower(email)`.

## Failure modes

1. **Duplicate owner: check-then-create TOCTOU with a single un-retried re-read.** Two concurrent
   first boots of `ensure_local_owner` both see no user (L162) and both burn an Argon2 hash on a
   random throwaway password (L170-174). The UNIQUE makes one `create_account` fail with `Conflict`;
   the loser re-reads once. Postgres raises the violation inside the winner's still-open
   transaction, so the loser's re-read can run before the winner commits and see nothing — a
   spurious `AppError::Unavailable` at startup (GUESS: needs a concurrency test; the in-memory store
   serializes on a write lock, so it cannot reproduce this). The `Conflict` arm also cannot tell an
   email clash from an id/space clash, so a rare id collision would be misreported as "owner
   exists". A real sign-up claiming the sentinel address can pre-seed the row and turn the owner
   bootstrap into a silent no-op for that account.
2. **Password hashing: non-atomic recovery and a timing gap on deactivated users.** In
   `reset_password` the token is consumed and every link revoked *before* the new hash is computed
   (L297-309). If `hash`, `set_password` or `find` then fails (Unavailable/RNG failure), the link is
   burned but the password is unchanged and no session granted: the person is locked out until a
   fresh link is requested. `sign_in`'s deactivated branch (L143-146) skips both `hash` and
   `verify`, so its latency differs from a normal password failure while the reply claims the same
   `InvalidCredentials` — a timing oracle against the "same answer" promise. The unknown-address
   decoy (L139) is `let _ =`, so an RNG failure there silently collapses the timing again. There is
   no `needs_rehash` path: `PasswordHasher` (ports.rs:4119) exposes only `hash`/`verify`, so hashes
   written with weaker parameters are never upgraded. `sign_up`/`ensure_local_owner` hash before the
   uniqueness check, so duplicate addresses push Argon2 work onto the runtime for unauthenticated
   callers (DoS amplifier; no rate limit at this layer).
3. **Email normalization: ASCII-only, with a DB/app mismatch.** `Email::new` lowercases only ASCII;
   non-ASCII local parts/domains (IDN, `Ä` vs `ä`, Turkish `İ`) are stored verbatim, so two
   byte-distinct newtypes can denote one person — two accounts, and recovery for one cannot find the
   other. The DB `text UNIQUE` is case-sensitive; safety rests entirely on every writer going
   through `Email::new`. Migration 0051 used Postgres `lower()` (locale-aware, can differ from
   Rust's ASCII lowercasing) and would itself fail if two legacy rows differed only by case. The
   0047 allow-list (`email text PRIMARY KEY`) is another writer outside this module whose
   normalization is not guaranteed here (GUESS; not verified in this pass).

## Proposed hardening (v0.4.0)

- **A. Idempotent owner bootstrap.** Replace check-then-create with an atomic upsert:
  `INSERT ... ON CONFLICT (email) DO NOTHING RETURNING ...`, then read. If a raw `Conflict` path
  remains, retry the re-read with a short bounded loop (e.g. 3 × 10 ms) before returning
  `Unavailable`, and match the *email* violation specifically. Risk: `ON CONFLICT` needs the email
  unique index as arbiter; the in-memory store must mirror the behavior or tests diverge.
- **B. Make recovery all-or-nothing.** Compute the new Argon2 hash *before* `consume`, and wrap
  `consume` + `revoke_all_for_user` + `set_password` + `delete_for_user` in one transaction (extend
  the reset-token/user/session ports with a transactional variant). On later failure either nothing
  changed or compensate by restoring the token. Risk: port expansion touches every adapter; hashing
  first wastes work on invalid tokens (acceptable — tokens are single-use).
- **C. Equalize sign-in timing.** For a deactivated user, still run `verify` against the real stored
  hash and discard the result (same cost as a failed login); treat a decoy `hash` error as a hard
  failure so silent fast-paths cannot reappear. Risk: small extra CPU for deactivated accounts —
  bound it with the same rate limit as sign-in; REQ-040 must stay byte-identical.
- **D. Rehash on success / charge duplicates less.** Add optional `needs_rehash(hash)` to
  `PasswordHasher` and, after a successful `verify`, upgrade hashes below policy; move a
  duplicate-address check ahead of the expensive hash in `sign_up`, or rate-limit sign-up so
  repeated duplicates cannot drive unbounded Argon2. Risk: trait change affects `FixedHasher`
  (testkit) and any external implementor; a post-login rehash failure must be best-effort and must
  never fail an otherwise valid sign-in.
- **E. One normalization rule, enforced at the boundary.** Either reject non-ASCII in `Email::new`
  (fail fast, keeps the byte-comparison invariant) or normalize with Unicode lowercase + NFC + IDNA
  for the domain, then add a backfill migration and make the unique index expression-based
  (`UNIQUE (lower(email))`); apply the same rule to the 0047 writer. Risk: new migration plus
  possible legacy duplicate resolution; changing normalization can strand stored addresses.

## Explicitly rejected / deferred

- Per-account rate limiting inside this module — belongs at the HTTP boundary; note the dependency.
- Moving Argon2 to `spawn_blocking` — changes the `PasswordHasher` async contract; ADR required.
- Storing addresses in audit rows/metadata — keep the current metadata-free events (REQ-040).

## Risks & rollout

Land C and the E fail-fast rejection first (no contract change, lowest blast radius), then A, then B
(port change), then D (trait change) last behind a flag. Every step must keep the byte-identical
unknown-address response and the single-transaction account creation (REQ-040/042). Verify the 0051
backfill before any normalization change so a case-only duplicate cannot break a deploy.

## Success criteria

- Concurrent `ensure_local_owner` × 8 on an empty DB yields exactly one owner and zero `Unavailable`.
- A forced `hash`/`set_password` failure mid-reset leaves the token usable and the old password valid.
- Timing test: deactivated, unknown-address and wrong-password sign-ins share one latency band.
- Sign-up with `Ä@...` and `ä@...` resolves to one account, or both are rejected, after E.
- Replayed legacy mixed-case rows still recover; a case-only duplicate is caught by the migration.
