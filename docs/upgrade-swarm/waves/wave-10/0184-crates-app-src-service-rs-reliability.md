# 0184 — crates/app/src/service.rs — reliability (v0.4.0)

Worker #184 · `crates/app/src/service.rs` · reliability · READ-ONLY review of `before-m5`.
The module is the composition root: `Stores` (50+ `Arc<dyn ...Store>`), `Services` (flags, secrets,
transports, timeouts), `App`, plus `Actor`, `SendOutcome`, `BotWithThread`, `Failure`. It does no IO,
so its reliability surface is the promises it makes to use cases and the invariants it enforces at
construction. Line refs against `before-m5`; file facts CONFIRMED by reading, test/build status GUESS.
No code ran.

Anchors: `ports.rs:9-13` ("sequences allocated by the store", "a multi-table write is one port
call"); `usecases.rs:11-14` (create_bot + thread in one transaction; send_message replays a nonce);
AGENTS: "transactions for multi-step writes", "a timeout on every outbound call", config "validated
at startup (fail fast)", "no secrets or personal data in logs".

## What is already right (keep, don't re-propose)
- Every port trait is `Send + Sync` (CONFIRMED: 40+ declarations in `ports.rs`); `App: Clone` is a
  cheap `Arc` clone, safe to share across Tokio workers.
- All `Services::with_*` setters consume `self` (`service.rs:384-535`): config is immutable after
  build, so no interior-mutability race on flags, secrets or the allow-list.
- Safe defaults: flags `false`, secrets/transports `None`, empty stdio allow-list, and
  `automation_wait_timeout_seconds.max(1)` at `service.rs:453`.
- Atomicity is already pushed down where one adapter owns the tables: `NewAccount`, `NewBot`+thread,
  nonce-idempotent `messages.insert` / `runs.create` (CONFIRMED at `ports.rs:814-826`, `884-893`).
- `Failure::from(AppError)` is an exhaustive match (`service.rs:605-615`): a new `AppError` variant
  breaks the build and forces the mapping to be revisited.

## Failure modes seen → concrete hardening proposals

### F1. No transaction boundary spans ports — partial writes are the default
`Stores` is a flat bag of independent handles (`service.rs:23-116`); there is no `UnitOfWork`,
transaction, outbox or reconciliation port. The atomicity rule only covers *single* port calls
(`ports.rs:12-13`); cross-store sequences are composed by hand in use cases. `send_message` inserts
the message (`usecases.rs:4316-4338`), then creates/steers runs (`usecases.rs:4366`, `4402-4453`).
Any failure in phase two (`find_active_for_thread`, `bots.find`, `model_choice_for`,
`runs.create` → `Unavailable`) returns `Err` while the message is durable: the client sees failure,
the thread holds a message with zero runs. In a group fan-out one run is created per iteration, so a
failure on target 2 leaves target 1 running and target 2 silent — a partially effective send plus an
error. Nonce dedup makes a *retry* converge, but nothing guarantees the client retries and the
intermediate state is user-visible. Same shape for account→session and charge→entitlement.
P1. Add a durable intent/outbox port to `Stores` (e.g. `pending_operations`) plus a reconciliation
    entry point owned by `App`: phase one writes the intent atomically with its first row; a worker
    (or the next request with the same nonce) completes or compensates phase two. For v0.4.0 prefer
    the narrow version — keep per-port atomicity, add an intent store and a "reconcile pending
    sends" use case — over a general transaction abstraction.
    Risk: cross-crate (new port + infra adapter + worker); do not change `MessageStore`/`RunStore`
    signatures; prove with a test that injects a phase-two failure and asserts no orphan state.

### F2. `App::new` is infallible; incoherent configurations boot successfully
`App::new` just stores both arguments (`service.rs:546-552`). Setters allow `with_billing(true,
None, None)` (`515-527`), `with_messaging(true)` with no Slack/transport (`478-490`), and
`with_vapid(None)` while notifications are wired (`505-513`). Runtime degradation ("refuse with a
clear error", "silent no-ops") masks the fault: an operator sees an enabled surface that cannot work.
P2. Add `Services::validate(&self) -> Result<(), ConfigError>` checking cross-field invariants
    (enabled flag ⇒ required provider/secret/transport present; wait timeout ≥ 1), mark it
    `#[must_use]`, and call it in the binary before `App::new`; keep runtime refusal as defence in
    depth. Risk: turns "boots degraded" into a boot failure — intended fail-fast but observable;
    gate behind a config-matrix test, note it in `docs/PARITY.md` if treated as external. Do not
    move `session_ttl_seconds` here (it is a per-call argument today).

### F3. Outbound transports are wired with no timeout or retry policy
`mcp_transport`, `plugin_fetcher`, `oauth_client`, `messaging_transport`, `model_transport`,
`computer`, `connector_runner` are bare traits (`service.rs:241-266`); the root's only timeout is
`automation_wait_timeout_seconds` (`282`). A hung dependency can pin a request indefinitely unless
an adapter happens to set its own deadline — which the root cannot audit.
P3. Add an `OutboundPolicy { connect_timeout, request_timeout, max_retries }` to `Services`, require
    it at construction, and wrap each outbound adapter at the composition site; keep streaming/model
    calls on a long explicit budget. Risk: a blanket short timeout cuts legitimate long runs — make
    it per-adapter and test the streaming path; touches infra wiring, land after P2.

### F4. `Failure` is lossy and may leak values into logs/audit
`Failure::from` stores `error.to_string()` verbatim in `detail` (`service.rs:603-620`) and flattens
every cause to a static `reason`. `AppError::Rule(DomainError)` (`error.rs:12-13`) is the risk: the
doc claims the detail "never contains personal data", but whether every `DomainError` Display string
is value-free is GUESS and unverified here. Flattening also discards `is_retryable()` /
`is_client_fault()` (`error.rs:43-66`), so an operator cannot tell a transient failure from a
permanent rule violation.
P4. Add a `class: permanent | transient | client` field derived from those methods, plus a test
    feeding a rule error carrying an email/name into `Failure` and asserting the detail excludes it;
    if it leaks, redact at the source. Risk: audit-row shape changes (internal surface — confirm
    before treating as DEVIATION).

### F5. No enforced send+sync or consistency bound on the root types
Ports are individually `Send + Sync` (CONFIRMED), but nothing states `App`, `Stores` and `Services`
must remain so; a future non-thread-safe field fails far away at an Axum handler. `SendOutcome`
(`563-570`) asserts "every run created or steered by the message" by doc only — after F1's partial
failure it is not returned, so the client cannot tell "no run yet" from "will never have one".
P5. Add a test-only compile-time assertion (`fn assert_send_sync<T: Send + Sync + 'static>() {}`)
    instantiated for all three types — zero production risk. Revisit `SendOutcome` only after P1.

### F6. No readiness/drain primitive in the root
`Services` exposes `metrics`/`metrics_enabled` (`283-287`) but no readiness state or shutdown
handle; the AGENTS rule "graceful shutdown that drains in-flight requests" is not expressible here.
P6. Add a small `Runtime`/`Shutdown` handle (readiness flag + in-flight counter) beside `Services`
    and wire it into `/readyz` and the signal handler. Risk: cross-crate; v0.4.0 follow-up, not a
    blocker for F1.

## Risks and trade-offs
- R1. F1 is the only finding fixing real inconsistency and the largest. Never ship a "transaction"
  that silently degrades to best-effort — call it an outbox/reconciler.
- R2. P4's `detail` and P2's fail-fast alter observable behaviour (audit rows, boot success); record
  in `docs/PARITY.md` and keep runtime refusals after validation.
- R3. P3 timeouts and P6 shutdown are infra-wide: separate commits with their own tests.
- R4. Keep public port signatures and existing `Failure` reason strings stable; add fields, don't rename.

## Explicit non-goals
No port-trait redesign, no client error-envelope change, no per-request config mutation, no
retry/backoff policy here (use cases/infra own it), no new feature surface.

## Suggested v0.4.0 rollout
Wave A (test-only): P5, P4. Wave B: P2 config validation + matrix test. Wave C: P3 outbound policy
(per-adapter, streaming exempt). Wave D: P1 intent store + reconciler, proven by an injected-failure
integration test and a nonce-retry test; P6 last.
Acceptance: no cross-store sequence leaves a durable first write with an error response and no
reconciliation path; every outbound adapter has an auditable deadline; incoherent config fails at
boot; `Failure` carries a retryability class and no personal data.

_Scope: line refs against `before-m5`; no code/tests/builds ran; `DomainError` Display PII-freedom GUESS._
