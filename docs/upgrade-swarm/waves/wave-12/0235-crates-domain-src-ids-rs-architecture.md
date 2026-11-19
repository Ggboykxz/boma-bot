# Upgrade proposal v0.4.0 — `crates/domain/src/ids.rs` (architecture lens)

Worker: 235 | Lens: architecture | Module: `crates/domain/src/ids.rs`
Status: proposal only — no code changed | Language: English
Scope: typed-id newtype structure in `boma-domain` (pure rules: no IO, no async, no clock).

## 1. Current structure (CONFIRMED — read this session)

- One macro `opaque_id!` (`#[macro_export]`, ids.rs:12-56) generates: a transparent newtype
  over `String`, derives `Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize,
  Deserialize`, plus `new(impl Into<String>) -> Result<Self, DomainError>`, `as_str()`,
  `Display`, `AsRef<str>`. `new()` rejects only values whose `trim()` is empty.
- Eleven id types are declared in ids.rs (58-102): `RoutineId, UserId, SessionId,
  OrganizationId, SpaceId, BotId, ThreadId, MessageId, RunId, EventId, InviteId`.
- Two more id types are declared *outside* this file via the same macro: `CardId` in
  `card.rs:15-18` and `ComputerId` in `computer.rs:15-18` (both CONFIRMED).
- `DomainError::EmptyId(&'static str)` (error.rs:22) carries the Rust type name via
  `stringify!`, not the offending value — good, no data leak.
- Wire/persistence ids are plain strings. The generator is an app port
  (`IdGenerator::new_id(&str) -> String`, `app/src/ports.rs:4155`) implemented in infra
  (`crypto.rs:132-145`) as `<prefix>_<random>`; callers build typed ids afterwards with
  `XId::new(ids.new_id("bot"))` (CONFIRMED, e.g. `infra/tests/m1.rs:179-185`).
- Only one test exists (`req_043`, ids.rs:108-119); it covers `new()`/`as_str()` only.

## 2. Target structure for v0.4.0

Keep the zero-cost newtype approach; strengthen the *boundary* (serde, parsing, prefix
contract) and make the type system carry more of the invariant. Do not add IO, a UUID
dependency, or a generator to the domain crate. Desired shape:

```text
ids.rs                 single registry: macro definition + all id declarations,
                       each with PREFIX: &str and one shared IdKind marker trait
                       (card.rs / computer.rs re-export or declare via the same macro)
new(value)             non-empty check (unchanged) -> DomainError::EmptyId
Deserialize            manual: route through new(), so "" never enters the domain
FromStr / TryFrom      uniform parse surface -> DomainError
into_inner(self)       explicit escape hatch to String (no implicit Into<String>)
Ord                    retained, documented as an internal tiebreaker only
```

**Contract to preserve:** ids remain opaque, unguessable, and shape-free on the wire.
Any change to accepted wire values is a DEVIATION (docs/PARITY.md) and needs approval.

## 3. Concrete upgrades (ranked: value / risk)

### U1 — Close the serde invariant hole (high value, medium risk)
Today `#[serde(transparent)]` derives `Deserialize` directly into the private `String`,
bypassing `new()`: an empty id (`""` or whitespace) can enter the domain from JSON or a
DB row even though `new()` forbids it (CONFIRMED ids.rs:17-35; the test never parses).
Proposal: keep `Serialize` derived, write a manual `Deserialize` for the macro that calls
`new()` and maps errors into serde's `de::Error`.
Risk: externally visible behavior change — a request or fixture carrying an empty id that
was previously accepted now fails. Mitigation: write REQ-named tests for parse rejection
*first*, grep goldens/fixtures for empty id values, and record the outcome in docs/PARITY.md
as a DEVIATION if any wire behavior changes.

### U2 — One registry + a prefix contract (medium value, low risk)
Id declarations are split across ids.rs, `card.rs`, `computer.rs`, and the prefix format
lives only at call sites (`"bot"`, `"spc"`, `"card"`). Proposal: add an optional macro arm
that emits `pub const PREFIX: &str` and implements a small `IdKind` trait (`PREFIX`,
`as_str`) for every id type; add a registry doc section in ids.rs listing all types; move
`CardId`/`ComputerId` declarations into ids.rs (re-exported from their original modules to
keep public paths stable). Keep the generator string-based in app/infra — domain stays pure.
Risk: changing the macro signature touches all call sites and adds public API. GUESS: a
prefix *grammar* is not mandated by the spec, so do not validate prefixes, only expose them.
Mitigation: a compile-time test that every `IdKind::PREFIX` is non-empty.

### U3 — Uniform parse/conversion surface (medium value, low risk)
Call sites repeat `XId::new(s).expect("an id is accepted")`, and there is no `FromStr`,
`TryFrom`, or way back into `String` without `.as_str().to_owned()` (CONFIRMED grep).
`computer.rs:8` imports `FromStr` but ids.rs implements none (CONFIRMED). Proposal: add
`FromStr<Err = DomainError>`, `TryFrom<&str>`, `TryFrom<String>`, and `into_inner(self) ->
String` in the macro. Risk: adding `From<TypedId> for String` would make cross-layer
conversion implicit and hide mistakes — skip it; keep `into_inner` explicit. Add
proptest round-trips (`FromStr -> Display -> FromStr`, `serde_json`) for a few types.

### U4 — Reconcile `Ord` with "not ordered" (high value if done, high risk — defer)
The module doc says ids "must not be treated as ordered" (ids.rs:3-5) but the macro derives
`PartialOrd, Ord` (line 16), and callers rely on it for deterministic sort tiebreakers
(CONFIRMED: `testkit/src/memory.rs:1625,1886,2622`, `app/src/account_export.rs:568,652`
BTreeMaps use `String` keys, plus several `.id.cmp(...)` sorts). Removing `Ord` would break
those sites and can change golden output order. Recommendation for v0.4.0: **keep `Ord`**,
add rustdoc stating it is an internal deterministic tiebreaker with no product semantics,
and migrate tiebreakers to `as_str().cmp(...)` in a later wave. Risk if left as-is: a strict
reading of the spec still sees an ordering contract. Mark this gap GUESS in docs/STATE.md
and let the owner decide before any removal.

### U5 — Strengthen the test contract (medium value, low risk)
`req_043` only checks `new()`/`as_str()`. Proposal: add REQ-named tests for (a) serde
rejecting empty ids, (b) `FromStr`/`Display`/serde round-trip, (c) cross-type ids not being
interchangeable at compile time using `trybuild` compile-fail fixtures.
Risk: `trybuild` is a new dev dependency (license check, brittle across rustc versions);
if that cost is rejected, keep type separation documented by `req_043` and drop (c).

## 4. Decisions needed / open questions

- D1: Is rejecting an empty id at parse time (U1) an acceptable DEVIATION? Needs owner sign-off.
- D2: Keep `Ord` as a non-semantic tiebreaker (U4 recommendation) or enforce the spec wording
  and migrate every sort in a dedicated wave? Owner call; recorded as GUESS until answered.
- D3: Is `trybuild` acceptable as a dev dependency (U5c)?

## 5. Evidence and limits

CONFIRMED from files read this session: ids.rs (full), error.rs (full), `domain/Cargo.toml`,
`card.rs:1-25`, `computer.rs:1-30`, `infra/src/crypto.rs:120-149`, and grep results across
`crates/` for `Id::new`, `as_str`, `opaque_id!`, and `cmp/sort` on ids. GUESS items are
explicitly labelled above. No commands were run against the spec/goldens; fixture impact of
U1 is therefore unverified — the implementing wave must check it before changing behavior.
