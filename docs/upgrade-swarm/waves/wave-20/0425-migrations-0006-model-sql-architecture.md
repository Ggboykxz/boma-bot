# Wave 20 / Worker 0425 — `migrations/0006_model.sql` (LENS: architecture)

Proposal for v0.4.0. Additive only: every change below *adds* files, columns, constraints or
comments; nothing already applied is rewritten, dropped or back-filled destructively.

## Current structure (CONFIRMED — read this session)

`0006_model.sql` is a 13-line forward-only migration. Header comment says "M2": a bot and a run
carry the model pinned for them. It issues six `ALTER TABLE … ADD COLUMN` statements:

| Table | Columns added |
|---|---|
| `bots` | `model_provider text`, `model_id text`, `model_pinned boolean NOT NULL DEFAULT false` |
| `runs` | `model_provider text`, `model_id text`, `model_pinned boolean NOT NULL DEFAULT false` |

Characteristics: no `UP`/`DOWN`, matches the one-way convention of `0001_init.sql`
(CONFIRMED). Style matches sibling additive migrations, e.g. `0039_routines_github_enabled.sql`
(CONFIRMED). All three columns are nullable except the boolean; there is no `CHECK`, no FK to a
model catalogue, no timestamp and no index.

Related surface read this session (CONFIRMED):
- `0043_models.sql` introduced `model_credentials` (`provider`, `space_id`, sealed `ciphertext`)
  and `user_model_preferences` (`model_key`, `updated_at`), RLS-protected.
- `spec/02-data-model.md` lists Bot (`modelProvider`, `modelId`, `pinned`) and Run
  (`modelProvider`, `modelId`, `modelPinned`) fields, plus
  `UserModelPreference / SpaceModelPreference / SpaceModelRoleChoice / UserModelRoleChoice`
  "resolved model per role (chat, helper, review, voice)".
- `spec/01-requirements.md` REQ-015: a run picks a model from bot/space/user preference, can be
  pinned, falls back when the first choice is unavailable, and must not silently switch models.

## Architectural assessment

The two tables duplicate the same three-column tuple. That is defensible denormalisation — a run
must name its resolved model without a second lookup (the file's own comment) — but there is no
documented contract for *which* tuple wins, how a `NULL` is interpreted, or how the boolean
relates to the string fields. The naming also diverges from `0043_models.sql` (`model_provider`
vs `provider`, `model_id` vs `model_key`), so the reader cannot tell whether these are the same
identifier space. Those are structural gaps, not data bugs; all upgrades below close them
additively.

## Concrete upgrades (additive only)

### 1. Entity-level column documentation via `COMMENT ON`
Add `COMMENT ON COLUMN` for all six columns stating: this is the *resolved* model snapshot;
`model_provider` + `model_id` name a catalogue entry; `model_pinned=true` means "never fall back"
(REQ-015); `NULL` means "not resolved / defer to preference chain". No behaviour change.
- **Risk**: low. Comments are metadata only; risk is drift if wording contradicts REQ-015 later.
  Mitigation: quote the requirement id and keep wording identical for both tables.

### 2. Additive `CHECK` constraint on the boolean/string relationship
Add a named `CHECK` on each table enforcing the invariant "a pinned row carries a provider and
id" (`model_pinned = false OR (model_provider IS NOT NULL AND model_id IS NOT NULL)`). Use
`ADD CONSTRAINT … NOT VALID` then a separate `VALIDATE CONSTRAINT` so existing rows are never
rewritten.
- **Risk**: medium. A legacy row with `model_pinned=true` and `NULL` strings would fail validation.
  Mitigation: `NOT VALID` first, validate only after an audit query confirms zero offending rows;
  if any exist, document them as DEVIATION rather than deleting data.

### 3. Additive resolution-source columns
Add `model_source text` to `runs` (and optionally `bots`) recording *where* the value was chosen:
`bot | space | user | deployment | oauth`, with a `CHECK (… IN (…))` added `NOT VALID`. This makes
the REQ-015 preference chain observable without joining the four preference tables of
`02-data-model.md`.
- **Risk**: medium. Adding an enum-like `text` + `CHECK` duplicates knowledge that belongs in a
  domain enum; if the chain grows (role choices, REQ-075 local servers) the constraint needs a
  follow-up migration. Mitigation: keep values in a documented set and add new values additively.

### 4. Additive `updated_at` and resolution timestamp
Add `model_resolved_at timestamptz` to `runs` (and `model_updated_at` to `bots`) with
`DEFAULT now()`, mirroring `0043_models.sql`'s `updated_at` pattern. Lets an auditor tell a
run-time snapshot from a later bot default change.
- **Risk**: low. Defaulted nullable-free column is cheap on a table already using `now()`
  defaults elsewhere (CONFIRMED in `0043_models.sql`). Only cost is one more field to populate.

### 5. Additive consistency comments / naming bridge note
Add a header comment block (and optionally `COMMENT ON COLUMN`) explicitly stating that
`model_provider/model_id` here are the same identifier space as `provider/model_key` in
`0043_models.sql`, and that the older names are kept for public-interface compatibility
(`02-data-model.md` lists them as Bot/Run fields). Pure documentation.
- **Risk**: low. If the two identifier spaces actually differ, the comment would be wrong.
  Mark as GUESS until confirmed against the adapters; do not rename any column.

## Additive-only guarantee

Items 1, 5 are documentation; 2, 3 add constraints/columns; 4 adds columns. No `DROP`, no
`ALTER COLUMN … TYPE`, no destructive `UPDATE`, no back-fill that rewrites existing rows. Each new
constraint is introduced `NOT VALID` and validated only after an audit. New migrations should be
separate numbered files (e.g. `0060_model_column_contract.sql`, `0061_model_resolution_source.sql`)
rather than edits to `0006_model.sql`, preserving the applied-migration hash chain.

## Verification before merge

- Apply all migrations against a fresh database and against a copy seeded with the current schema.
- Assert existing rows are unchanged (row counts and column values) after each new migration.
- Run the audit queries from items 2/3; record counts in `docs/STATE.md`.
- Mark every claim here CONFIRMED or GUESS per the workspace rules; this document's claims are
  CONFIRMED where noted, item 5's equivalence is GUESS pending adapter confirmation.

Status: proposal only — no file outside this report was modified.
