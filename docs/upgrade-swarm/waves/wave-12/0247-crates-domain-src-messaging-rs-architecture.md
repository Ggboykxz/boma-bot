# Worker #247 — `crates/domain/src/messaging.rs` — architecture lens — v0.4.0 proposal

## Scope and status

Read-only review of `crates/domain/src/messaging.rs` (531 lines), `crates/domain/src/lib.rs`
(re-exports, lines 89-91), and the direct consumers `crates/app/src/usecases.rs`
(`messaging_inbound`, lines 3073-3159), `crates/api/src/public.rs` (lines 494-516),
`crates/infra/src/stores/messaging.rs` (`parse_platform`, lines 40-44),
`crates/infra/src/stores/rows.rs` (lines 314-317), `crates/domain/src/thread.rs` (line 11)
and `crates/app/src/ports.rs` (lines 3477-3560). No code was changed. All claims below are
CONFIRMED from those files; anything inferred is marked GUESS.

## Messaging domain model today

One module owns three concerns: (1) a `ChannelPlatform` enum (`Slack | Telegram | Generic`)
with `as_str`/`parse`, (2) three verifiers (`verify_slack(secret, ts, body, provided, now)`,
`verify_telegram(expected, provided)`, `verify_generic(secret, body, provided)`) plus a shared
`constant_time_eq`/`to_hex`, and (3) three payload parsers producing one flat `ChannelMessage`
struct of raw `String` fields (`workspace_id`, `conversation_id`, `user_id`, `text`,
`Option<String> message_id`). Three Slack/Telegram/Generic serde payload trees are private to
the module. `external_key()` string-formats `platform:workspace:conversation`.

Architecture shape CONFIRMED at call sites: platform dispatch is **shotgun surgery**. The app
ingress matches `platform` twice — once to pick a verifier + secret source (Slack: `slack.signing_secret`
config; Telegram: `reveal_webhook_secret` store; Generic: `reveal_token` store), once to pick a parser —
and `api/public.rs` has a third match for the handshake path. Infra adds two more mappings
(`parse_platform`, row decode). The enum's own doc says Teams is "one more adapter and one more
verifier, nothing structural" — CONFIRMED false in practice: it means editing 6+ files across
four crates.

## Findings

1. **Flat, stringly-typed identity; invalid states representable.** `workspace_id` and `user_id`
   are empty strings for Telegram/Generic and for missing senders (lines 319, 324, 366); all id
   fields are unconstrained `String`. `external_key()` concatenates with `:`, so ids containing a
   colon are ambiguous. AGENTS.md requires newtypes and unrepresentable invalid states; neither holds.
2. **Verify→parse order is convention, not enforced.** Parsers accept raw `&[u8]`; nothing stops a
   caller from parsing an unverified body. The security property in the module doc ("prove the caller
   is the platform, then hand the verified message") is upheld only by discipline in `usecases.rs`.
3. **Error taxonomy wrong.** Every parse failure returns `DomainError::InvalidName` with prose;
   verification returns bare `bool`. Refusal reasons are erased, and `InvalidName` is already an
   overloaded adapter bucket (see worker #228).
4. **Triplicated logic.** `text.chars().take(CHANNEL_TEXT_MAX).collect()` appears three times;
   platform dispatch appears in four crates; `ChannelMessage` construction is copy-pasted.
5. **Test gap.** Only hand-written unit tests; no proptest/fuzz for three external-format parsers,
   which AGENTS.md mandates.

## Concrete upgrades for v0.4.0

- **A1 — One dispatch surface on `ChannelPlatform`.** Add associated fns `verify(...) -> bool` and
  `parse(body) -> Result<ChannelMessage, _>` that delegate to the per-platform helpers, and migrate
  `usecases.rs` (both matches) and `api/public.rs` to them. Keeps secret acquisition in the app
  layer (domain stays IO-free) but removes the duplicated *shape* of dispatch. Adding a platform
  becomes one `match` inside domain plus one secret-source arm in app.
- **A2 — Validated construction + id newtypes.** Add `ChannelMessage::new(...)`/`TryFrom` that
  enforces non-empty, capped text and consistent optional ids; introduce a `ChannelText` newtype
  (non-empty, `CHANNEL_TEXT_MAX`-capped) and a structured `ConversationRef` whose `external_key()`
  escapes the separator instead of naive `format!`. Keep existing raw fields readable so DB rows
  and golden fixtures do not change in this release (see Risks).
- **A3 — Encode the verify→parse invariant.** Introduce a `VerifiedBody<'a>` newtype obtainable only
  from the verifiers; make `parse_*` (and the A1 `parse`) accept it. Then no code path can parse an
  unproven payload. Carve out the Slack `url_verification` handshake explicitly (it is parsed by
  `slack_url_verification_challenge` before/independently of the event path).
- **A4 — Typed messaging error + verification outcome.** Add `DomainError::InvalidMessage` (or a
  `MessagingError` converted at the boundary) for parse refusals, and replace verifier `bool` with
  a `VerificationOutcome` carrying a reason enum. Keep the edge behavior uniform — the module doc
  requires "every failure looks like every other failure", so `usecases.rs` still maps all outcomes
  to `AppError::NotFound`. Add `#[non_exhaustive]` on the reason enum.
- **A5 — Parser correctness tests + de-duplication.** Add proptest/fuzz tests for all three parsers
  (arbitrary bytes never panic, truncation at `CHANNEL_TEXT_MAX` holds) and a single
  `truncate_text` helper reused by all three. Keep `chars().take()` semantics byte-identical — it
  counts Unicode scalar values, and changing it is a DEVIATION.

## Risks and mitigations

- **Source break at consumers.** Newtypes/outcome types break app, infra, testkit and API call sites.
  Mitigation: land A1/A2 as additive constructors + accessors first; switch fields/types in a
  follow-up wave, not atomically with A3/A4.
- **Externally visible change.** `external_key()` output and error messages may be asserted by
  routes/goldens. Any change to key format or Display is a DEVIATION — record in `docs/PARITY.md`,
  wait for approval. Default to byte-identical output in v0.4.0; escaping only applies once approved.
- **IO must stay out of domain.** A1 cannot hide secret loading (Slack config vs two store reads);
  forcing it in would violate the pure-domain crate boundary. Mitigation: dispatch only the pure
  verification/parse step; keep secret provision behind an app port.
- **Uniform-refusal regression.** Returning reasons (A4) risks leaking them to clients. Mitigation:
  reasons are internal-only; the edge still returns one uniform 404, with a test asserting no
  reason string reaches the response.
- **Over-abstraction for three platforms.** A trait object per verifier adds ceremony for little
  gain today. Mitigation: prefer enum dispatch (A1) over traits; revisit only when Teams lands.
- **Handshake carve-out.** Gating `parse` on `VerifiedBody` (A3) could break the URL-verification
  flow; keep that path on a raw-body helper and cover it with a dedicated test.
