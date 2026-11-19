# Worker #337 — Computer row mapper architecture proposal (v0.4.0)

- Module: `crates/infra/src/stores/computer_row.rs` · Lens: architecture
- Status: proposal. Claims tagged CONFIRMED were verified by reading this file,
  `stores/computer.rs`, `stores/rows.rs` and `domain/src/computer.rs`; GUESS items need owner
  confirmation.
- REQs in scope: REQ-026..031 (computer lifecycle, REQ-029 takeover, REQ-031 privacy).

## Current structure (CONFIRMED)

The file is a standalone three-part row mapper: `ComputerRow` (`sqlx::FromRow`, 19 fields),
`COMPUTER_COLUMNS` (a `&'static str` projection), and the free function `computer(row) ->
Result<Computer, AppError>`. CONFIRMED: it is the *only* store row module kept out of the shared
`stores/rows.rs`, whose own doc says the row shape and its mapping are "written down once" so a
column change breaks in one place. CONFIRMED: `ComputerRow` is the sole row struct in the tree
without `#[derive(Debug)]`. CONFIRMED: `COMPUTER_COLUMNS` and `computer()` are consumed only by
`stores/computer.rs` (five query sites), so the blast radius of this module is contained.
Timestamps are projected through the custom SQL function `boma.rfc3339(..) AS ..` and carried as
`String` end-to-end (a pattern used repo-wide, not unique to this file).

## Structure target

Keep a single file (~70 lines — no submodule split). Make the file a conforming member of the row
layer: add `Debug`, document why it stands apart or fold it into `rows.rs`, express the mapping as
`TryFrom`, and make numeric parsing fail closed. No behavior reaches the API without a PARITY entry.

## Concrete upgrades

### 1. Remove the duplicated `takeover_at` projection — do first
CONFIRMED: `COMPUTER_COLUMNS` selects `takeover_at` twice — raw at line 35 and again as
`boma.rfc3339(takeover_at) AS takeover_at` at line 40 — while `ComputerRow` declares one
`takeover_at: Option<String>` field. Two result columns share the name; sqlx by-name decoding
picks one, so the mapper's value depends on driver column-order behavior rather than an explicit
contract. Drop the raw occurrence and keep the rfc3339 alias.
Risks: GUESS which of the two values is bound today — capture the original's `GET` output with a
non-null takeover hold before and after (differential test) to see if this is a DEVIATION;
if the raw `timestamptz` was being decoded, the wire timestamp format changes → log in
`docs/PARITY.md` and wait for approval; add `req_029_takeover_at_is_rfc3339` test.

### 2. Restore the "one place" invariant: fold into `rows.rs` or justify the split
CONFIRMED: `rows.rs` holds every other row struct + mapper and documents that invariant; this
module is the lone exception and also omits `Debug`, which every `rows.rs` row derives. Either
(a) move `ComputerRow` / `COMPUTER_COLUMNS` / `computer()` into `rows.rs` and reduce this file to a
thin re-export, or (b) keep the split but add a module-level rationale (REQ-026..031 added late)
and add `#[derive(Debug)]` to match the convention. Prefer (a): one row layer, one place to
rename a column.
Risks: purely mechanical but touches `computer.rs:13` import and every `{COMPUTER_COLUMNS}` site —
run `cargo fmt --check`, clippy and the full suite after the move; no behavior change, no
DEVIATION; do not split into more submodules (fights the one-store-per-file convention).

### 3. Replace lossy `unwrap_or(MAX)` clamping with checked, validated conversion
CONFIRMED: `computer()` uses `u64::try_from(..).unwrap_or(u64::MAX)` for `screen_generation`,
`memory_limit_mb` and `idle_minutes`, so a negative/corrupt `i32` silently becomes `u64::MAX`;
the inverse in `computer.rs:120,142,145,147` uses `unwrap_or(i32::MAX)`. CONFIRMED: the domain
already exposes `validate_memory_limit` / `validate_cpu_limit` / `validate_idle_minutes` and the
bound constants (`domain/computer.rs:133-189`), but the mapper calls none of them; `cpu_limit:
f64` passes through unvalidated (NaN/negative/infinite survive). Convert with `TryFrom` returning a
typed `AppError` and run the domain validators at the boundary; make invalid states
unrepresentable rather than saturating.
Risks: rejecting rows that "worked" while corrupt changes availability → could be a DEVIATION;
GUESS whether such rows exist (query the original DB for negatives/NaN before choosing fail-closed
vs explicit null); needs unit tests for negative i32 and non-finite cpu, named `req_027_*`.

### 4. Stop the per-query `format!` + `AssertSqlSafe` allocation for static columns
CONFIRMED: all five sites in `computer.rs` build `sqlx::AssertSqlSafe(format!("SELECT
{COMPUTER_COLUMNS} ..."))`; `COMPUTER_COLUMNS` is a `&'static str` with no dynamic parts, so
`format!` allocates on every call and `AssertSqlSafe` hand-asserts safety in place of compile-time
checking. Cheap win now: use `concat!` literals (const-folded, no allocation, no assertion escape
hatch). Defer `sqlx::query_as!` + committed offline metadata to a workspace ADR — the repo uses
runtime `query_as` everywhere (sibling #329 reaches the same conclusion), and the custom
`boma.rfc3339` function is GUESS-unknown to sqlx's offline validator.
Risks: `concat!` must stay byte-identical to the current string to avoid a projection change;
verify with the same differential test as upgrade 1; no behavior change, no DEVIATION.

### 5. Make the mapper a `TryFrom<ComputerRow>` and add REQ-named tests
CONFIRMED: `computer()` is a free function and this module has zero tests. `running_idle`
(`computer.rs:221`) already relies on `.map(computer).collect()` short-circuiting on the first
error; a `TryFrom<ComputerRow> for Computer` gives the same semantics idiomatically and composes
with `collect::<Result<Vec<_>,_>>()`. Keep a thin `computer()` wrapper if any other caller is
found. Add focused unit tests constructing `ComputerRow` literals (all 6 `ComputerStatus::ALL`
values, `takeover_by_user` via `UserId`, null `external_id`/`home_id`, the upgrade-3 edge cases),
named `req_026_*` / `req_029_*`.
Risks: a 19-field literal is brittle by design — it surfaces column changes loudly (desired);
GUESS no downstream crate depends on the free `computer()` symbol (grep the workspace first);
`TryFrom` could collide if the domain later adds its own impl — keep the impl in `infra`.

## Cross-cutting risks

- Every externally visible change above (timestamp format, rejecting corrupt rows) is candidate
  DEVIATION material: record in `docs/PARITY.md` and wait for owner approval before shipping
  (AGENTS.md). Purely structural items (2, 4, 5) are not DEVIATIONS if output is unchanged.
- `COMPUTER_COLUMNS` is a shared projection contract; grep all callers before editing it (CONFIRMED
  callers are only `computer.rs` today, but re-check at implementation time).
- Test-first and REQ ids in test names are mandatory; clippy pedantic is on, so avoid `unwrap`/
  `expect` in non-test code when adding the checked conversion.
- Claims here are from static reading only (CONFIRMED by file inspection); nothing was run, so no
  test-pass or runtime-behavior claim is made — verify with the original as oracle during work.
