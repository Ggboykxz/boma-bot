# Worker 0205 — `crates/domain/src/account.rs` architecture proposal (v0.4.0)

Lens: architecture. Scope: read-only review of the 235-line file plus `ids.rs`, `error.rs`,
`lib.rs`, `signin.rs`, `tenancy.rs`, `crates/domain/Cargo.toml` and targeted greps in `crates/`.
No code changed, no tests run. "Current state" claims are **CONFIRMED by source inspection**;
forward-looking items are proposals (GUESS until implemented and tested). One claim borrowed from
worker #161's report is labelled as such.

## Current structure (CONFIRMED)

- Six types in one flat module: `Email` (`:16-54`), the free `MIN/MAX_PASSWORD_LENGTH` constants and
  `validate_password` (`:60-87`), `SpaceRole` (`:90-140`), `Space` (`:143-174`), `User`
  (`:177-191`), `Session` (`:193-202`). `lib.rs:45` re-exports the five public types plus the
  validator. No submodules, no aggregate root, no constructors for the three structs.
- `Email` is a validated newtype (`new` at `:29-41`: trim, ASCII-lowercase, single `@`, dotted
  domain) but **derives `serde(transparent)` Deserialize** at `:16-20`. The same hole exists for
  every id from `opaque_id!` (`ids.rs:12-56`): deserialization writes the inner `String` directly,
  bypassing `new()` and its empty-id check. Invariants are enforced on one construction path only.
- Serde coverage is inconsistent: `Email` and `User` derive serde, `Space` and `Session` do not,
  and `Email`/`User` even spell the derives differently (fully-qualified vs imported).
- `validate_password` mixes units: `chars().count()` for the floor (`:75`) but `bytes()` for the
  ceiling (`:81`), while the reason strings say "characters" and "bytes" respectively (`:78-84`).
- `SpaceRole` has three hand-maintained vocabularies: `#[serde(rename_all = "lowercase")]` (`:91`),
  `as_str` (`:107-113`), `parse` (`:120-127`), plus `covers` defined as
  `(self as u8) <= (other as u8)` (`:131-133`). Authorization order is coupled to declaration order.
- `Space.is_default` (`:152`) carries no "at most one default per organization" invariant;
  `Space::validate` (`:162-173`) checks only the name. `User.name`/`locale` (`:184,188`) are raw
  strings with no validator, unlike `Space`'s name and `Email`.
- Time is untyped text: `User.deactivated_at: Option<String>` (`:190`) and
  `Session.expires_at: String` (`:201`). The DB layer already renders these with `boma.rfc3339`
  (`infra/src/stores/users.rs:37,56,107`) and seeds use literal RFC3339
  (`conformance/src/golden_world.rs:322,628`). `chrono` is already a domain dependency
  (`Cargo.toml:16`, used in `routine.rs:9`), so typed time costs no new crate.
- The only tests (`:204-235`) are three fixed-value unit tests. The `Email` address parser has no
  proptest, though AGENTS.md requires property/fuzz coverage for every external-format parser.

## Guiding principle for v0.4.0

One source of truth per rule; a newtype that holds its invariant on *every* construction path,
including serde; typed time with `now` passed in (the crate stays clock-free); and property-level
evidence at each parser and policy boundary. Preserve all wire/DB shapes unless a DEVIATION is
approved.

## Concrete upgrades (5)

1. **Make validation survive deserialization.** Replace the derived `Deserialize` on `Email`
   (`:16-20`) with a `serde(transparent)`-compatible impl that calls `Email::new` and maps failure
   through `serde::de::Error`; do the same for ids in `opaque_id!` (`ids.rs:16-18`). Keep `Serialize`
   as-is. *Risk:* rejected wire/persisted values that were previously let through, and serde error
   text is operator-visible. *Mitigation:* preserve the RFC3339/string wire format and the
   `DomainError::InvalidEmail`/`EmptyId` messages; add tests feeding `"", "x", "@x"` and empty ids
   through `serde_json::from_str`, and run golden/contract suites before landing.

2. **Typed timestamps, domain stays clock-free.** Introduce a `Timestamp` newtype over
   `chrono::DateTime<Utc>` (or use it directly) for `Session.expires_at` and
   `User.deactivated_at`; add `is_expired(now)` / `deactivated()` methods rather than letting callers
   compare strings. *Risk:* touches infra stores, conformance golden seeds and app logic;
   worker #161 reports a lexicographic expiry comparison in `app/usecases.rs:87` (GUESS — not
   re-verified here). *Mitigation:* keep RFC3339 text on the wire and in the DB, stage as a newtype
   with string serde, pass `now` in from services, and assert ordering/round-trip in tests.

3. **Make invalid account states unrepresentable.** Replace the `email_verified`/`deactivated_at`
   pair with a lifecycle enum (e.g. `UserState::{Active{verified}, Deactivated{at}}`) or, more
   conservatively, private fields plus constructors and accessors; add `User::validate` covering
   name/locale alongside `Space::validate` (`:162-173`). Encode "exactly one default space" as an
   app-level operation since the invariant is cross-aggregate. *Risk:* widest blast radius —
   fields are read across infra stores, conformance seeds and API tests (greps at
   `infra/tests/m1.rs:186-236`, `conformance/golden_world.rs:287`); changing struct shape is a
   DEVIATION. *Mitigation:* land accessors first with public fields intact, keep DB columns,
   record the DEVIATION in `docs/PARITY.md` and wait for approval before the enum lands.

4. **Remove the implicit role-order coupling.** Drive `ALL`, `as_str`, `parse`, `covers` and the
   serde rename from one table of `(Self, &'static str, u8)` entries, so `covers` reads an explicit
   rank instead of the enum discriminant (`:131-133`). *Risk:* role comparison is an authorization
   gate — a wrong rank is a privilege escalation, and a duplicate/missing table entry changes the
   wire vocabulary. *Mitigation:* keep `Owner > Admin > Member` byte-identical; add a test asserting
   rank order, full round-trip for every variant, and `parse` rejection of unknown/upper-case
   values; no new dependency.

5. **One password policy unit + parser property tests.** Name the units explicitly
   (`MIN_PASSWORD_CHARS`, `MAX_PASSWORD_BYTES`) and decide whether both bounds count chars,
   bytes, or each their own documented unit, replacing the current silent mix at `:75/:81`; add
   proptest cases for `Email::new` (idempotent normalization, no panic, all accepted addresses
   round-trip) and for the password boundaries. *Risk:* shifting an acceptance edge changes
   behavior — a DEVIATION. *Mitigation:* keep the numeric thresholds (8 / 256) and the exact reason
   strings fixed; property tests assert invariants and boundary monotonicity, not new cutoffs;
   keep REQ-040/042 in test names (`tools/trace_check.py` counts declarations, not comments).

## Sequencing / cross-cutting risks

- Do #4 and #5 first: local, no wire change, directly testable. #1 next (strictness at the
  deserialization boundary), then #2 (domain + infra + conformance), then #3 last.
- #1, #2 and #3 can alter observable behavior (rejected inputs, error text, struct shape).
  Per AGENTS.md, record each as a DEVIATION in `docs/PARITY.md` and wait for owner approval.
- Preserve the anti-enumeration and redaction guarantees: `DomainError::InvalidEmail` never quotes
  the address (`error.rs:137-144`) and byte-identical refusal responses must survive all refactors.
- Verification gate per step: `cargo fmt --check`, `clippy -D warnings`, domain unit + proptest,
  `tests/` integration, golden/contract suites. Re-run every test in the implementing session
  before flipping any claim here from CONFIRMED-by-inspection to verified-by-test.

## Out of scope for this module

Session TTL policy, sign-up/sign-in and recovery flows live in `crates/app/src/accounts.rs`
(worker #161); the auth extractor and DTOs in `crates/api` (wave-9); DB schema/migrations in
`migrations/`; translation of user-facing error text in the i18n layer. This proposal only
restructures the account domain model and its enforceable invariants.
