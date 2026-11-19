# Upgrade proposal — `crates/domain/src/account.rs` (lens: reliability)

- Worker: 206
- Target version: v0.4.0
- Lens: reliability (invariant violations, invalid states representable, normalization)
- Evidence basis: file read in full this session (235 lines). Code facts below are **CONFIRMED** by that read; intent/spec inferences are marked **GUESS**.

## Scope

Pure domain module: `Email`, password validation, `SpaceRole`, `Space`, `User`, `Session`.
No IO, no async. Reliability here = "can a value that violates the rules exist in memory?"

## Failure modes

**FM-1 — `Email` validation is bypassable via serde (CONFIRMED, high).**
`#[serde(transparent)] pub struct Email(String)` derives `Deserialize`. `Email::new` is the only validated constructor, but serde builds `Email` from any JSON string directly. The doc claim "validated once and carried everywhere else" is false: deserialization is a second, unvalidated construction path. `User` also derives `Deserialize`, so a `User` with a malformed `email` deserializes cleanly. Any HTTP/infra DTO that deserializes these types can inject invalid identities past `REQ-040` checks.

**FM-2 — local part is lowercased (CONFIRMED behavior; collision risk).**
`to_ascii_lowercase()` on the whole address merges distinct RFC-local-parts (`Bob@x.com` == `bob@x.com`). Deliberate for byte-for-byte comparison (comment cites REQ-040), but it silently collapses two identities → account confusion / recovery collision. **GUESS:** acceptable if comparison always means "normalized address"; document as an explicit normalization contract, not an accident.

**FM-3 — multiple `@` accepted (CONFIRMED, medium).**
`split_once('@')` splits at the first `@`: `a@b@c.com` yields domain `b@c.com`, which contains a dot → accepted. Invalid addresses persist, breaking "at most one @".

**FM-4 — password min/max use different units (CONFIRMED, low).**
Min uses `chars().count()`, max uses `password.len()` (bytes). A 100-char Cyrillic password (300 bytes) is rejected while its char count passes; units are inconsistent and the error messages ("characters" vs "bytes") disagree. Not a security hole, but a reproducibility/clarity defect.

**FM-5 — `SpaceRole::covers` depends on declaration order (CONFIRMED, high).**
`(self as u8) <= (other as u8)` with no `#[repr(u8)]` and no `rank()` method. Reordering variants or inserting one silently flips permission semantics — a security downgrade with no compiler warning. `ALL` is a hand-maintained duplicate of the same ordering.

**FM-6 — time fields are raw `String` (CONFIRMED, medium).**
`User::deactivated_at: Option<String>`, `Session::expires_at: String`. Any garbage/empty string is representable; no parse guarantee, no timezone, no `is_expired()`. Expiry logic elsewhere will rely on fragile lexicographic string comparison unless a fixed canonical format is enforced everywhere.

**FM-7 — public fields defeat invariants (CONFIRMED, medium).**
`Space`, `User`, `Session` have all-pub fields and no validating constructors. `Space::validate` exists but is a standalone call the caller can forget; `Space` with an empty/overlong name is representable. No invariant enforces "at most one default space" or "deactivated_at consistent with active state" (**GUESS**: may be DB-enforced).

**FM-8 — no length cap on `Email` (CONFIRMED, low).**
Unbounded string length → oversized rows, storage abuse; contrast with password's explicit ceiling.

## Concrete hardening for v0.4.0

1. **H-1 (FM-1): make deserialization validate.** Replace derived `Deserialize` on `Email` with a manual impl that calls `Email::new` (map error to serde `de::Error::custom`). Keep `Serialize` as-is. Add a `#[serde(try_from = "String")]` alternative if simpler. This is the single highest-value fix: it closes the injection path without changing the wire format (still a plain string) — **no API/format DEVIATION** expected, but record in `docs/PARITY.md` as a validation-location change.
2. **H-2 (FM-3, FM-8): tighten `Email::new`.** Reject more than one `@` (`split_once` then assert `domain` has no `@`), reject empty domain labels / leading/trailing dot, and cap length (e.g. 254 bytes total, 64 local) with a new `DomainError` variant or reuse `InvalidEmail`. Keep lowercase normalization, but document it in rustdoc as the canonical-comparison contract.
3. **H-3 (FM-4): single unit for password.** Count bytes for both floor and ceiling (or chars for both), with matching messages. Prefer bytes throughout to bound Argon2 input deterministically.
4. **H-4 (FM-5): decouple permissions from order.** Add `#[repr(u8)]` and an explicit `rank(self) -> u8` via exhaustive `match`, and have `covers` and `ALL` derive from it. A single `match` makes future variant additions a compile-time todo (deny `unimplemented` is already configured). Preserves external behavior exactly.
5. **H-5 (FM-6): typed timestamps.** Introduce a domain `Timestamp` newtype (parsed, UTC, canonical RFC3339) for `deactivated_at`/`expires_at`, plus `Session::is_expired(now)`. Risk: changes serde shape only if strings were non-canonical (**GUESS**: DB stores canonical text) — verify against `spec/openapi.yaml` before shipping; if it changes the wire format, file a DEVIATION.
6. **H-6 (FM-7): validating constructors.** Make fields private (or add `Space::new`, `User::new`, `Session::new`) that run `validate`/normalization, with accessors. Higher-churn option: only do this if callers in `app`/`infra` can be updated in the same wave; otherwise add `#[must_use]` validators and document the obligation.

## Risks & ordering

- H-1 and H-4 are behavior-preserving and low-risk — do first.
- H-2 tightens acceptance: an address previously accepted (multi-`@`) is now rejected. Could be an externally visible change; check `spec/05-known-defects.md` and `docs/PARITY.md` before merging.
- H-5/H-6 touch construction sites across crates; sequence after H-1..H-4 and keep each in its own commit with `cargo fmt --check`, `clippy -D warnings`, and tests.

## Tests to add (REQ ids in names)

- `req_040_email_deserialization_rejects_invalid_address` (serde path).
- `req_040_email_rejects_multiple_at_and_overlong_local`.
- `req_040_password_units_are_consistent` (multibyte case).
- `req_042_role_ordering_is_explicit` (reorder-safety via rank).
- `req_041_session_expiry_uses_typed_timestamp` (if H-5 lands).

Line count: 62 (<120). Status: proposal only, no code changed (READ-ONLY worker).
