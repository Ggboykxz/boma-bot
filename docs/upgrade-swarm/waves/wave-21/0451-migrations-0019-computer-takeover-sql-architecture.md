# Wave 21 / Worker 0451 — `migrations/0019_computer_takeover.sql` (LENS: architecture)

Proposal for v0.4.0. Additive only: every change below *adds* columns, constraints, an index, a
table or comments; nothing already applied is rewritten, dropped, back-filled or renamed. New work
goes into later numbered files (e.g. `0060_takeover_pair_invariant.sql`, `0061_takeover_audit.sql`)
so the applied-migration hash chain stays intact. Never edit `0019` in place.

## Current structure (CONFIRMED — read this session)

`0019_computer_takeover.sql` is an 8-line forward-only migration (no `UP`/`DOWN`, matching
siblings such as `0020_team_computers.sql`). Header cites REQ-029. It issues one statement:

```sql
ALTER TABLE computers
    ADD COLUMN takeover_by text,
    ADD COLUMN takeover_at timestamptz;
```

Both columns are nullable. `takeover_by` names the member holding exclusive control; `takeover_at`
is when the hold began; both are cleared on release (file comment, CONFIRMED). There is **no** FK,
`CHECK`, index, `COMMENT`, or paired-column invariant. `computers` already carries tenant RLS from
`0017_computers.sql` (keyed on `space_id`), so the two new columns inherit space scoping with no
RLS change of their own (CONFIRMED).

Related surface read this session (CONFIRMED):
- `0017_computers.sql`: `computers` has `status`, `screen_generation`, `private`, `bot_id`, and
  `UNIQUE (space_id, bot_id)`; `runs.status` (in `0002_m1.sql`) is unconstrained `text`, so the
  `waiting_takeover` run state needs no migration.
- `0020_team_computers.sql`: a team computer (`private=false`) is one row shared by a whole space.
- `0018_screen_capabilities.sql`: screen access is a bearer capability bound to
  `screen_generation` (REQ-030).
- `spec/01-requirements.md` REQ-029: exclusive hold, bot input refused until release, UI shows
  "you have control", browser state kept per checkpoint policy.
- `spec/golden/computer-takeover-conflict.json`: a second takeover returns `409 CONFLICT`; the
  holder is named — exclusivity is currently an application-layer promise only.
- `spec/06-decisions-needed.md` DEC-03: `waiting_takeover` is queue-holding (intent = GUESS).

## Architectural assessment

The design is admirably minimal — a two-column lock on the computer row is the right place for a
single source of truth, and at-most-one-holder is *inherent* in a single scalar column. The gaps are
structural, not data bugs: (a) the two columns can be half-set because nothing enforces the
both-or-neither contract the header describes; (b) exclusivity relies on the app issuing an atomic
claim that the schema neither expresses nor documents; (c) `takeover_by` is an opaque string with no
stated identity space, so "who holds control" cannot be joined or validated; (d) release erases all
evidence, so no history of who held a screen when survives; (e) for a team computer the hold names
no bot, although the run that paused is per-bot. All upgrades below close these additively and
leave public interface names untouched.

## Concrete upgrades (additive only)

### 1. Enforce the paired-column invariant (`takeover_by` ↔ `takeover_at`)
Add a named `CHECK ((takeover_by IS NULL) = (takeover_at IS NULL))` via `ADD CONSTRAINT …
NOT VALID`, then a separate `VALIDATE CONSTRAINT` only after an audit query proves zero
half-set rows. Makes the header comment executable and stops a partial hold from leaving the
computer permanently "busy" or, worse, silently free.
- **Risk**: medium — a legacy row with one column set would fail validation. Mitigation: audit
  first; if offending rows exist, document them in `docs/STATE.md` as a DEVIATION and leave the
  constraint `NOT VALID` rather than deleting data.

### 2. Declare the claim protocol and add a hot-path partial index
`COMMENT ON COLUMN` for both columns stating the atomic claim is
`UPDATE computers SET takeover_by=$1, takeover_at=now() WHERE id=$2 AND takeover_by IS NULL
RETURNING …` (the DB-level compare-and-swap behind the 409 golden), that a takeover must **not**
bump `screen_generation` (or REQ-030 would invalidate the human's own live capability), and that
release nulls both. Add `CREATE INDEX computers_takeover_active_idx ON computers (space_id)
WHERE takeover_by IS NOT NULL;` for cheap conflict checks and "who holds what" observability.
- **Risk**: low — comments are metadata and a partial index is non-destructive. Mitigation: mark
  the claim-protocol wording GUESS until confirmed against the takeover adapter; the index is cheap
  because active holds are rare.

### 3. Type the holder identity with a deferred FK
Add `takeover_by` documented as a member key, plus `ADD CONSTRAINT computers_takeover_by_fkey
FOREIGN KEY (takeover_by) REFERENCES users (id) ON DELETE SET NULL NOT VALID` (validate after
audit). This turns an opaque string into a joinable, integrity-checked identity and makes the
409 message's "holder" resolvable without guessing.
- **Risk**: medium/high — if the app writes an external subject (provider id) or a deleted member
  rather than a `users.id`, validation fails or `ON DELETE` fires wrongly. Mitigation: CONFIRM the
  exact identity space in the adapter/spec first; if it is external, skip the FK and ship only the
  `COMMENT ON COLUMN` naming the space. Never rewrite existing values to satisfy the FK.

### 4. Record the bot dimension for team computers
Add nullable `takeover_bot_id text` with a deferred FK to `bots (id)` (`NOT VALID`), documented as
"the bot whose thread moved to `waiting_takeover`". On a team computer (REQ-031) the shared row is
held by one member but pauses one *bot's* run; naming that bot lets a release be scoped and keeps
the hold correlated with DEC-03's queue-holding run.
- **Risk**: medium — introduces denormalisation with `runs.status`, and populating it needs an app
  change, so existing active holds stay NULL. Mitigation: keep nullable, no `NOT NULL`, no default
  back-fill; treat NULL as "bot not recorded at acquire time (legacy)".

### 5. Add an append-only takeover audit table
Create `computer_takeover_events` (`id`, `space_id`, `computer_id`, `bot_id`, `actor`, `event`
`CHECK IN ('acquire','release')`, `at timestamptz`) with `computers`-style tenant RLS via the same
`DO $$ … FOREACH … $$` pattern from `0017_computers.sql`, mirroring `0030_audit_events.sql`. The
live columns answer "is it held now?"; the table answers "who held it, when?" — needed because
release currently erases that evidence (REQ-029's manual sign-in has no trace).
- **Risk**: low/medium — unbounded growth and a new RLS surface. Mitigation: write-only from the
  takeover path, index on `(computer_id, at)`, document retention in `docs/PLAN.md`; enable and
  FORCE RLS with a `space_id` policy identical to the other tenant tables.

## Additive-only guarantee

Items 1/3/4 use `ADD CONSTRAINT … NOT VALID` with validation gated on an audit; 2 adds comments and
a partial index; 5 adds a new table. No `DROP`, no type change, no destructive `UPDATE`, no
back-fill rewriting existing rows. Public interface names (`takeover_by`, `takeover_at`,
`waiting_takeover`, the `/rpc/computer/takeover` 409 contract) are unchanged — no DEVIATION.

## Verification before merge

- Apply all migrations to a fresh DB and to a copy of the current schema; assert row counts and
  existing column values are unchanged.
- Run the audit queries for items 1 and 3; record counts in `docs/STATE.md`.
- Apply a takeover twice against the golden request and confirm the second returns 409.
- Confirm a takeover does not change `screen_generation` (REQ-030) and that release nulls both
  live columns while leaving one `acquire` + one `release` audit row.
- Label every claim above CONFIRMED or GUESS per workspace rules; item 3's FK and item 2's protocol
  wording are GUESS pending adapter confirmation.

Status: proposal only — no file outside this report was modified.
