# 0182 — `crates/app/src/push.rs` — reliability proposal (v0.4.0)

Worker #182 · module `crates/app/src/push.rs` (499 lines) · lens: reliability · READ-ONLY review of
`rebuild-workspace/.worktrees/before-m5`. Proposals only; no code changed here. English per repo disk
convention. Claims: **CONFIRMED** = read in this tree this session; **GUESS** = inference needing a test.
Spec anchor: REQ-064 (Web Push, MUST), `spec/01-requirements.md`:512-522 — notably "pushes never carry
conversation content", "per-(space, member, bot) daily cap", "dead subscription deleted after repeated
permanent failures". Complements 0181 (architecture); overlap is intentional where reliability differs.

## Baseline (CONFIRMED)

- Best-effort contract: every failure logs and returns; a run never fails (`:3-5`, `:291-312`).
- Payload is pinned to five content-free keys; titles come from a fixed fr/he/en catalog; `url` and
  `tag` embed only typed ids (`build_push_payload` `:266-285`, `title_for` `:127-141`).
- VAPID private key is redacted from `Debug` (`:34-41`); malformed config fails at boot (`:50-73`).
- `deliver` (`:401-469`) maps 2xx → success, 401/403/404/410 → permanent, other/transport errors →
  transient; the store deletes a subscription on the 3rd permanent failure
  (`crates/infra/src/stores/notifications.rs`:327-373). Client: 5 s timeout, no redirects (`:471-482`).

## Failure modes

### FM1 — Content / secret leakage (high impact, low likelihood)

Pushes are content-free by construction (CONFIRMED), but the *observability* path is not. Every
`tracing::warn!(%error, ...)` on the webhook path (push.rs `:250`; also
`crates/infra/src/messaging_http.rs`:105) formats a raw error that can embed the full request URL —
and `webhook.expose_secret()` (`:232`) is a bearer-secret URL (CONFIRMED that `%error` is logged;
GUESS that reqwest/validation `Display` includes the URL — verify with a unit test before claiming a
leak). `check_outbound_url` reasons flow into `AppError::Rule(InvalidName(reason))`, also logged via
`%error` (GUESS on contents). `note`/`note_success` log only store errors (`:484-499`), which is safe.
Titles cannot be attacker-steered: unknown locales fall back to `en` (`:128-140`). Content-free is
convention-only (a future `json!` field could violate it) — CONFIRMED structural gap, see 0181.

### FM2 — Dedup (medium)

`tag: run:{run_id}` (`:281`) dedups only in the browser; there is no app-level idempotency.
`record_send` mints a fresh `"log"` id per send (`:241-246`, `:453-458`) and `notification_log` has no
run/tag column (`notifications.rs`:280-292), so repeated `notify_run_waiting` for the same run (e.g.,
several question cards) produce N browser pushes, N cap rows and N webhook POSTs (CONFIRMED shape;
GUESS on how often the worker re-notifies one run). Webhooks have no tag concept, so a browser-deduped
run still spams channels. Opposite risk: a dedup that is too aggressive suppresses a legitimate second
"needs your answer" after the first was seen (GUESS until a product rule is chosen).

### FM3 — Delivery failure (high)

1. **Webhook 4xx/5xx is miscounted as success.** `post_json` returns `Ok(status)` for *any* status
   (`messaging_http.rs`:108); `push_destinations` treats every `Ok(_)` as delivered, decrements the
   budget, records a send and logs "succeeded" (`:239-248`). A 500 therefore hides a failed delivery
   and burns cap (CONFIRMED by both files — highest-value fix in this module).
2. **No transient retry.** A single 5xx/network blip drops the notification forever (`:459-467`);
   best-effort by spec, but silent. GUESS on real-world frequency.
3. **Accounting is lossy.** `record_send` errors are `let _ =` (`:242-246`, `:454-458`), so a DB write
   failure under-counts the cap; `note_failure`'s `NotificationFailureOutcome` is discarded (`:484-492`),
   so strike/delete events are invisible to the app. No metrics or counts on any drop path (CONFIRMED).
4. **Unbounded fan-out latency.** Browser loop (`:369-384`) and webhook loop (`:198-253`) are sequential;
   N subscriptions × 5 s plus M webhook calls can hold the caller for minutes (CONFIRMED; GUESS on
   `self.http` timeout in `messaging_http.rs`). Deleting on 401/403 may remove a valid subscription
   after VAPID key rotation (CONFIRMED classification; GUESS on occurrence). `http_client()` silently
   falls back to a bare `Client::new()` if its build fails, losing timeout/no-redirect policy
   (`:480`; GUESS that this path is reachable). `deliver` does not re-check endpoint safety at send time
   (`:438-448`), though subscribe-time https/no-credentials/DNS checks exist
   (`crates/api/src/rpc.rs`:4214-4255); residual rebinding risk (GUESS).

### FM4 — Daily cap (high)

The cap is checked, not reserved. `push_one` checks `count_today` once (`:340-352`) and then loops over
all subscriptions with **no in-loop budget** (`:369-384`): one member with N subscriptions can send N
pushes against a cap of 3 (CONFIRMED). `push_destinations` re-reads `count_today` (`:187-192`) and keeps
a local `remaining`, but check and `record_send` are separate transactions, so concurrent runs for the
same (space, user, bot) all pass the check (CONFIRMED structure; GUESS on concurrent notifications).
Day boundary is UTC (`notifications.rs`:311-314). Cap-read failure fails closed — skips delivery
(`:189-192`, `:348-351`), protecting the cap but losing the notice; unobservable (CONFIRMED).

## Concrete hardening (v0.4.0)

- **P1 — classify webhook status (P0 fix).** In `push_destinations`, require `(200..300).contains(&status)`
  inside `Ok(status)`; treat 3xx/4xx/5xx as transient failure and do NOT decrement `remaining` or call
  `record_send`. Risk R1: changes externally visible "delivered" accounting and webhook retry behaviour —
  record in `docs/PARITY.md` if any receiver relied on the old success log.
- **P2 — one shared, per-send budget.** Compute `remaining` once in `push_one`, pass it to both loops,
  decrement only on confirmed success, and stop at zero (fixes N-subscription overshoot). Then add an
  atomic `reserve_sends(context, bot_id, n)` port (SQL transaction + test doubles) for the TOCTOU race.
  Risk R2: port change across app/infra/testkit; a rolled-back reservation must not burn budget.
- **P3 — safe error logging.** Log a redacted status/kind (`status=%status`, `kind=...`) instead of
  `%error` on webhook/push paths; never format a URL or secret. Add a test that a sentinel token placed
  in a webhook URL never appears in captured tracing output. Risk R3: loses diagnostic detail — keep a
  debug-level, allow-listed reason string (no URL, no headers).
- **P4 — dedup key.** Add `dedup_key` (e.g. `run:{run_id}:{kind}`) to `notification_log` with a unique
  constraint (migration), make `record_send` idempotent, and skip fan-out when a row exists inside a
  cooldown window. Apply the same key as an `X-Boma-Dedup-Key` header on webhook POSTs. Risk R4: wire
  addition to external receivers and a migration; choose the window with the product owner (GUESS), and
  allow a fresh notice when the previous waiting card is no longer pending.
- **P5 — one outcome sink.** Return a typed `Delivered | Gone(permanent) | Transient` from `deliver`,
  then record success/failure, log one structured event and emit a counter in one place; propagate the
  store's `NotificationFailureOutcome` so deletion is visible. Add bounded retry (e.g. 2 attempts,
  jittered backoff) for transient statuses only. Risk R5: retry can duplicate webhook delivery — lean on
  P4's key; keep 5 s timeout, no redirects, and 401/403/404/410 ⇒ permanent semantics from REQ-064.
- **P6 — bound the caller.** Fire-and-forget via `spawn` (or bounded `join_all`) with a total deadline,
  and check `check_outbound_url` again at delivery. Risk R6: detached tasks complicate shutdown/draining
  (AGENTS.md); a post-send check could reject a subscription saved before a config change.

## Tests (traceability in the name)

`req_064_webhook_5xx_is_not_counted_as_delivered`, `req_064_n_subscriptions_obey_daily_cap`,
`req_064_concurrent_runs_do_not_exceed_cap`, `req_064_repeated_waiting_dedups`,
`req_064_webhook_secret_absent_from_logs`, `req_064_permanent_failure_deletes_after_three_strikes`.

## Risks / non-goals

R1–R4 above are DEVIATION candidates (accounting, wire header, migration) — get owner approval before
shipping. Keep the pinned five-key content-free payload, exact title strings, en fallback, UTC day and
cap-reached-silence byte-identical; do not weaken SSRF checks or log secrets. Evidence: CONFIRMED —
`push.rs`:1-499; `messaging_http.rs`:88-109; `notifications.rs`:265-394; `rpc.rs`:4214-4255; spec :512-522.
GUESS — reqwest error `Display` contents, concurrent-notification frequency, cooldown window, the
`self.http` timeout, key-rotation/rebinding likelihood. Nothing was executed in this pass.
