# 0239 — `crates/domain/src/mail.rs` — architecture (v0.4.0)

- Worker 239 · Lens: architecture · Target: v0.4.0 · Read-only review; no code edited.
- Module: `crates/domain/src/mail.rs` (321 lines, v0.3.0-rc5 at `before-m5`).
- Contract/context read this session (CONFIRMED): `spec/01-requirements.md` REQ-047 (:383-389),
  `spec/03-api-contract.md` (`POST /api/v1/email/inbound`, 25 MB, :129),
  `spec/04-nonfunctional.md` (inbound mail 25 MiB, :51),
  `spec/05-known-defects.md` (D-03 note on mail attachments, :26-30),
  `spec/TRACE.md` (REQ-047, :64), `spec/openapi.yaml` (:274-305),
  `crates/domain/src/lib.rs` (:28, :78), `crates/domain/src/error.rs` (:23-31, :67-102),
  `crates/domain/src/account.rs` (`Email` newtype, :20-38),
  `crates/domain/src/plugin.rs` (per-module typed error pattern, :51-80),
  `crates/app/src/usecases.rs` (`mail_inbound`, :2135-2160),
  `Cargo.toml` (workspace lints/deps), `docs/DEPENDENCIES.md` (proptest deferred, :68).

## What the mail domain model is today (CONFIRMED)

One flat result struct (`ParsedMail`: `from`, `from_name`, `recipients`, `subject`, `body`), two size
constants, one entry point (`parse_rfc822(&[u8]) -> Result<ParsedMail, DomainError>`), seven private
helpers, and four unit tests carrying the REQ-047 id. The shape is a projection of what REQ-047
needs: a run is started from the first `text/plain` part, and the caller only reads the decoded
fields. There is exactly one production consumer, `app::mail_inbound` (:2151-2160), which hashes the
relay token first, parses, then matches a `boma+<bot>@<domain>` recipient by `strip_prefix` on
`mail.recipients`; `lib.rs:78` re-exports the whole public surface.

Architecturally the file is a **single-pass monolith with no intermediate model**: `parse_rfc822`
owns header folding, header lookup, address parsing, RFC 2047 decoding, MIME part selection and
transfer decoding in one 45-line function calling free functions that each re-parse strings. The
consequences:

- **No typed vocabulary at the boundary.** Addresses are lowercased bare `String`, although
  `account.rs:20` already defines a validated `Email` newtype and `DomainError::InvalidEmail` is a
  privacy-safe error (test `req_040_an_email_error_never_quotes_the_address`, `error.rs:138`).
- **All four failure paths use `DomainError::InvalidName`** (`:41, :43, :47, :50`) — a generic name
  error for a mail shape violation; `InvalidEmail` exists and is deliberately unused here.
- **Header map is `HashMap`, first duplicate wins** (`:109-119`). A second `From` or `Content-Type`
  is silently ignored; the module doc promises it "refuses loudly rather than guessing", but
  ambiguity is currently guessed.
- **Address grammar is `rfind('<')` and `split(',')`** (`:122-149`): quoted commas, groups and
  multiple addresses inside `From` are mis-parsed; `Content-Type` parameters are split on `;`
  without quote awareness (`:241-244`).
- **RFC 2047 is UTF-8/B and Q only**, and Q decoding does not map `_` to space (`:151-200`,
  `:203-235`); malformed words emit `?` instead of refusing. MIME walking is one level deep, picks
  the first `text/plain` with no depth/part/size cap (`:238-266`).
- **Truncation is silent** (`MAIL_SUBJECT_MAX`, `MAIL_BODY_MAX`, `:14-16, :64, :81`) while the
  doc comment claims loud refusal for anything beyond scope.
- **No property or fuzz test** for this external-format parser; `docs/DEPENDENCIES.md:68` explicitly
  defers `proptest` to the milestone that needs it, and AGENTS.md requires it for every parser.

## Target structure for v0.4.0 (GUESS)

Keep `parse_rfc822` + `ParsedMail` + the two constants as the stable facade (they are the only
public surface and the only consumer compiles against them). Behind it, split the file into a
`mail/` module with a staged pipeline and typed intermediates:
`raw bytes → HeaderBlock (folded, ordered, duplicate-aware) → Address/EncodedWord decoders →
MimePart selection → ParsedMail`. Each stage is a pure function with its own tests; the facade
composes them. No IO, no async, no clock — this stays a domain-crate rule.

## Concrete upgrades, each with a risk

1. **Stage the pipeline and split the file.** Introduce `mail/{mod,headers,address,encoded_word,
   mime}.rs` with `HeaderBlock<'a>` and a `MailPart` intermediate; `mod.rs` keeps the re-exports so
   `lib.rs:78` and `usecases.rs:2151` do not change. Risk: the largest diff of the five — a silent
   output change on an edge header is easy to miss with only four tests; land golden fixtures from
   `docs/DEPENDENCIES.md`-style real captures first, and keep the facade byte-identical.
2. **A dedicated `MailParseError` typed error** (same pattern as `PluginSourceError`,
   `plugin.rs:51-80`): `NotUtf8`, `NoHeaders`, `MissingFrom`, `InvalidFrom`, `AmbiguousHeader`,
   `Oversize`, each with a fixed, address-free message; map to `DomainError::InvalidEmail` or a new
   domain variant at the crate boundary. Risk: `app::mail_inbound` uses `?` from `parse_rfc822`
   (:2151), so the `From` conversion must be added or the caller breaks; any changed error string is
   potentially externally visible and needs a `docs/PARITY.md` entry if it differs.
3. **Addresses as a validated newtype + explicit truncation policy.** Return `Email`-shaped values
   (reuse/align `account.rs:20`) for `from`/`recipients`, and either expose `subject_truncated`/
   `body_truncated` flags or refuse oversize outright — pick one and document it. Risk: the caller
   does `strip_prefix` on `&str` recipients (:2154-2158), so the newtype needs `as_str()`; keep ASCII
   lowercasing to preserve the `boma+`-routing behavior; adding fields or refusing previously
   accepted mail is a DEVIATION requiring owner approval.
4. **Correct the RFC 2047/address/MIME grammar within the "refuse loudly" contract.** Handle Q-word
   `_` → space and non-UTF-8 charsets explicitly (decode or refuse, never `?`), parse quoted commas
   and groups in address lists, reject duplicate `From`/`Content-Type`, and cap multipart depth,
   part count and accumulated bytes. Risk: fixes change parsed output for malformed inputs and can
   reject mail the old reader accepted — differential-test against the current implementation and
   record differences in `docs/PARITY.md`; if attachments are ever surfaced, honor D-03 (sniff
   magic bytes, never trust the declared type).
5. **Property/fuzz coverage as part of the refactor.** Add `proptest` as a domain dev-dependency
   (license check + `docs/DEPENDENCIES.md` + `cargo deny` first) and assert for arbitrary byte input:
   `parse_rfc822` never panics, never returns a `from` without `@`, honors both size caps, and is
   deterministic for a fixed input; add a small corpus of adversarial messages (folded headers,
   nested multipart, bad base64, embedded NUL). Risk: new dev-dependency and CI time; the current
   `i + 2 < bytes.len()` indexing in `decode_quoted_printable` (`:209-219`) and `splitn` slicing need
   a panic audit before the property test will hold; a fuzz-discovered behavior change is a
   DEVIATION if it affects accepted mail.

Priority: 4 and 5 first (correctness and panic-safety of a parser exposed to relay-authenticated
external mail), then 2, then 1 and 3 behind the facade. All behavior changes stay gated by
`docs/PARITY.md`; none of the proposals adds a feature outside REQ-047.
