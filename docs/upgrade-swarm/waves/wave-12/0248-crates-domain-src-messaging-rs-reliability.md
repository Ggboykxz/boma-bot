# Worker #248 — `crates/domain/src/messaging.rs` — reliability lens — v0.4.0 proposal

## Scope and status

Read-only review of `crates/domain/src/messaging.rs` (531 lines), its re-export in
`domain/src/lib.rs:89-91`, and the two consumers found by grep: `app/src/usecases.rs`
(`messaging_inbound` 3073-3232, `deliver_external_reply` 3246+) and
`infra/src/stores/messaging.rs:396-449` (`resolve_external_thread`). No code changed. Claims are
CONFIRMED from source reads this session unless marked GUESS. No tests were run by this worker.

## What the module does today

Pure, IO-free signature verification and payload parsing for three platforms (Slack, Telegram,
Generic HMAC). `verify_*` are constant-time; `parse_*` turn every rejection into
`DomainError::InvalidName`. `ChannelMessage` carries `message_id: Option<String>` (line 81).
`external_key` (376-382) joins `platform:workspace:conversation` with `:`.

## Failure modes

1. **Dedup is impossible with the data that survives parsing.** Slack retries deliver the same
   `event_callback` several times; Telegram redelivers the same `update_id` until acked. The Slack
   parser captures `ts` but the envelope struct (189-198) has no `event_id`; the Telegram struct
   (258-264) has no `update_id` at all and keeps only `message.message_id` (326); Generic leaves
   `message_id` optional (343, 370). CONFIRMED downstream: `messaging_inbound` inserts a message and
   creates a run on *every* accepted delivery with `client_nonce: None` (usecases.rs:3199, 3217),
   even though `stores/messages.rs:8` and `stores/runs.rs:5` already implement nonce idempotency.
   One redelivery therefore means one duplicate message + one duplicate run.
2. **Lost messages from collapse-to-one-error and uniform refusal.** All parse rejections are the
   same untyped `InvalidName` (225-246, 299-314, 352-363) and `messaging_inbound` maps every
   refusal to `AppError::NotFound` (usecases.rs:3065-3071, 3147-3155). Non-2xx makes platforms
   retry, so *permanent* rejects (edits, bot messages, `url_verification`) burn the platform's
   retry budget; a transient malformed body at the boundary is indistinguishable from an
   intentional reject. Outbound replies are worse: `deliver_external_reply` logs and swallows
   platform failures with no retry queue (usecases.rs:3236-3239) — a reply is permanently lost if
   the platform is down at that instant. GUESS: the API maps `NotFound` to 404; verify in wave.
3. **Delivery order is silently assumed.** No sequence/order field is exposed. Slack `ts` is a
   string (`"1234.5678"`); lexicographic order is wrong (`"9.9" > "10.1"`). Telegram `update_id`
   is discarded. Nothing states that ingress is unordered, so a downstream consumer that sorts or
   assumes FIFO ordering will misbehave under concurrent webhook deliveries.
4. **Silent content loss and empty identities.** `text.chars().take(CHANNEL_TEXT_MAX)` (252, 325,
   369) drops everything past 8 000 chars *before* the run sees it, which can cut a command
   mid-token; `team_id`/`user`/`from` use `unwrap_or_default()` (249, 251, 321-324), producing
   accepted messages with empty workspace/user ids that are hard to attribute or dedup.
5. **Replay and edge-case holes.** `verify_slack` computes `(now - sent).abs()` on an
   all-digits i64 (130-136): the subtraction/abs can overflow and panic in debug builds for
   extreme inputs; there is no per-signature nonce cache, so any signed body replays freely inside
   the ±5-minute window. `verify_generic` matches the `sha256=` prefix case-sensitively (155), so
   an upper-cased prefix is a false negative. `external_key`'s `:` delimiter collides whenever a
   component contains `:` (e.g. workspace `a:b` + conversation `c` == workspace `a` + `b:c`);
   with empty workspaces it also collapses separate installations.

## Concrete hardening for v0.4.0

- **H1 — canonical delivery key (dedup).** Add `event_id: Option<String>` to `SlackEnvelope` and
  `update_id: Option<i64>` to `TelegramUpdate`; expose `ChannelMessage.delivery_key: String` as
  `platform-prefixed canonical id` (Slack `event_id` → fallback `ts`; Telegram `update_id`; Generic
  required `messageId`, else a documented hash of body+fingerprint). Wire app to pass it as
  `client_nonce` to both `messages.insert` and `runs.create` so redelivery is a store-level no-op —
  no new column. CONFIRMED feasible from the cited idempotent stores.
- **H2 — typed rejection.** Introduce `pub enum RejectKind { Permanent, Transient }` (or a
  structured `InboundReject`) returned by the parsers alongside the error; have ingress answer 2xx
  for permanent rejects (stop wasting retries) and 5xx for transient ones (invite retry). Keep all
  current `Display` strings byte-identical for goldens.
- **H3 — verifier robustness.** Replace line 136 with `now.abs_diff(sent) > SLACK_MAX_SKEW_SECONDS as u64`
  (or `saturating_sub`); accept `sha256=` case-insensitively; reject missing `team_id`/`user`/`from`
  rather than defaulting to empty (subject to spec confirmation — see Risks). Add a proptest over
  timestamp strings including i64 extremes, non-ASCII digits and empty bodies.
- **H4 — ordering and truncation contract.** Document that ingress is unordered and expose a typed,
  `Ord`-able `Sequence` (Slack `ts` parsed to `(u64, u64)`, Telegram `update_id`) so consumers can
  order deliberately. Stop truncating the text used for work: process the full payload and cap only
  the stored display copy; add a test asserting no silent loss past `CHANNEL_TEXT_MAX`.
- **H5 — key hygiene and tests.** Length-prefix `external_key` components (or hash the triple) and
  include the installation/tenant id; add a property test that distinct triples never collide.
  Name new tests with REQ ids per AGENTS.md (e.g. `req_048_redelivery_is_idempotent`).

## Risks and mitigations

- **Externally visible changes:** H2 status-code changes, H3 stricter field rejection and H4 text
   semantics are DEVIATIONs — record in `docs/PARITY.md` and await approval. Mitigation: land H1,
  H5 and the verifier fixes first (no wire change); gate H2-H4 behind parity entries.
- **Wrong dedup key suppresses real work:** `event_id` fallback to `ts` can collide across distinct
  events sharing a timestamp. Mitigation: prefer `event_id`, emit a metric on dedup hits, and keep a
  manual replay path.
- **Permanent/transient misclassification drops messages:** default unknown failures to
  *transient* (retry), never permanent; add a counter for each class.
- **Spec compatibility:** requiring Generic `messageId` may break producers that omit it; GUESS —
  resolve in `docs/STATE.md` before enforcing, falling back to the hashed key otherwise.
- **Reply loss remains out of scope here:** H1-H5 do not add an outbound retry queue; that lives in
  `deliver_external_reply` and should be a separate brief/ADR, not a silent change to this module.
