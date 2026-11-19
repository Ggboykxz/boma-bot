# 0227 — `crates/domain/src/error.rs` — architecture (v0.4.0)

- Worker: 227 · Lens: architecture · Target: v0.4.0 · Read-only review; no code edited.
- Module: 145-line hand-written `DomainError` enum (20 variants) + manual `Display`/`Error` + three
  inherent helpers + 2 unit tests. Re-exported at crate root (`lib.rs:71`, `pub use error::DomainError`).
- Spec anchors: `AGENTS.md` "Errors" (typed errors with thiserror in libraries, one stable API error
  format, never leak internals), "no secrets or personal data in logs", REQ-040 (e-mail is personal
  data, never quoted), REQ-015 (PinnedModelUnreachable), REQ-010 (DelegationRefused).
- Read in this session: `crates/domain/src/error.rs`, `crates/domain/src/lib.rs`, `artifact.rs:20-64`,
  `messaging.rs:44-55`, plus a grep of `DomainError` uses across `crates/domain`, `crates/infra` and
  `crates/app`. Did **not** re-read `crates/api/src/error.rs` mapping this session.
- CONFIRMED = read in this session; GUESS = inferred.

## Structure today (CONFIRMED)

A single flat enum, `#[derive(Debug, Clone, PartialEq, Eq)]`, with no IO/async/runtime dependency and
only one crate dependency (`crate::run::RunStatus`). That purity matches the domain-crate contract and
must be preserved. Payload styles are inconsistent across variants: owned `String` (most), borrowed
`&'static str` (`EmptyId`), structured fields (`WeakPassword { reason }`, `MessageTooLong { limit }`,
`InvalidPageSize { size, max }`, `InvalidTransition { from, to }`), and bare unit variants
(`EmptyMessage`, `ArchivedBot`, `EmailAlreadyTaken`, `InvalidNonce`).

Policy is encoded in three places: the manual `Display` match (lines 67-103), the small helper set
(`invalid_transition`, `unknown_trigger`, `is_invalid_transition`, lines 107-124), and downstream
classification in `AppError`/`ApiError` (per wave-10 doc 0169; GUESS, not re-read here). The enum is
workspace-exhaustive, so adding a variant is a compiler-checked multi-crate edit — a strength worth
keeping.

Architectural findings:

1. **`InvalidName(String)` is a catch-all with an unsafe `Display`.** It is used as every
   validation/parse/external-format failure across `artifact.rs`, `messaging.rs`, `billing.rs`,
   `memory.rs`, `ids.rs` and many `crates/infra` adapters. `Display` (`error.rs:77`) prints its
   payload **verbatim**, and `messaging.rs:51-53` passes raw attacker-controlled input
   (`unknown messaging platform \`{other}\``) into it — so a client can make the echoed message
   contain arbitrary text. `InvalidEmail` deliberately hides its payload (REQ-040 test, lines
   138-144), but the neighbouring catch-all does not: the privacy rule is applied inconsistently.
2. **`#[derive(Debug)]` leaks the hidden data anyway.** `InvalidEmail(String)` stores the raw address
   and `WeakPassword { reason }` stores a reason; `Debug` prints both. Any `{:?}` log of a
   `DomainError` (e.g. `tracing` at the boundary) discloses exactly what `Display` went to lengths to
   hide. CONFIRMED from the derive and payloads; GUESS that a `Debug` log exists today.
3. **Hand-rolled `Display`/`Error` duplicates the library pattern.** `AGENTS.md` defaults to
   `thiserror` in libraries; the messages currently live in a 35-arm match that must be edited in
   lockstep with the variant list, with no compiler help if a new variant is added but its message
   forgotten (the `match` is exhaustive, so a missing arm is a compile error — partial mitigation).
4. **Five "unknown vocabulary" variants duplicate one shape.** `UnknownStatus`, `UnknownTrigger`,
   `UnknownRole`, `UnknownEventType`, `UnknownSystemReason` all carry a `String` and render
   `unknown X: {value}` (lines 73, 78, 85, 94, plus trigger at 74). `EmptyId`, `InvalidCursor` and
   `UnknownStatus`-style values are also quoted into `Display` without a privacy review.
5. **Almost no test surface.** Only a transition test and the REQ-040 email test; no exhaustive
   `Display` matrix, no property test over arbitrary payloads, no test that *every* variant is
   PII-safe, and no test pinning messages byte-for-byte before a refactor.

## Concrete upgrades for v0.4.0 (each with a risk)

1. **Migrate to `thiserror` and pin messages with a golden matrix test.** Replace the manual
   `Display`/`Error` impls with `#[derive(thiserror::Error)]` + `#[error("...")]` per variant, keeping
   every message byte-identical. First add an exhaustive test that asserts the current `Display`
   string for all 20 variants. Risk: any message drift is externally visible where `DomainError`
   reaches a client (GUESS — depends on the api mapping); the golden test makes drift a diff, not a
   surprise, and any intended change needs a `docs/PARITY.md` entry.

2. **Make PII safety structural, not per-variant.** Introduce a small internal wrapper (e.g.
   `Redacted(String)`) or a `Display`-safe field type for addresses, names and raw parser input, so
   the value is stored for diagnostics but `Debug`/`Display` never emit it. Concretely: change
   `InvalidEmail`'s field to a redacting type and give `InvalidName` a sanitising constructor that
   rejects/strips echoed input. Add a property test asserting no variant's `Display` or `Debug`
   contains a sentinel secret. Risk: `Clone + Eq` and `&'static str` call sites must keep compiling;
   a redacting `Debug` can hide data operators rely on, so pair it with an explicit `tracing` path.

3. **Split the `InvalidName` catch-all by failure domain.** Replace it with purpose-specific variants
   (`UnknownPlatform`, `MalformedPayload`, `InvalidArtifactName`, `InvalidStripeSignature`,
   `InvalidMemoryPath`, …), keeping `Display` messages identical. This restores the layer boundary:
   external-format parse failures stop masquerading as "invalid name". Risk: large ripple across
   `crates/infra` adapters and `crates/domain` validators (~40 sites), and each new variant ripples
   into the `AppError`/`ApiError` mapping — land as one atomic change with the matrix test from #1.

4. **Collapse the "unknown vocabulary" family behind a shared type.** Add either a generic
   `UnknownVocabulary { kind: &'static str, value: String }` variant or a private helper that builds
   all five current variants, centralising the `unknown {kind}: {value}` rendering and the
   redaction decision. Risk: a generic variant discards the type-level distinction callers may
   match on (e.g. `UnknownSystemReason` in `tenancy.rs`); preserve the existing variants as thin
   constructors/wrappers unless a grep proves no production `matches!` depends on them.

5. **Add `#[must_use]`, constructors and accessors consistently.** `invalid_transition` and
   `is_invalid_transition` are `#[must_use]`, but `unknown_trigger` is not; add `#[must_use]` and
   small constructors for the remaining structured variants (`message_too_long`, `invalid_page_size`,
   `pinned_model_unreachable`) so callers cannot spell the struct fields wrongly. Risk: constructors
   are additive and low-risk, but tempting to make fields private — keep variants public so the
   workspace-exhaustive match remains possible.

## Explicit non-goals

No IO/async/clock dependency in this crate; no HTTP status codes or API envelopes here (those stay in
`api`); no new variants beyond what the spec needs; no `#[non_exhaustive]` (it would break the
compiler-checked cross-crate mapping); no behaviour change without an ADR / `docs/PARITY.md` entry;
and no attempt here to redesign `AppError` — see wave-10 doc 0169 for that layer.
