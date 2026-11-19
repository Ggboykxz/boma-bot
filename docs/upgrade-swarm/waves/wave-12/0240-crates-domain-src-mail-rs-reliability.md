# Worker 0240 — `crates/domain/src/mail.rs` — reliability proposal (v0.4.0)

Lens: reliability. Scope: `parse_rfc822` and its helpers only; consumer is
`crates/app/src/usecases.rs::mail_inbound` (REQ-047). Evidence below is source review in this
session (CONFIRMED by reading code); no build, test, or differential run was performed here
(GUESS where runtime impact is inferred). This file is the worker's only write.

## Summary

The parser is deliberately minimal and "refuses loudly" (module doc, lines 1-7), but in practice
it accepts malformed addresses and silently degrades on encodings, normalization and message-shape
changes. Because its output drives trust decisions (allowlist / member / routine sender matching)
and run routing in `mail_inbound`, silent degradation is the primary reliability defect: bad input
becomes either a wrong identity or an unexplained "ignored" outcome with no operator signal.

## Failure modes

**1. Invalid addresses (CONFIRMED).**
- `from` is accepted when it is non-empty and merely `contains('@')` (lines 49-53): `@`, `a@`,
  `@b`, `a@@b` all pass. No domain/local-part/control-character validation.
- `parse_address` uses `rfind('<')` (line 124), so a display name containing `<` or a second
  angle-address misparses; a missing `>` returns the rest of the value as the address
  (lines 126-131), which then only needs an `@` to pass.
- `parse_address_list` splits on bare `,` (lines 143-149). `"Doe, John" <j@e>` and group syntax
  (`Team: a@b;`) are shredded into fragments; the bot address can be lost. Since routing finds the
  bot only in `mail.recipients` (`usecases.rs:2154-2160`), a lost recipient yields silent
  "ignored" (`2161-2166`).
- Duplicate headers are resolved first-wins with no signal (`headers_map`, lines 108-119); a
  second `From`/`To` is discarded invisibly.
- Errors use `DomainError::InvalidName` (lines 37-53) although `InvalidEmail` exists
  (`error.rs:24`) and its Display intentionally never quotes an address (`error.rs:138-144`).
  Mail parse failures are semantically mis-typed; do not "fix" by echoing header values into errors.

**2. Normalization (CONFIRMED).**
- `to_ascii_lowercase` (lines 131, 139) lowercases the local part too (RFC 5321 local parts are
  case-sensitive in principle). The consumer lowercases the sender again (`usecases.rs:2190`) and
  compares to allowlist/member/routine addresses; if stored entries preserve mixed-case local
  parts, legitimate mail is held or dropped (impact GUESS: depends on stored casing).
- RFC 2047 decoding handles only charset `UTF-8` and encodings `B`/`Q` (lines 175-194). Any other
  charset emits the encoded word literally (`out.push_str(word)`, line 195) — silent degradation
  that contradicts the module's "refuse rather than guess" promise.
- RFC 2047 `Q` uses `_` for space, but `decode_quoted_printable` (lines 203-235) never maps it:
  `=?UTF-8?Q?Hello_World?=` yields `Hello_World`. Whitespace between adjacent encoded words is
  also kept, though RFC 2047 §6.2 says to ignore it (lines 155-199).
- `from_utf8(raw.to_vec())` (line 40) copies up to 25 MiB (spec 04-nonfunctional:51). Base64/QP
  transfer decoding (lines 269-283) materializes the whole decoded body before
  `chars().take(MAIL_BODY_MAX)` (line 81): a 25 MiB inbound message can amplify memory well beyond
  the 64 KiB extraction cap. `MAIL_BODY_MAX`/`MAIL_SUBJECT_MAX` truncate silently, never error.
- Bodies are rejoined with `\n` (line 105), normalizing CRLF content; fine for text, wrong if a
  body is ever hashed or verified.

**3. Template drift (CONFIRMED code path; occurrence GUESS).**
- The implicit message contract is `From`/`To`/`Cc`/`Subject`/`Content-Type`, with the bot address
  as `boma+<id>@domain` in `To`/`Cc`. If the relay starts using `Delivered-To`, `X-Original-To`,
  `Envelope-To` or `Resent-To`, or changes the plus-tag format, `mail.recipients` misses the bot
  and mail is silently ignored — no log, metric or held state.
- Multipart support is one level only (doc lines 5-7). Nested `multipart/*` with a `text/plain`
  leaf yields an empty body with no signal. Boundary parsing is case-sensitive and requires no
  spaces (`strip_prefix("boundary=")`, line 243): `boundary = "X"` or `BOUNDARY=` fails, leaving
  the entire body empty. `body.split("--{boundary}")` (line 246) can also split on boundary-like
  text inside a part.
- Unknown transfer encodings pass through raw (line 281), so encoded content can reach a run as if
  it were plain text.

## Concrete hardening for v0.4.0

1. Add a conservative `Addr` newtype + validator used by `parse_address`/`parse_address_list`:
   single `@`, non-empty local/domain, balanced angle brackets, no CR/LF/control bytes, length
   caps. Map parse failures to a dedicated `DomainError::InvalidMail` variant (keep messages
   free of address values). **Risk:** stricter acceptance changes which mail starts runs → record
   as DEVIATION in `docs/PARITY.md` and await approval; prefer "hold + metric" over hard reject
   at the app layer.
2. Replace comma-splitting with an RFC-aware address-list parser (quoted strings, comments,
   groups, angle addresses). **Risk:** hand-rolling invites new bugs; evaluate a vetted crate,
   requiring an ADR, `cargo add`, docs.rs check and license entry in `docs/DEPENDENCIES.md`.
3. Define one `normalize_address()` shared by parsing and address storage, and document the
   local-part case policy. **Risk:** existing allowlist entries may need a migration.
4. Extend RFC 2047: `_`→space for Q, strip inter-word whitespace, decode common charsets (e.g.
   `encoding_rs`), and surface an explicit `Unsupported` reason instead of literal words.
   **Risk:** new dependency and visible subject/name text changes (DEVIATION check).
5. Bound decoding *during* decode (stream cap at `MAIL_BODY_MAX` + small slack), avoid the
   `to_vec()` copy, and turn oversize into a typed error rather than silent truncation. **Risk:**
   new error variant; reconcile the 25 MiB inbound cap with the 64 KiB extraction cap in an ADR.
6. Fail loudly on anomalies: duplicate `From`, unknown transfer encoding, unsupported charset,
   unrecognized recipient header. Emit a typed reason; have `mail_inbound` record a "held" outcome
   plus a metric so drift is observable instead of an invisible "ignored". **Risk:** more mail held;
   policy change needs approval. Domain stays pure — no logging inside `crates/domain`.
7. Tests: `proptest` for panic-freedom on arbitrary byte input and round-trips for address/QP/
   RFC 2047; golden fixtures for quoted-comma recipients, `Delivered-To`, nested multipart, quoted
   boundaries, non-UTF-8 encoded words and oversized bodies. Name tests `req_047_*` for
   `tools/trace_check.py`. Sanitize fixtures of personal data. **Risk:** no existing mail golden
   exists (CONFIRMED), so fixtures must be captured from a controlled relay.

## Cross-cutting risks

- Availability vs strictness: every strictness change alters run routing; gate behind explicit
  approval and keep the app-layer policy (hold vs drop) unchanged by default.
- Public `ParsedMail` field types and constants are re-exported (`lib.rs:78`); changing them needs
  an ADR and is an externally visible DEVIATION.
- Keep all added behavior inside REQ-047; no new features outside the spec.
