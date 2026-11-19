# Worker 352 — `crates/infra/src/stores/mail.rs` — reliability proposal (v0.4.0)

Scope: the Postgres `MailStore` adapter for REQ-047 inbound bot mail (rules, held queue).
Lens: reliability of the inbound path — duplicate sends, a stuck/lost outbox, and delivery
tracking. All claims **CONFIRMED** unless marked **GUESS**.

## Evidence (read in this worktree)

- `hold` is a bare `INSERT` with a **caller-supplied** id and no conflict clause
  (`mail.rs:110-139`). The id is minted fresh per call in the use case (`usecases.rs:2365`).
- `mail_inbound` re-runs the entire decision flow on every relay POST: rule lookup, member/allow
  checks, message insert + run create for admitted senders, or a new `hld` row for `hold`
  (`usecases.rs:2135-2382`). No inbound `Message-ID` or idempotency key is persisted anywhere.
  GUESS: the relay retries webhook POSTs without a stable dedup token — standard behaviour, but
  confirm against the relay config before sizing the dedup window.
- `take_held` is `DELETE ... RETURNING` — a single statement with a single row-level winner
  (`mail.rs:179-209`). This part is already sound; do not replace it with read-then-delete.
- `mail_held_release` calls `take_held` **first**, in its own tenant transaction, then does bot
  lookup, message insert and run create in separate store calls (`usecases.rs:2495-2566`).
  `tenant::with_tenant` opens a fresh transaction per call (`tenant.rs:41-63,107+`). A failure
  after the take leaves the held row deleted and no run — the message is lost.
- No outbox, retry row, delivery status, or lifecycle link (held → released → run/message) exists
  in `held_mail`/`bot_mail_rules` (`migrations/0040_bot_mail.sql:8-39`). Drop is audited; release
  is not (`usecases.rs:2342-2354` vs `2495-2566`).
- `tenant::unavailable` collapses **all** sqlx errors (including unique violations) to
  `AppError::Unavailable`; `is_unique_violation` exists but is unused here (`tenant.rs:153-163`).
  Conflict-aware dedup must map the conflict explicitly, not through `unavailable`.

## Failure modes

- **FM-1 Duplicate sends/triggers.** A redelivered webhook has no idempotency anchor. Admitted
  senders get a new message + a new run per retry; `hold` senders get a new `hld` row per retry.
  "Starts or continues work" becomes N runs; side effects and model cost multiply. This is the
  same duplicate-delivery window the rebuild claimed to drop by killing the legacy Graphile
  dispatch (`05-known-defects.md:43-44`) — retries re-open it at the store boundary.
- **FM-2 Outbox stuck / lost work.** `take_held` commits a delete before the release work; no
  transaction or outbox spans delete → message → run. A crash, an archived bot, a model-choice
  failure, or a transient DB error after the take loses the message with nothing to retry, and a
  client retry then gets `NotFound`. Conversely, if the relay POST fails after a successful
  `hold`, the retry re-runs the flow and duplicates (feeds FM-1).
- **FM-3 No delivery tracking.** The store records only row presence: no status, no outcome
  reference, no attempt count, no audit on release. Operators cannot answer "was this mail
  handled, by whom, and did it start a run?", cannot spot a stuck delivery, and cannot prove a
  single-admission promise. PII (sender address) must stay out of logs/audit metadata.

## Concrete hardening (v0.4.0)

- **H-1 Idempotent inbound anchor (high value, small diff).** Add a receipt to the migration:
  `mail_inbound_receipts(message_key text PK, bot_id text, result text, created_at timestamptz)`,
  where `message_key` is the RFC 822 `Message-ID` (fallback: hash of sender+recipient+subject+date
  +body). Make `hold` an `INSERT ... ON CONFLICT DO NOTHING RETURNING`, and let `mail_inbound`
  short-circuit on a duplicate receipt *before* inserting a message/creating a run. Detect the
  conflict with `is_unique_violation`, not `unavailable` (`tenant.rs:159`). *Risks:* `Message-ID` is
  caller-controlled and may be absent or spoofed — define the fallback and a dedup window/GC so
  receipts do not grow without bound; a hash-only key can collide across genuinely distinct mail.
  Changing the `MailStore` return type is an internal API change (no wire DEVIATION). Confirm the
  relay retry semantics (GUESS above) before fixing the window.

- **H-2 Atomic release with a retryable outbox (high value, highest blast radius).** Introduce
  `mail_release_outbox(held_id text PK, space_id, bot_id, payload jsonb, status, attempts,
  available_at, created_at)`. In one tenant transaction, atomically claim the held row (keep the
  `DELETE ... RETURNING` single-winner) and enqueue the job; a worker then inserts the message,
  creates the run, and marks the row done. Crashes leave a bounded-retry row with a dead-letter
  state — never a silently lost message. Alternative: extend the port to accept a shared executor
  so delete + message + run commit together. *Risks:* refactor across store boundaries; needs
  testcontainers tests, a cancellation-safety review, backoff/dead-letter policy, and short lock
  scope. Must not regress the existing single-winner `take_held` guarantee.

- **H-3 Delivery tracking + audit (medium).** Write `status` (held/released/dropped/failed),
  `attempts`, and the resulting `run_id`/`message_id` onto the receipt/held lifecycle; emit an
  audit event on release as drop already does, keyed by `held_id` with no raw email in metadata.
  *Risks:* audit metadata shape may be asserted by golden/contract tests — verify first; keep
  sender addresses out of structured logs (`AGENTS.md` observability rule).

- **H-4 Bounded held queue (low/medium).** `held_list` is hard-capped at `LIMIT 50` with no
  pagination (`mail.rs:150-155`); add keyset pagination plus a retention/cleanup job so the
  "stuck" queue cannot grow unbounded. *Risks:* signature change is internal API; keep the
  current default call working.

- **H-5 Tests (low risk).** testcontainers integration tests named with the REQ id:
  `req_047_a_redelivered_inbound_mail_starts_one_run`, `req_047_a_failed_release_is_retryable`,
  `req_047_a_release_is_recorded_once`, plus a concurrent-double-take assertion.

## Ordering / risk summary

1. H-1 + H-5 first: idempotency at the store boundary, no wire change.
2. H-3 next, gated on golden/contract verification and PII review.
3. H-2 last: the durable reliability win, but it is a cross-store transaction refactor and needs
   retry/dead-letter tests. H-4 can ride along.
4. Net: the real defect is that reliability currently depends on the relay calling exactly once;
   the main risk is not the SQL but adding a dedup/retention policy (externally visible only via
   internal APIs) without confirming relay retry behaviour and a safe dedup window.
