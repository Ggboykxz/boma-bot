# 0181 — `crates/app/src/push.rs` architecture proposal (v0.4.0)

Module: `crates/app/src/push.rs` (499 lines, read this session on `before-m5`).
Lens: architecture / structure. Spec anchor: REQ-064 (Web Push, MUST),
`spec/01-requirements.md` :512-522, and REQ-065 (destinations, MUST) :610-618.
Status: "current shape" is CONFIRMED by reading the file, `crates/app/src/ports.rs`,
`service.rs` and the spec. Upgrades and risks are PROPOSAL/GUESS (design inference,
not implemented). English per repo disk convention; spec/ADR/charter stay authoritative.

## 1. Current shape (CONFIRMED)

One app-layer module carries four distinct responsibilities:

- **Signing config** — `VapidConfig` reads/validates `BOMA_VAPID_PRIVATE_KEY` / `_SUBJECT`
  from env, never prints the key (custom `Debug`), and signs via `parse_builder` (PEM or
  base64url). `None` means "built but inert".
- **Content-free payload** — `build_push_payload` emits exactly `{v, kind, title, tag, url}`
  with `serde_json::json!`; `title_for` is a hardcoded fr/he/en catalog with an en fallback.
- **Fan-out and gating** — `push_one` checks VAPID → policy (`allows`) → `count_today` cap →
  locale → subscriptions, then delivers; `push_destinations` posts the same title to enabled
  Slack/Teams webhooks through `MessagingTransport::post_json(..., false)`.
- **Raw wire delivery** — `deliver` rebuilds the web_push request, decomposes it with
  `into_parts()`, converts the method and copies headers into a `reqwest::Request`, and executes
  it on a module-local `OnceLock<Client>` (5 s timeout, no redirects).

Public entries `notify_run_waiting` / `notify_run_outcome` are called from `worker.rs:184,690`;
every failure logs and returns so a run never fails. `vapid_policy_title` (:143-149) is a pure
wrapper over `title_for`, and `push_destinations` carries an unused `outcome` (`let _ = outcome`).

## 2. Architecture issues (CONFIRMED)

1. **Layering leak.** Raw `reqwest` transport lives in `app`, bypassing the port seam used for
   destinations (`MessagingTransport`, `ports.rs:3682-3723`). It is not injectable, so `deliver`
   cannot be unit-tested without a live HTTP endpoint, and web_push's request internals are
   copied by hand into reqwest — brittle against `web_push` upgrades.
2. **Cap is checked, not reserved.** `count_today` and `record_send` are separate calls in two
   places (`push_one` :340-352, `push_destinations` :187-192), so concurrent runs can all pass the
   check (TOCTOU). The browser loop (:369-384) has no in-loop budget at all, while the destination
   loop decrements `remaining` — a member with N subscriptions can exceed the daily cap.
3. **Content-free is convention-only.** A doc comment plus a `json!` literal are the only guard
   against a future field carrying a thread id or tool args; nothing makes that state unrepresentable.
4. **Titles are hardcoded in `app`.** The catalog bypasses any i18n layer and parses locale with a
   naive `split('-')`. Titles are user-visible and likely golden-asserted, so wording is not free.
5. **Failure handling is scattered.** Five early `note(..., false)` branches in `deliver`, several
   `let _ =` writes, and no single classification/metrics hook, contrary to the repo's
   observability standard.

## 3. Concrete upgrades for v0.4.0 (with risks)

**P4 — One typed title source; delete the wrapper (do first, pure refactor).** Put
`enum NotificationTitle { Waiting, Completed, Failed }` with one `render(locale) -> &'static str`
holding the catalog, remove `vapid_policy_title`, and have both callers use it. Risks: (a) the
exact strings are user-visible and probably asserted by goldens — **change no wording** in v0.4.0;
(b) keep the en fallback byte-identical; (c) locale parsing stays a documented limitation until a
spec decision is recorded. No wire change.

**P3 — Typed payload instead of `json!`.** Define `#[derive(Serialize)] struct PushPayload<'a>`
with `v`, `kind`, `title`, `tag`, `url` and serialize it, so adding a field is a reviewable edit.
Add a unit test asserting the exact string for REQ-064. Risks: (a) `json!` emits keys in sorted
order under serde_json's default map; a struct emits declaration order — declare fields
`kind, tag, title, url, v` or the bytes change and a DEVIATION is required (GUESS: verify the
`preserve_order` feature in `Cargo.toml`); (b) keep `v: 1`; (c) no field may ever hold conversation
content.

**P2 — Atomic cap reservation and one shared budget.** Add a transactional
`reserve_sends(context, bot_id, n) -> Option<u64>` to `NotificationStore` (`ports.rs:4480-4540`),
call it once in `push_one`, and thread a single `remaining` through both the browser and
destination loops, decrementing per successful send. Risks: (a) port change means an infra impl
plus test doubles; (b) a failed/rolled-back reservation must not burn budget; (c) preserve "cap
reached → silent, run unaffected"; (d) add a concurrent-runs test. Interim fallback if the port
change is too invasive: one local `remaining` in `push_one` fixes the N-subscription overshoot but
not the cross-process TOCTOU.

**P1 — `PushTransport` port; move raw HTTP to infra.** Add a port taking plain data
(endpoint, p256dh, auth, signature, payload) and returning a typed
`Delivered | Gone | Transient`, implement it with the existing reqwest client in `crates/infra`,
and reduce `deliver` to signing plus a port call. Risks: (a) keep 5 s timeout, no redirects and
the `EndpointSafetyChecker` write-time SSRF rule (`ports.rs:4542-4557`); (b) pass bytes/strings,
not web_push types, across the trait to avoid generic leakage; (c) larger diff and new wiring in
`service.rs`; (d) no wire/API change.

**P5 — One outcome sink.** Have `deliver` return the typed result from P1 and record
`note_success` / `note_failure` / `record_send`, log one event and emit one metric in a single
place. Risks: (a) preserve 401|403|404|410 ⇒ permanent and the store's third-permanent-delete rule
(`ports.rs:4520-4532`); (b) metric name/labels must follow `metrics.rs` (GUESS — check before
promising); (c) never log endpoint URLs, webhook URLs or key material.

## 4. Ordering, invariants, non-goals

Order: **P4 → P3 → P2 → P1 → P5** (P5 depends on P1's typed status; P4/P3 are cheap and
testable first). Invariants to preserve: payload stays the pinned five-key, content-free shape;
webhook secrets stay sealed and out of logs/DTOs; cap, policy and SSRF checks never weaken.
Non-goals: no new locale or wording, no payload key/value change without a DEVIATION and owner
approval, no second datastore or service, no ADR edit without the owner.

## 5. Evidence

CONFIRMED — read this session: `push.rs` :1-499; `ports.rs` :3680-3723, 4480-4557;
`service.rs` :266,288-290; `worker.rs` :184,690 (grep); spec :512-522, :610-618.
GUESS — serde_json key order under current features; whether goldens assert exact titles; the
metric naming convention in `metrics.rs`; whether web_push request building can move cleanly
behind a string/bytes-only port.
