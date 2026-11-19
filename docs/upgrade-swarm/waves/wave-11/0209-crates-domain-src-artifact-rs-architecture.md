# Worker 0209 — `crates/domain/src/artifact.rs` architecture proposal (v0.4.0)

Lens: architecture. Scope: read-only review of the 290-line file plus targeted greps in `crates/`
and `spec/`; no code changed, no tests run (read-only assignment). Findings below are **CONFIRMED by
source inspection**; items marked RISK are forward-looking.

## Current structure (as read)

- The module is a bag of free functions and three constants, not a model: no `Artifact` type, no
  name/description/mime value objects, no id newtype. Contrast `ids.rs`, `page.rs`, `bot.rs`.
- The artifact entity itself lives outside the domain: `NewArtifact`/`ArtifactSummary` in
  `crates/app/src/ports.rs`, fields dictated by `spec/02-data-model.md:119`.
- Validation is split across layers: domain owns name/description/mime (`artifact.rs:23,51,153`),
  app owns size and nonce with a magic `200` (`usecases.rs:1641-1655`), api owns the body limit
  (`rpc.rs:273` reuses `ARTIFACT_MAX_BYTES`). No single entry point owns "a valid artifact".
- `sniff_artifact_mime` and `detect_mime` return raw `String`/`Option<String>` built from string
  literals ("image/png", "text/html", ...); callers cannot tell a value was validated.
- Every rejection — MIME lie, oversize, bad name — is `DomainError::InvalidName(String)`
  (`artifact.rs:26,53,156,169,173,...`): the error taxonomy does not describe the failure.
- `lib.rs:46-49` re-exports `ARTIFACT_MAX_BYTES` and the functions but not `ARTIFACT_NAME_MAX` /
  `ARTIFACT_DESCRIPTION_MAX`; the public surface is inconsistent.
- Sniffing is hand-rolled: five signatures (PNG/JPEG/GIF/PDF/ZIP), a 4 KiB lowercased substring
  scan for HTML (`:89-104`), and a full UTF-8 decode for plain text (`:107-114`). No SVG/WebP.
- Tests (`:193-290`) cover name and sniff only: `validate_artifact_description` is exported but
  untested, with no property/fuzz coverage despite these being external-format parsers.

## Target structure for v0.4.0

Keep the module pure and IO-free, but give it a real model and one validated boundary:

```
artifact.rs   MimeType (newtype, parsed at construction)
              ArtifactName / ArtifactDescription (bounded newtypes)
              ArtifactDraft::try_new(name, description, declared, bytes, nonce) -> Result<Self>
              sniff: MimeSniffer trait + MagicByteSniffer (default impl)
ids.rs        ArtifactId  (opaque_id! — currently missing from the id family)
error.rs      InvalidMime / ContentTypeMismatch / ArtifactTooLarge variants
```

`ArtifactDraft` is the single place applying every create-time rule; `usecases.rs:1641-1662`
becomes a thin map from `ArtifactInput` to `ArtifactDraft` to `ports::NewArtifact`. Keep all
REQ-036 test names and `BOMA_*`/wire field names byte-stable (traceability + parity gates).

## Concrete upgrades

### 1. Replace stringly MIME with a `MimeType` newtype (types + clarity)
`MimeType::parse(&str)` owns `declared_type_ok` (`:132-143`) and normalises to lowercase/trim;
`sniff_artifact_mime` returns `MimeType` instead of `String`, and `detect_mime` returns
`Option<DetectedKind>` mapped to a canonical `MimeType`. "Stored type was proven" becomes
representable in the type system, and the string literals scattered in `signature` disappear.
Risk: `usecases.rs:1662-1673` and DTO mapping must convert to/from the new type; `mime_type` is a
stored/wire field, so its serialized form must stay identical or be logged as a DEVIATION.
Also re-export the two MAX constants in `lib.rs` for a coherent public surface.

### 2. Add `ArtifactId` and an `ArtifactDraft` aggregate owning all create rules (cohesion)
`opaque_id!` in `ids.rs` already gives the template; artifacts are the only persisted entity with
a bare `String` id (`app.services.ids.new_id("art")`, `usecases.rs:1664`). Introduce `ArtifactId`,
then move size and nonce checks out of `usecases.rs:1643-1655` into `ArtifactDraft::try_new`, with
one `ARTIFACT_NONCE_MAX` constant replacing the literal `200`. Version-chain invariants (dense,
monotonic, root self-referencing — REQ-036, `spec/02-data-model.md:119`) can later become a
`VersionChain` value object here instead of ad-hoc store logic.
Risk: touches `ports.rs`, the artifact store trait and API DTOs across three crates; land as one
mechanical commit and preserve `clientNonce` replay semantics (`usecases.rs:1656-1661`).

### 3. Give rejections typed `DomainError` variants (diagnostics)
`InvalidName` is currently overloaded for four different failures. Add `InvalidMime(String)`,
`ContentTypeMismatch { declared: String, detected: String }` and `ArtifactTooLarge { limit: usize }`,
and reuse the existing `InvalidNonce`. Keep `Display` text byte-identical where clients branch on it.
Risk: `DomainError` is shared — the `Display` match (`error.rs:67-101`) and any exhaustive matches
must be updated; changing client-visible error strings is a DEVIATION (`docs/PARITY.md` + approval).

### 4. Put sniffing behind a `MimeSniffer` port and widen safe detection (security + perf)
Define `trait MimeSniffer { fn sniff(&self, bytes: &[u8]) -> Option<DetectedKind>; }` with
`MagicByteSniffer` as the default impl, so the domain does not hard-wire one detector. Add SVG
detection (`<svg` before `<html`): the spec forbids rendering artifact HTML in the app document
(`spec/08-ui-ux.md:134`), and SVG is the obvious script-bearing bypass of that rule. Bound
`is_plain_text` to the same 4 KiB head as `looks_like_html` to avoid decoding up to 4 MiB.
Risk: hand-rolling keeps zero deps but more code to fuzz; adopting `infer` (MIT — record in
`docs/DEPENDENCIES.md`) couples us to an external detector. Recognizing more types changes which
uploads are accepted → DEVIATION check before shipping.

### 5. Close the test gaps with proptest and a sniffer fuzz target (quality gate)
Add property tests: name/description accepted iff within bounds with allowed chars;
`declared_type_ok` rejects anything not `token/token`; `detect_mime` never returns an unknown
literal. Add the missing `validate_artifact_description` test and a round-trip case asserting any
accepted `MimeType` re-parses to itself.
Risk: proptest is a dev-dependency only (no runtime cost); keep REQ ids in test names —
`tools/trace_check.py` counts declaration lines, and AGENTS.md requires property tests for
critical rules plus fuzz for external-format parsers.

## Sequencing / cross-cutting risks

1. Do #1 and #3 together (mechanical, but #3 touches a shared enum); run `fmt --check`,
   `clippy -D warnings` and tests after each step.
2. #2 next — widest blast radius across crates; land as one commit, keep idempotency intact.
3. #4 last of the code changes: most security value, most behavior-change risk (DEVIATION gate).
4. #5 can ride alongside any step; never weaken a test to make a refactor compile.
5. No UI/i18n impact. Every claim above is CONFIRMED by source/grep only — re-run the suite in the
   implementing session before flipping statuses to verified-by-test.
