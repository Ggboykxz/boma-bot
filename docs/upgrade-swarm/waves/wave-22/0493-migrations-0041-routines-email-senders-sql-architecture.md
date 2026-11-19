# 0493 — `migrations/0041_routines_email_senders.sql` — architecture (v0.4.0)

- Worker #493 · Lens: architecture · Target: v0.4.0 · Read-only review; no code was changed.
- Module: `migrations/0041_routines_email_senders.sql` (7 lines) at `before-m5` (v0.3.0-rc5).
- Read for context (CONFIRMED by direct reads): `migrations/0014_routines.sql`,
  `migrations/0039_routines_github_enabled.sql`, `migrations/0040_bot_mail.sql`,
  `migrations/0051_lowercase_users_email.sql`, `spec/01-requirements.md` REQ-047,
  `spec/02-data-model.md:79-87`, `crates/app/src/usecases.rs:2190-2335`,
  `crates/infra/src/stores/routines.rs`, `AGENTS.md`, and wave-20
  `0441-migrations-0014-routines-sql-architecture.md`.

## Current structure (CONFIRMED)

0041 is a single additive statement: `ALTER TABLE routines ADD COLUMN email_senders text[] NOT NULL
DEFAULT '{}'`. There is no index, no element/length/cardinality CHECK, no normalization and no
`COMMENT`; the column inherits the `routines` RLS from 0014 (`space_id = boma.current_space_id()
OR boma.is_system()`). It is the third additive ALTER on `routines` alongside 0039
(`github_enabled`, REQ-050) after the 0014 base — the entity is intentionally fragmented, so edits
belong in NEW migrations, never in 0041 itself.

Runtime semantics (CONFIRMED in `usecases.rs::mail_inbound`): the inbound `From` is lowercased
(`:2190`) then compared by **exact string equality** against each stored entry (`:2209-2211`);
member/bot-allowlist senders win a conversational run (`:2247-2286`), a sender admitted *only* by a
routine wakes every matching active routine (`:2288-2335`), otherwise the 0040 `hold`/`drop` policy
applies. The match is an in-memory scan of `routines.list(&context, &bot.id)` (`:2203`), not a DB
predicate. The sibling bot-level allowlist in 0040 is `jsonb`, not `text[]` — a deliberate type
divergence with no documenting comment.

## Guiding principle for v0.4.0

Additive only: every change below is a NEW migration containing `COMMENT`, `ADD COLUMN ... DEFAULT`,
`ADD CONSTRAINT ... NOT VALID` + `VALIDATE`, `CREATE INDEX`, or a trigger/new table with its own RLS.
No dropped/renamed column, type change, replaced policy, or backfill that rewrites existing rows.
Preserve the column name, the `text[]` type, the `'{}'` default, the exact RLS predicate and the
mail precedence order — any externally visible change is a DEVIATION requiring `docs/PARITY.md`
(AGENTS.md). Inferred claims are marked GUESS.

## Concrete upgrades (5), each with a risk

1. **Document the ownership and overlap contract in-place.** New migration with
   `COMMENT ON COLUMN routines.email_senders` recording: added by 0041 for REQ-047; overlaps the
   0040 bot allowlist deliberately; member/bot allowlist takes precedence; routine-only senders wake
   routines; `text[]` here vs `jsonb` in `bot_mail_rules.allowed_senders` is intentional.
   - *Risk:* near-zero (comments are inert). *Mitigation:* keep wording identical to the 0041/0040
     header comments; no test or wire change.

2. **Forbid NULL/malformed elements.** Add
   `CHECK (array_position(email_senders, NULL) IS NULL) NOT VALID`, then `VALIDATE CONSTRAINT`, plus
   a generous `cardinality(email_senders) <= 100` and per-element `length` cap.
   - *Risk:* legacy rows with a NULL entry or oversized arrays block `VALIDATE`, and a hard cap
     rejects input the API accepts today (a DEVIATION). *Mitigation:* run a read-only `SELECT ...
     WHERE array_position(..., NULL) IS NOT NULL` audit first; use generous caps; keep the domain as
     the primary guard. GUESS: no producer writes NULL entries.

3. **Close the case-normalization gap.** The sender is lowercased at match time but stored values
   are compared verbatim, so a mixed-case entry silently never matches (contrast 0051, which
   lowercased `users.email`). Add a `NOT VALID` CHECK that every element equals its lowercase, or a
   `BEFORE INSERT/UPDATE` trigger normalizing with `array_agg(lower(...))`.
   - *Risk:* a rewriting trigger changes the stored value relative to what the caller supplied
     (externally visible); a CHECK cannot use a subquery and a backfill would rewrite rows, which
     breaks the additive-only rule. *Mitigation:* prefer lowercasing at the boundary in the RPC/DTO
     layer plus a CHECK on new writes; call it CONFIRMED only after auditing existing rows. No
     backfill in this migration.

4. **Index sender-based wake pushdown.** Add
   `CREATE INDEX routines_email_senders_gin ON routines USING GIN (email_senders);` so a future
   `WHERE active AND email_senders @> ARRAY[$1]` can replace the in-memory filter at
   `usecases.rs:2203-2213`.
   - *Risk:* an unused index is pure write amplification (GUESS: routine counts are small, so the
     scan is cheap today), and any real win needs a new `RoutineStore::find_by_email_sender` port
     method — API/DEVIATION scope. *Mitigation:* land the index only with that store method in the
     same slice, or defer it; `EXPLAIN ANALYZE` before/after on a seeded DB.

5. **Optionally add a normalized per-address join table.** If #4's port method lands, add
   `routine_email_senders (space_id, bot_id, routine_id, address, UNIQUE (routine_id, address))`
   with FKs to `routines`/bots `ON DELETE CASCADE` and an RLS policy byte-identical to
   `routines_tenant_isolation`, enabling an equality lookup instead of `@>`.
   - *Risk:* a table with no consumer is dead weight and is broader than REQ-047 (SHOULD); a
     `citext` dependency would need a `docs/DEPENDENCIES.md` entry. *Mitigation:* propose it in
     `docs/STATE.md` and land only with the REQ owner's approval; keep the RLS predicate exact.

## Cross-cutting risks / sequencing

- Sequence 1 → 2/3 → 4 → 5: comment first (free), then constraints that need a data audit, then
  the index, then the table. Each step is independently revertable and stays additive.
- Freeze the public surface: column name/`text[]`/`'{}'` default, the RLS predicate, the mail
  precedence order (member/bot → routine → policy), and the `email_senders` field in `wire.rs`/
  `ports.rs`. `tools/trace_check.py` is unaffected (no new REQ id; context is REQ-047).
- Gate with `cargo fmt --check`, `clippy -D warnings`, the `req_024` routine-wake tests and the
  REQ-047 inbound-mail test in `crates/infra/tests/m1.rs`; replay mail goldens. No new dependency
  unless #5 is approved.

## Out of scope for this module

Editing 0041/0039/0014, backfilling existing rows, changing the type to `jsonb`, changing match
precedence, the 0040 held-mail policy, and the `mail_inbound` orchestration. Those belong to other
workers/modules; this proposal only strengthens the `email_senders` column additively.
