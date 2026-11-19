# 0210 — crates/domain/src/artifact.rs — reliability (v0.4.0)

Worker #210 · `crates/domain/src/artifact.rs` · reliability · READ-ONLY review of `before-m5`.
Scope: pure validation of artifact names, descriptions and content type (REQ-036, D-03). It does no
IO, so its reliability surface is the guarantees it gives callers: what it rejects, what it silently
accepts, and the error it returns. Line refs against `before-m5`; file facts CONFIRMED by reading;
no code/tests/builds ran (test/build status GUESS).

Spec/AGENTS anchors: spec/01-requirements.md:297-302 (REQ-036 versions 1..3, latest default, global
delete); spec/02-data-model.md:119 (Artifact carries rootArtifactId, version, clientNonce —
"versions form a chain rooted at the first artifact"); spec/05-known-defects.md:18 (D-03 sniff);
STATE.md:935-952 (shipped D-03 policy); AGENTS "make invalid states unrepresentable", "typed errors
with thiserror", "validation at the boundary", "newtypes for ids".

## What is already right (keep, don't re-propose)

- Cap constants are explicit and shared (artifact.rs:11-16; reused at rpc.rs:273 and
  usecases.rs:1643): one source of truth for 4 MiB and the 120/1000-character bounds.
- D-03 is genuinely enforced here: magic bytes, HTML-in-first-4-KiB, UTF-8 text, generic corrected,
  specific-vs-specific refused, HTML-as-non-HTML refused (artifact.rs:69-191, tests 211-289).
- Name rejects separators and control chars (23-44); description allows only `\n`/`\t` controls (51-66).
- `detect_mime`/`sniff_artifact_mime` are total over `&[u8]`: no panic on short/empty input
  (`bytes[..min(4096)]` at 90 is safe).

## Failure modes seen → concrete hardening proposals

### F1. Validation returns `()` and discards the normalised value
`validate_artifact_name`/`validate_artifact_description` (23-66) return `Ok(())` after trimming, so
the trimmed name is recomputed independently at usecases.rs:1670-1672. The validated bytes and the
stored bytes can drift if either side changes.
P1. Return a validated newtype carrying the trimmed form and have the use case store that value; keep
    the existing `Result<(), DomainError>` functions as thin wrappers for callers/tests. No external
    behaviour change. Risk: small domain API addition + one use-case edit; test that a name with
    surrounding spaces is stored trimmed and still validates.

### F2. Every refusal is `InvalidName` — one error for name, description and MIME
All validators (26, 53, 59, 156, 169, 173-189) return `DomainError::InvalidName(String)`, whose Display
prints the reason verbatim (error.rs:31, 77); that text flows into `Failure.detail` via
`AppError::Rule` (see wave-10, service.rs:603-620). Clients cannot distinguish "bad name" from "bad
MIME", and the reason embeds the client's declared type (`declared type \`{declared}\``) — attacker-
controlled text logged verbatim.
P2. Add dedicated variants (`InvalidArtifactName`, `InvalidArtifactDescription`, `UnsupportedMime`,
    `MimeMismatch`) with static, value-free Display messages; map them to the same bad_request if the
    envelope must stay stable. Test that a client-controlled declared type is not echoed. Risk: adding
    variants is non-breaking; changing the client error string is externally visible — record in
    docs/PARITY.md.

### F3. Missing/truncated content is accepted as a valid type
Empty bytes are UTF-8 with no control chars, so `is_plain_text` (107-114) returns true and
`detect_mime` classifies `b""` as `text/plain` (118-129); no test covers the empty case (269-289). An
8-byte PNG header or a lone `PK\x05\x06` passes `signature` (69-86) with no structural check, so a
truncated blob is stored and served as `image/png`/`application/zip`.
P3. Reject empty content in the domain (`detect_mime` -> `None` on empty) and add an `ArtifactBytes`
    newtype requiring non-empty and `len() <= ARTIFACT_MAX_BYTES` at construction; move the size check
    out of usecases.rs:1643-1650 so the bound is a type invariant, not a caller convention. Risk: empty
    artifacts may be accepted today (GUESS — no golden); add a test/golden first. Full PNG/ZIP parsing
    is out of scope — magic + minimum-length sanity only.

### F4. HTML detection has boundary and false-positive gaps
`looks_like_html` (89-104) lowercases only the first 4 KiB and maps all non-ASCII to `\u{1}`, so a
marker split across the 4096 boundary is missed; conversely `<html`/`<script` are plain substrings, so
a text/markdown note containing `<script` is misclassified as HTML and refused (182-185), and
`<htmlfoo` is accepted as a marker.
P4. Allow a small overlap window at the 4 KiB boundary, require a token boundary after tag names, and
    test both the split-boundary and false-positive cases. Keep the D-03 policy unchanged. Risk: a
    newly refused upload is a behaviour change — gate with the D-03 differential tests and note in
    docs/PARITY.md if clients rely on it.

### F5. Structured text-family types are declared, not verified
`sniff_artifact_mime` returns the declared type unchanged whenever bytes are plain text and the
declaration is in the text family (178), so malformed bytes stored as `application/json` or
`application/xml` are never parsed (the test at 263-265 pins this). A declared
`text/plain; charset=utf-8` is rejected as malformed by `declared_type_ok` (132-143), which has no
MIME-parameter support — a common client header.
P5. For JSON/XML only, add a cheap well-formedness check before accepting the declared type (else
    normalise to `text/plain`); accept and strip a `; charset=...` parameter in `declared_type_ok`,
    keeping the stored type canonical. Add property tests for parameter spellings and truncated JSON.
    Risk: parsing a 4 MiB worst case costs CPU — bound it; charset support widens accepted input
    (safe, but pin with a test).

### F6. No domain invariant for version chains or nonce shape
This module knows nothing about `rootArtifactId`/`version`/`clientNonce` (spec/02-data-model.md:119).
The use case checks nonce length only (usecases.rs:1651-1655); the store silently starts a new family
when `root_artifact_id` is unknown in the space (ports.rs:2926-2927), so a stale or foreign root
yields version 1 of a new chain with a plausible id instead of an error.
P6. Add pure domain helpers here: `ArtifactVersion` newtype (positive), `ArtifactNonce` newtype
    (non-empty, <=200 chars), and `resolve_root(existing: Option<&Id>, requested: Option<&Id>) ->
    RootDecision` so the use case can distinguish "new family" / "continue chain" / "refuse foreign
    root". Keep silent re-root as the default decision until the spec owner approves changing it.
    Risk: changing unknown-root behaviour is externally visible (DEVIATION — docs/PARITY.md + owner
    approval); land types + decision with old behaviour preserved, flip only on request.

## Risks and trade-offs
- R1. F3 and F4 can newly reject uploads accepted today; confirm with golden fixtures and a
  differential run before shipping.
- R2. F2 variants and F6's root decision touch the client surface; add, do not rename, and record any
  envelope change in docs/PARITY.md.
- R3. The 4 MiB cap is checked in the use case (usecases.rs:1643) after base64 decode (rpc.rs:2466);
  a domain newtype localises the invariant but cannot move the HTTP body cap (rpc.rs:273) — keep both
  and treat decode amplification as an API-layer follow-up.
- R4. This crate is pure/no-IO by charter: no parser dependency here; if a JSON/ZIP parser is needed,
  put it in app/infra and keep only the decision logic in this module.

## Explicit non-goals
No change to the D-03 accept/refuse matrix, no blob-format parsers in the domain crate, no store or
transport changes, no new feature surface, no HTTP body-limit change in this module.

## Suggested v0.4.0 rollout
Wave A (pure, test-only): P1 newtype, P2 variants + no-echo test, empty/truncated-content tests (F3).
Wave B: P4 marker-boundary fix and P5 JSON/XML + charset handling behind differential tests. Wave C:
P6 version/nonce/root types with current behaviour preserved; seek owner approval before any root
decision change. Acceptance: no invalid name/description/MIME is stored, empty or magic-only blobs are
refused, HTML is accepted only when declared as such, and version/nonce/root invariants are expressible
as domain types with REQ-036-named tests.

_Scope: line refs against `before-m5`; no code/tests/builds ran; empty-artifact and golden coverage
marked GUESS._
