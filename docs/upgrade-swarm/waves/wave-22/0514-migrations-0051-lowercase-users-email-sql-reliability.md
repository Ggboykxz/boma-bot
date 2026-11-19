# 0514 — migrations/0051_lowercase_users_email.sql — reliability (v0.4.0)

Worker #514 · module `migrations/0051_lowercase_users_email.sql` · lens: reliability · READ-ONLY
review of `rebuild-workspace/.worktrees/before-m5` at the v0.3.0-era source. Proposals only; no code
in this change. Line refs are against this worktree. Claims are tagged CONFIRMED (read this session)
or GUESS (inferred; no migration was executed, no concurrency test was run). Boundaries consulted:
`migrations/0002_m1.sql:70-79`, `:226-236`; `migrations/0040_bot_mail.sql`;
`migrations/0041_routines_email_senders.sql`; `migrations/0047_access_grants.sql:6-17`;
`crates/domain/src/account.rs:22-41`; `crates/infra/src/stores/users.rs` (via cited reviews);
`docs/upgrade-swarm/waves/wave-14/0320-...-accounts-rs-reliability.md:39-47,68-71`;
`docs/upgrade-swarm/waves/wave-15/0351-...-mail-rs-architecture.md:31-32`.

## Current shape (CONFIRMED)

- The whole migration is one statement: `UPDATE users SET email = lower(email);` (0051:1-7).
- `users.email` is `text NOT NULL UNIQUE` (0002_m1.sql:72). That UNIQUE is **not deferrable** and
  is **case-sensitive** (plain `text`, no `citext`, no functional index — grep of all 59 migrations
  found no `citext`, no expression index on email).
- New rows are normalized in Rust: `Email::new` does `value.trim().to_ascii_lowercase()`
  (account.rs:29-41). The backfill uses SQL `lower()`, which is locale/Unicode aware and does not
  trim. The two rules are already different.
- Email is a byte-compared identity: `find_by_email` uses `WHERE email = $1`; the mail store uses
  `lower(u.email) = $2` (0351:32). No FK references `users(email)` (only `users(id)`).

## Failure modes

**F1 — Case collision aborts the backfill (CONFIRMED code, deterministic failure).** The old rule
allowed byte-distinct twins such as `Alice@Example.com` and `alice@example.com` under the UNIQUE
on raw `text`. `lower()` maps both to one value inside a single statement; the non-deferrable
UNIQUE rejects the second row with `23505 unique_violation`. PostgreSQL statement atomicity means
no partial backfill is left, but the whole statement rolls back and the deployment fails at
startup with a generic constraint error that does not name the colliding addresses. Auto-merging
is not safe: the twins may have different password hashes, memberships and reset history. P0.

**F2 — Two normalization rules diverge (CONFIRMED; impact GUESS without load test).** SQL `lower()`
is Unicode/locale aware and does not trim: `İ`→`i̇`, `ẞ`→`ß`, and `  A@b.co ` stays spaced.
`Email::new` would reject or produce different bytes for each. Backfilled rows can therefore hold
values the app can never query with a domain-normalized `Email`, so recovery "find by address"
and the invitation "caller == invited" check miss them. Because 0051 is a one-shot backfill and the
DB has no CHECK, any raw-SQL writer, import or fixture can reintroduce mixed-case/untrimmed rows
tomorrow; the case-sensitive UNIQUE will happily accept them. This is the ongoing uniqueness gap,
not just a one-time defect. P1. (Also noted independently in wave-14/0320 F3.)

**F3 — Cross-table address drift (CONFIRMED schema, impact GUESS).** 0051 touches only
`users.email`. Other byte-compared addresses may still be mixed-case from the pre-rule era:
`invitations.email` (0002_m1.sql:229, compared with the caller address at accept time),
`access_grants.email` PRIMARY KEY (0047:7), `routines.email_senders[]` (0041:7) and the bot-mail
allowlist (0040:5). After the backfill a stored `Alice@x.com` no longer equals the canonical
`alice@x.com`, so an allow-listed sender or a pre-existing invite can silently stop matching —
an authorization/recovery reliability regression introduced *by* the fix. P1.

**F4 — Full-table rewrite, lock and bloat (CONFIRMED statement shape, volume GUESS).** The
unqualified UPDATE rewrites every row, including the overwhelming majority already lowercase via
`Email::new`. That holds row locks on a global (non-RLS) table, maintains the UNIQUE index for every
row and emits WAL/dead tuples; on a large deployment startup readiness is delayed and the table is
left bloated until vacuum. P2.

**F5 — No preflight, no postcondition, no verification (CONFIRMED).** There is no collision check
before the UPDATE, no assertion that zero rows still violate the canonical form afterwards, and no
REQ-040-named migration/integration test seeding a case-variant twin. A future edit to the rule
silently changes what the migration does. P2.

## Concrete hardening (additive; with risks)

- **H1 — Preflight collision report.** Before the UPDATE, run
  `SELECT lower(email), array_agg(id ORDER BY id) FROM users GROUP BY lower(email) HAVING count(*) > 1`
  and `RAISE EXCEPTION` with the offending addresses/ids. Risk: the deploy still stops (correct),
  but with an actionable message; do **not** auto-merge — require operator resolution and record the
  decision in `docs/STATE.md`. Must use the same canonical expression as the UPDATE (see H3).
- **H2 — Touch only changed rows.** `UPDATE users SET email = lower(email) WHERE email <> lower(email);`
  followed by `ANALYZE users;`. Risk: none functionally (EMAIL is NOT NULL, so `<>` is safe);
  keeps locks/WAL/bloat proportional to actual drift. Keep the predicate byte-identical to H1/H3.
- **H3 — One canonical rule.** Pick and document a single definition, then express it identically in
  `Email::new` and SQL. Recommended default for v0.4.0: keep the deliberate ASCII/trim rule and make
  SQL match (`btrim` + ASCII-only lowercase, or an explicit `COLLATE "C"` where supported), plus a
  `CHECK` that the stored value equals the canonical form. Risk: rejecting non-ASCII/IDN addresses
  is an externally visible behavior change → ADR + `docs/PARITY.md`; the wave-14 alternative
  (`email_key` generated column + UNIQUE index, Rust using the same rule) is stronger but larger.
  Do not silently change either side; ask the operator.
- **H4 — Enforce case-insensitive uniqueness at the DB boundary.** After H1 dedup, add a UNIQUE
  index on the canonical key (`lower(email)` or the generated `email_key`) so raw-SQL twins cannot
  be inserted after the one-shot backfill. Risk: index build fails without H1; it changes a schema
  invariant (ADR + migration). Safe from FK breakage: no FK targets `users(email)` (CONFIRMED).
- **H5 — Backfill dependent addresses in the same migration** (invitations, access_grants,
  email_senders arrays, bot-mail allowlist), or at minimum audit them and document the drift.
  Risk: access_grants has its own PK — lowercasing can collide and needs the same preflight as H1;
  access_grants is FORCE RLS system-only (0047:13-17), so confirm the migration role bypasses RLS
  before touching it. If that cannot be confirmed, defer with an explicit note rather than guessing.
- **H6 — Postcondition + tests.** End with a verification DO block that raises if any row still
  differs from the canonical form or if duplicates remain. Add REQ-040-named tests: a mixed-case
  sign-up yields one canonical row, recovery finds it, a concurrent insert of a case-variant twin is
  rejected after H4, and a legacy mixed-case invitation still resolves after H5.

## Recommended v0.4.0 order

H1 + H2 + H6 first (cheap, removes the startup failure and the blind spot). Then H3/H4 behind an
ADR and operator decision — they change the normalization/uniqueness contract. H5 last, gated on
the RLS/access_grants check; otherwise document as a known gap in `docs/STATE.md`.

## Open questions for the operator (do not guess silently)

- Is ASCII/trim canonicalization intentional, or should full Unicode lowercase + NFKC become the
  rule? H3 depends on it, and D-08 (spec/05-known-defects.md) already flags unasserted auth config.
- On a pre-existing case collision, should the deploy fail with a report (H1) or should a merge
  policy be defined? Merging needs rules for password hash and memberships.
- Must pre-rule mixed-case invitations/allowlist entries keep working (H5), or is a one-time
  invalidation acceptable?

## Non-goals

No change to the public API or wire format, no new feature outside REQ-040, no data deletion or
automatic account merge, and no lowering of any gate. Proposals only — code changes are out of scope
for this worker.
