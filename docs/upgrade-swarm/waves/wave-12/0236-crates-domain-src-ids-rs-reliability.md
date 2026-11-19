# ids.rs — reliability hardening proposal (v0.4.0)

Worker #236 · MODULE `crates/domain/src/ids.rs` · LENS reliability · READ-ONLY audit.

Status of claims: every "current state" item below was read from the tree in this session
and is marked CONFIRMED; proposals are marked GUESS where behaviour is inferred.

## Current state (CONFIRMED)

- One `opaque_id!` macro generates eleven newtypes (`RoutineId` … `InviteId`) plus `CardId`
  declared separately in `card.rs`. Each is `struct $name(String)` with `#[serde(transparent)]`,
  deriving `Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize`.
- `new(value)` rejects only `value.trim().is_empty()` and otherwise stores the value **verbatim**;
  the rejected/kept distinction is the only validation that exists.
- `#[serde(transparent)]` means JSON deserialisation writes the inner `String` directly,
  bypassing `new()` entirely: an empty or malformed id can enter through the wire.
- There is no `FromStr`, no prefix/length/alphabet check, and no canonicalisation.
- The existing test explicitly asserts a `BotId("bot_123")` and `ThreadId("bot_123")` compare
  equal as strings — cross-entity reuse is accepted by design today.
- Generation lives outside the domain (`infra/crypto.rs::IdGenerator`): `prefix_` + 26
  base64url chars. On RNG failure it falls back to a process counter + nanosecond stamp,
  which is **ordered and guessable**, contradicting the spec's "must not be ordered or
  guessable" rule. The domain cannot detect this because it has no shape/entropy contract.

## Failure modes

### FM-1 — id mixups across entities (CONFIRMED mechanism, GUESS impact)

Distinct newtypes do stop `BotId` being passed to a `ThreadId` parameter at compile time.
What they do **not** stop is value-level confusion: constructor arguments, JSON/DTO fields,
SQL columns and log lines are all plain `String`. Because `new()` accepts any nonblank
string, a thread id fed into `BotId::new`, a swapped pair of route params, or a copied id
in an event payload is silently accepted and only surfaces later as a lookup miss or, worse,
a hit on the wrong entity. `PartialOrd/Ord` on the inner string make such values sortable,
which invites the "ids are ordered" mistake the spec forbids.

### FM-2 — parse/validation errors (CONFIRMED)

- Whitespace is checked but not trimmed: `" bot_1"` and `"bot_1"` are two valid, distinct ids.
- No length ceiling: a multi-megabyte string is a valid id; if used as a DB key or in a URL it
  becomes a memory/latency and log-amplification vector.
- No control-character / non-ASCII filter: embedded newlines or NULs can forge log lines and
  break naive text protocols.
- Deserialisation bypass is the sharpest edge: `#[serde(transparent)]` admits `""`, whitespace,
  oversized and control-laden ids into the domain with zero `DomainError`, so the documented
  invariant ("an empty id is always a bug") is not actually enforced at the boundary.
- Error reporting is coarse: `EmptyId(&'static str)` names the type, not the rejected value,
  which is good for privacy but gives operators no classification of *why* an id was refused.

### FM-3 — random generation (CONFIRMED)

Generation is a fallible OS-RNG call in `infra`, not a domain concern, but ids.rs is where
the product's shape contract should live. Today nothing checks that a generated id is
unpredictable, well-formed, or unique-in-practice. The counter fallback yields ids that are
sequential and enumerable; no domain check flags them. Prefixes are passed as free-form
`&str` at call sites (`"bot"`, `"thr"`, `"sub"`, `"card"`), so a typo yields an id whose
prefix lies about its entity type — indistinguishable from a legitimate one.

## Concrete hardening (ordered, small steps)

1. **Centralise the contract in `ids.rs`.** Give the macro an optional per-type prefix
   (`opaque_id!(BotId, "bot")`), keep `CardId` on the same macro, and expose
   `pub const PREFIX: &str`. This is a compile-time constant, not runtime guessing.
2. **Canonical validation in `new()`.** Trim first, then enforce: exact `prefix_` prefix,
   26-character base64url body from the known alphabet, total length bound, non-empty after
   trim. Map failures to a new typed `DomainError::InvalidId { kind, reason }` (keep
   `EmptyId` for the blank case; never echo the value, matching `InvalidEmail`).
3. **Close the serde bypass.** Replace `#[serde(transparent)]` with a manual `Deserialize`
   that calls `new()` on the raw string, so wire input obeys the same rules as constructors.
   Keep `Serialize` transparent so the wire format is unchanged.
4. **Add `FromStr` + `TryFrom<String>`** delegating to `new()`, and stop storing untrimmed
   input. This makes parse errors uniform and testable at every boundary.
5. **Derive shape from generation.** Have `infra::IdGenerator` take the domain prefix
   constant instead of a string literal, and return a typed error rather than silently
   falling back. If a fallback must remain, use a CSPRNG-seeded, non-sequential scheme and
   tag the id so it is auditable — never an ordered counter.
6. **Discourage ordering.** Consider dropping `PartialOrd/Ord` (keep `Eq/Hash`) so no code
   can accidentally treat ids as ordered; add a `#[deprecated]`-style doc note if removal
   breaks callers.
7. **Tests (traceable names).** `req_043_id_prefix_roundtrip`, `req_043_id_rejects_wrong_prefix`,
   `req_043_id_rejects_empty_and_whitespace_via_serde`, `req_043_id_length_ceiling`,
   `req_043_id_control_chars_rejected`, plus a `proptest` over arbitrary strings asserting
   `new()` accepts iff the shape contract holds.

## Risks and mitigations

- **BREAKING (needs ADR/DEVIATION).** Adding prefix/shape validation changes what `new()`
  accepts and how JSON deserialises; fixtures use prefixes like `usr_1` and `card_golden`
  that may not match a 26-char body (GUESS — verify against golden fixtures before step 2).
  Mitigation: land validation behind a feature flag or migrate fixtures first, and record the
  external-behaviour change in `docs/PARITY.md` with approval.
- Removing `Ord` may break sort-based code (unknown call sites, GUESS). Mitigation: grep all
  sorts of ids first; if any exist, keep `Ord` and document it as ordering-for-indexing only.
- A strict serde impl adds a small deserialisation cost on every id field; negligible versus
  DB I/O, but benchmark if ids appear in large arrays.
- Replacing the RNG fallback touches `infra` (outside this module): coordinate with its owner
  so the "no ordered/guessable id" rule is enforced in one place, not duplicated.

## Recommendation

Do steps 1, 3, 4, 7 first (low risk, high reliability payoff); treat step 2 and step 5 as a
single gated change with fixtures migrated and a DEVIATION entry, because they alter accepted
input and generation behaviour. Steps 6 is optional polish pending a grep for id sorting.
