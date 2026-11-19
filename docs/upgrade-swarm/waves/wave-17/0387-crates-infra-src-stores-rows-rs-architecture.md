# v0.4.0 architecture proposal — `crates/infra/src/stores/rows.rs` (shared row mappers)

Worker #387 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no code changed).

Status: every "current state" claim below is CONFIRMED by reading `rows.rs`, `stores/mod.rs`,
`crates/domain/src/card.rs` and neighbours, and by `grep` over `crates/infra/src` in this worktree.
Forward-looking items are proposals (GUESS until implemented, tested and measured).

## Current structure (CONFIRMED)

- `rows.rs` is the single row→domain decode boundary: 598 lines, ~20 `#[derive(sqlx::FromRow)]`
  structs, ~17 free mapper functions, plus one presentation helper (`prompt_snippet`, l.522-537).
  It is a hub, not a leaf: `grep` found 79 `rows::` references from ~20 store modules (users,
  sessions, spaces, bots, roster, threads, messaging, messages, runs, events, cards, groups,
  invitations, memory, routines, tools, reactions, admin, account_export, ...). Any mapper or row
  signature change therefore ripples across most of the store layer.
- Three jobs share one namespace: row shape (structs), decode/convert (functions), and
  activity-list presentation (120-char snippet + `PROMPT_SNIPPET_LENGTH`). Return types mix two
  layers: most mappers return `boma_domain::*`, while `group`, `bot_summary`, `invitation` and
  `routine` return `boma_app::ports::*`.
- Duplicated shapes: `BotRow` (l.57-71) and `BotSummaryRow` (l.446-462) repeat the same 13 fields;
  `bot_summary()` (l.278-300) copies them field-by-field into a temporary `BotRow` before calling
  `bot()`. The file already knows the better pattern: `ActivityRow` (l.215-220) uses
  `#[sqlx(flatten)] run: RunRow`.
- Mapping is exposed only as free functions (`pub(crate) fn user(row: UserRow) -> Result<User,
  AppError>` etc.); there are no `TryFrom<Row>` impls anywhere in infra (grep: none). Callers spell
  `map(rows::user)` in ~20 places, so the row↔domain relationship is invisible to generics.
- Wire→enum decoding has two sources of truth. `RunStatus`/`RunTrigger` use `FromStr`
  (domain `run.rs:143,228`); `MessageRole`/`EventType`/`ToolKind`/`SpaceRole` use `parse()`; but
  `card()` (l.410-430) re-implements the snake_case names of `CardKind`/`CardStatus` by hand even
  though those domain enums already carry `#[serde(rename_all = "snake_case")]` (`card.rs:21-44`)
  and expose no `parse()`; `thread()` (l.312-320) and `invitation()` (l.400) use bool coercion /
  `Generic` fallback. `effect()` (l.500-508) leaves `kind` as a raw `String`.
- No `#[cfg(test)]` module in `rows.rs` (nor in any other store module), although this is the most
  reused mapper hub; REQ ids appear only in doc comments (REQ-009/010/022/048), never in test names.
  Reliability failures (NULL→empty id, `unwrap_or(MAX)`, corrupt JSON→503) are covered by worker
  #388; this proposal addresses structure, not that taxonomy.

## Guiding principle

Keep row struct names, fields and mapper signatures stable for callers while making the module's
internal boundaries explicit: one place per row shape, one conversion contract per type, one parse
vocabulary per wire enum. No stored format, column name or public interface change; any visible
delta goes to `docs/PARITY.md` first.

## Upgrades (5 concrete items)

1. **Split the hub into `stores/rows/` submodules behind a stable facade.** Cluster by aggregate:
   `identity.rs` (User/Session/Space/Membership), `agents.rs` (Bot/BotSummary/Group),
   `work.rs` (Thread/Message/Run/Event/Activity), `cards.rs` (Card/Effect),
   `catalog.rs` (Memory/Tool/Reaction/Routine); re-export every symbol unchanged from
   `rows/mod.rs`.
   *Risk:* a missed re-export or name collision can break up to 79 call sites at once (GUESS:
   compile errors, not silent behavior). *Mitigation:* do item 3 first, add a `pub use` parity
   check, and run `cargo check -p boma-infra` before moving any function body.

2. **Deduplicate the bot row with `#[sqlx(flatten)]`.** Redefine `BotSummaryRow` as
   `#[sqlx(flatten)] bot: BotRow, snoozed_until, unread_count`, delete the 13-field manual copy in
   `bot_summary()`, and keep `bot()` as the single conversion.
   *Risk:* flatten needs distinct column names and runtime `query_as`; flattened `Option<String>
   thread_id` must stay `Option` or the NULL case changes (see #388 F1). *Mitigation:* byte-diff the
   emitted SELECT against today's roster query and replay the REQ-009 roster golden before merging.

3. **Adopt `TryFrom<Row>` as the conversion contract, with wrappers during migration.** Add
   `impl TryFrom<UserRow> for User` (etc.) — legal because row types are local to infra (GUESS: no
   conflicting impls; grep found none) — and let the existing `pub(crate) fn user(row)` call
   `row.try_into()` so current `map(rows::user)` callers keep compiling; migrate call sites later.
   *Risk:* changing `?` order can alter which failure wins when a row has two bad fields
   (wire-visible via `AppError`), and keeping both paths risks duplicated validation.
   *Mitigation:* one validation body per type, an equivalence test per mapper on valid rows, and no
   row field reordering.

4. **Give rows.rs one job: move presentation out, separate the two return layers.** Relocate
   `prompt_snippet()` + `PROMPT_SNIPPET_LENGTH` next to the activity read (`runs.rs:557`) or into a
   `presentation` helper so the decode module holds no UI-truncation policy; group the
   `ports`-returning mappers (`bot_summary`, `invitation`, `routine`, `group`) apart from the
   `domain`-returning ones, documenting the domain→app dependency direction.
   *Risk:* moving the snippet invites "small fixes" to truncation that are externally visible →
   DEVIATION. *Mitigation:* move verbatim, add a byte-exact test for whitespace collapse and
   120-char truncation, and leave `AppError` mapping untouched.

5. **One parse vocabulary per wire enum.** Add `parse()`/`FromStr` to `CardKind` and `CardStatus`
   in `boma_domain::card`, sourced from the same snake_case names serde uses, and have `card()`
   call them; route `thread.kind`, `invitation.status` and channel platform through the same
   validate-or-error style (error taxonomy belongs to #388 H5, not duplicated here).
   *Risk:* touching the domain crate widens the diff; a renamed variant can silently diverge from
   stored rows, and unknown legacy values may start erroring → DEVIATION-gated.
   *Mitigation:* audit distinct stored values first, derive names from one table shared by serde and
   `parse`, and replay the REQ-007/017 card goldens.

## Cross-cutting risks / sequencing

- Order 3 → 2 → 1 → 4 → 5: fix the conversion contract and the duplicated shape first, then split
  files, then relocate presentation, then centralize enums. Each step is its own commit.
- The 79 call sites are the blast radius: never combine a rename with a move in one step.
- Gate each step with `cargo fmt --check`, `clippy -D warnings`, `cargo check -p boma-infra`, the
  `crates/infra/tests` round-trips and REQ-named trace checks; no new dependency is needed.
- Do not change stored columns, formats or error envelopes silently; #388 owns corruption /
  saturation hardening, and any new `AppError` variant needs an ADR + `docs/PARITY.md` entry.

## Out of scope for this module

SQL text and tenant scoping in the ~20 store adapters (their own workers), HTTP DTOs (`wire.rs`),
sequence/idempotency logic in `runs.rs`/`messages.rs`, and the reliability taxonomy in #388.
