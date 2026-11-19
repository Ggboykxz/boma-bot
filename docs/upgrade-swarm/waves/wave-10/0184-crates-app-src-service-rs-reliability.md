# 0184 — crates/app/src/service.rs — reliability (v0.4.0)

Worker #184 · `crates/app/src/service.rs` · reliability · READ-ONLY review of `before-m5`.
The module is the composition root: `Stores` (50+ `Arc<dyn ...Store>`), `Services` (flags, secrets,
transports, timeouts), `App`, plus `Actor`, `SendOutcome`, `BotWithThread`, `Failure`. It performs no
IO itself, so its reliability surface is the *promises it makes to use cases* and the invariants it
does (or does not) enforce at construction. Line refs against `before-m5`; file facts CONFIRMED by
reading, test/build status GUESS. No code ran.

Spec/AGENTS anchors: `ports.rs:9-13` ("sequences allocated by the store", "a multi-table write is
one port call"); `usecases.rs:11-14` (create_bot + thread in one transaction; send_message replays a
nonce); persistence rule "transactions for multi-step writes, idempotency keys where the spec has
retries or payments"; async rule "a timeout on every outbound call"; config rule "validated at
startup (fail fast)"; observability rule "no secrets or personal data in logs".

## What is already right (keep, don't re-propose)

- Every port trait is `Send + Sync` (CONFIRMED: 40+ declarations in `ports.rs`), so the `Arc` fields
  are safe to share across Tokio workers; `App: Clone` is a cheap `Arc` clone, not a deep copy.
- All `Services::with_*` setters consume `self` (`service.rs:384-535`): config is immutable after
  build, so there is no interior-mutability race on flags, secrets or the allow-list.
- Safe defaults: flags `false`, secrets/ transports `None`, `stdio_allowed` empty, and
  `automation_wait_timeout_seconds.max(1)` at `service.rs:453` — good fail-closed instincts.
- Atomicity is already pushed down where one adapter owns all the tables: `NewAccount`,
  `NewBot`+thread, and nonce-idempotent `messages.insert` / `runs.create` (CONFIRMED at
  `ports.rs:814-826`, `ports.rs:884-893`). Keep this; do not re-litigate it.
- `Failure::from(AppError)` is an exhaustive match (`service.rs:605-615`): adding an `AppError`
  variant breaks the build, forcing the mapping to be revisited.

## Failure modes seen → concrete hardening proposals

### F1. No transaction boundary spans ports — partial writes are the default
`Stores` is a flat bag of independent handles (`service.rs:23-116`). There is no `UnitOfWork`,
`Transaction`, outbox or reconciliation port anywhere in the root. The atomicity rule only covers
*single* port calls (`ports.rs:12-13`); cross-store sequences are composed by hand in use cases.
`send_message` inserts the message (`usecases.rs:4316-4338`), then creates/steers runs in a second
phase (`usecases.rs:4366`, `4402-4453`). Any failure in phase two (`find_active_for_thread`,
`bots.find`, `model_choice_for`, `runs.create` → `Unavailable`) returns `Err` while the message is
already durable: the client sees failure, the thread holds a message with zero runs. In a group
fan-out the loop is one run per iteration, so a failure on target 2 leaves target 1 running and
target 2 silent — a partially effective send plus an error. Nonce dedup makes a *retry* converge
(message insert and run create return the existing rows), but nothing guarantees the client retries,
and the intermediate state is user-visible. The same shape applies to account→session and
charge→entitlement sequences.
P1. Add a durable intent/outbox port to `Stores` (e.g. `pending_operations`) plus a reconciliation
    entry point owned by `App`: phase one writes the intent atomically with its first row; a
    worker (or the next request with the same nonce) completes or compensates phase two. For
    v0.4.0 prefer the narrow version — keep per-port atomicity, add an intent store and a
    "reconcile pending sends" use case — over a general transaction abstraction.
    Risk: cross-crate (new port + infra adapter + worker); must not change `MessageStore`/`RunStore`
    signatures; verify with a test that injects a phase-two failure and asserts no orphan state.

### F2. `App::new` is infallible; incoherent configurations boot successfully
`App::new(stores, services)` just stores both (`service.rs:546-552`). The setters allow
`with_billing(true, None, None)` (`515-527`), `with_messaging(true)` with no Slack/transport
(`478-483`, `485-490`), and `with_vapid(None)` while notifications are wired (`505-513`). The
runtime path degrades gracefully ("refuse with a clear error", "silent no-ops"), so the fault is
masked: an operator sees an enabled surface that cannot work, and only discovers it at first use.
P2. Add `Services::validate(&self) -> Result<(), ConfigError>` checking cross-field invariants
    (enabled flag ⇒ required provider/secret/transport present; wait timeout ≥ 1; allow-list
    non-empty only if stdio is in use), mark it `#[must_use]`, and call it in the binary before
    `App::new`. Keep the runtime refusal as defence in depth.
    Risk: turns a "boots degraded" deployment into a boot failure — intended fail-fast, but it is
    observable; gate behind a config-matrix test and note it in `docs/PARITY.md` if considered
    external. Do not move `session_ttl_seconds` here (it is a per-call argument today).

### F3. Outbound transports are wired with no timeout or retry policy
`Services` holds `mcp_transport`, `plugin_fetcher`, `oauth_client`, `messaging_transport`,
`model_transport`, `computer`, `connector_runner` as bare traits (`service.rs:241-266`). The only
timeout in the root is `automation_wait_timeout_seconds` (`282`). A hung dependency can therefore
pin a request indefinitely unless each adapter happens to set its own deadline — which the root
cannot verify or audit. This is also the natural place to honour "a timeout on every outbound call".
P3. Introduce an `OutboundPolicy { connect_timeout, request_timeout, max_retries }` in `Services`,
    require it at construction, and wrap each outbound adapter at the composition site so the
    deadline is enforced uniformly; keep streaming/model calls on a long, explicit budget rather
    than the default.
    Risk: a blanket short timeout cuts legitimate long runs — make it per-adapter and test the
    streaming path; this touches infra wiring, so land it after P2.

### F4. `Failure` is lossy and may leak values into logs/audit
`Failure::from` stores `error.to_string()` verbatim in `detail` (`service.rs:603-620`) and collapses
every cause to a static `reason`. `AppError::Rule(DomainError)` (`error.rs:12-13`) is the risk: the
doc claims the detail "never contains personal data", but whether every `DomainError` Display string
is value-free is GUESS and unverified here. The flattening also discards `AppError::is_retryable()`
/ `is_client_fault()` (`error.rs:43-66`), so an operator reading an audit row cannot tell a transient
dependency failure from a permanent rule violation.
P4. Add a `class: permanent | transient | client` field derived from those two methods, and add a
    test that feeds a rule error carrying an email/name into `Failure` and asserts the detail does
    not contain that value; if it does, redact at the source rather than in the logger.
    Risk: audit-row shape changes (internal surface — confirm before treating as DEVIATION).

### F5. Send/readiness types carry no enforced send+sync or consistency bound
The ports are individually `Send + Sync` (CONFIRMED), but nothing at the struct level states that
`App`, `Stores` and `Services` must remain so; a future non-thread-safe field would fail far away at
an Axum handler. Likewise `SendOutcome { message, runs }` (`563-570`) asserts "every run created or
steered by the message" by doc only — after F1's partial failure it simply is not returned, so the
client cannot distinguish "no run yet" from "will never have one".
P5. Add a test-only compile-time assertion (`fn assert_send_sync<T: Send + Sync + 'static>() {}`)
    instantiated for all three types — zero production risk. Revisit `SendOutcome` only after P1
    makes the message/run pair a real invariant.
    Risk: none; this is a guard, not a behaviour change.

### F6. No readiness/drain primitive in the root
`Services` exposes `metrics` and `metrics_enabled` (`283-287`) but no readiness state or shutdown
handle; graceful shutdown that drains in-flight requests is an AGENTS rule the composition root
cannot currently express.
P6. Add a small `Runtime`/`Shutdown` handle alongside `Services` (readiness flag + in-flight
    counter) and wire it into the binary's `/readyz` and signal handler.
    Risk: cross-crate; schedule as a v0.4.0 follow-up, not a blocker for F1.

## Risks and trade-offs
- R1. F1 is the only finding that fixes real data inconsistency; it is also the largest. Do not
  ship a "transaction" that silently downgrades to best-effort — name it an outbox/reconciler.
- R2. P4's `detail` change and P2's fail-fast both alter observable behaviour (audit rows, boot
  success); record each in `docs/PARITY.md` and keep runtime refusals after validation.
- R3. P3 timeouts and P6 shutdown are infra-wide; land them in separate commits with their own
  tests so a regression is attributable.
- R4. Keep every existing public port signature and the `Failure` reason strings stable where
  clients depend on them; add fields, do not rename.

## Explicit non-goals
No redesign of port traits, no change to client error envelopes, no per-request config mutation,
no retry/backoff policy in this module (belongs to use cases/infra), no new feature surface.

## Suggested v0.4.0 rollout
Wave A (cheap, test-only): P5 assertion, P4 detail/retryability test. Wave B: P2 config validation
with a matrix test. Wave C: P3 outbound policy (per-adapter, streaming exempt). Wave D: P1 intent
store + reconciler, proven by an injected-failure integration test and a nonce-retry test; P6 last.
Acceptance: no cross-store sequence can leave a durable first write with an error response and no
reconciliation path; every outbound adapter has an auditable deadline; an incoherent config fails
at boot; `Failure` carries a retryability class and no personal data.

_Scope: line refs against `before-m5`; no code/tests/builds ran; PII-freedom of `DomainError` Display marked GUESS._
