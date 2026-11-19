# 0180 — `crates/app/src/ports.rs` — reliability proposal (v0.4.0)

Worker #180 · module `crates/app/src/ports.rs` · lens: reliability · READ-ONLY review of
`rebuild-workspace/.worktrees/before-m5` at v0.3.0-era source. Proposals only; no code changed here.
Claims: **CONFIRMED** = read in this tree this session; **GUESS** = inference needing a test.
Scope: this file (4,601 lines, ~60 traits) plus its two implementation families — the hand-written
doubles in `crates/testkit/src/{memory,scripts}.rs` and the Postgres/SQLx adapters in
`crates/infra/src/stores/*.rs` (CONFIRMED by grep). Spec anchors: AGENTS.md "ports and adapters",
"typed errors", "cancellation safety considered", "idempotency keys", "contract test"; REQ-002,
REQ-004, REQ-005, REQ-012, REQ-017, REQ-020.

## What is already right (keep; do not re-propose)

- Every port is product-owned and names `AppError`, never an adapter (CONFIRMED). `TenantContext`
  is passed explicitly on tenant-scoped reads, so isolation is visible at the boundary.
- Atomicity is *documented* where it matters: REQ-004 `create_account` / `create_with_thread`,
  REQ-005 store-allocated `seq`, nonce-returning `insert`/`create`, `consume`, `accept`, `retry`,
  `claim_next_queued`, `take_oauth_session` (CONFIRMED, doc comments).
- Clocks are injected (`Clock::now() -> String`) and timestamps are caller-supplied RFC 3339, so
  goldens are reproducible (CONFIRMED). Fenced writes exist (`set_status`, `set_steering_consumed`,
  `fence: Option<i64>`) (CONFIRMED).
- Secrets cross as `secrecy::SecretString` on the newer ports (CONFIRMED).

## Failure modes

### F1. Trait drift — the contract is prose, not types (high)

~60 traits encode their most important promises only in `/// # Errors` prose and header comments.
Nothing compiles when an adapter violates them (CONFIRMED). Concrete drift already visible in this
file: the verb *delete* returns `Result<(), _>` in `SessionStore::delete` but `Result<bool, _>` in
`SecretStore`/`SkillStore`/`McpServerStore`/`ArtifactStore::remove_family` — the same concept has two
meanings (CONFIRMED). Pagination is ad hoc: `before: Option<i64>` (`MessageStore::page`) vs
`Option<&str>` (`RunStore::list`), `limit: i64` vs `usize` vs `Option<usize>`, `ActivityPage` and
`MessagePage` unrelated (CONFIRMED). Idempotency is re-invented per port: `StoredMessage.created`,
"return the existing `Run`", separate `find_by_nonce` replay checks, `EffectStore::record` Conflict —
no shared vocabulary, so a new port gets a new, subtly different rule (CONFIRMED). Adding a method to
a `#[async_trait]` trait has no default body, so every fake and adapter must be touched — but if a
default is ever added, the scripted doubles can silently keep passing without modelling it (GUESS).

### F2. Async cancellation safety — atomicity cannot be enforced by the trait (high)

All store methods are `#[async_trait] async fn(&self, ...)`. Cancelling the returned future (timeout,
`select!`, worker shutdown) is free at every `.await`. The trait *claims* "one statement" /
"one transaction" for the nonce, consume, accept and fence paths, but nothing prevents an adapter
from opening a transaction and dropping the future between two round-trips — leaving a half-written
`NewAccount`, a consumed token with no session, or a leased run with no worker (CONFIRMED prose,
GUESS that any current adapter is non-atomic). The `fence: Option<i64>` parameters conflate
"unconditional write" with "fence value was None", so a dropped-and-retried call that passes `None`
can overwrite a live worker's status (CONFIRMED shape, GUESS on a real drift). `EventPublisher`
returns a raw `tokio::sync::broadcast::Receiver<Event>` from a *sync* method; `Lagged` handling is
the caller's job and is undocumented here, so a cancelled/reconnected subscriber can silently miss
events even though the docs promise replay from last sequence (CONFIRMED structure, GUESS on impact).

### F3. Mock divergence — doubles are trusted but never held to the contract (high)

`crates/testkit` hand-writes `Memory*Store` and `Scripted*` doubles for these traits; `crates/infra`
hand-writes the Postgres adapters (CONFIRMED). Two separate implementations of ~60 contracts drift by
construction: an in-memory store that does not allocate `seq` atomically, does not enforce the
`(thread, run)` pending-card uniqueness, or does not return `Conflict` on a second `accept` will make
a REQ test green while the real store rejects the same call (GUESS — needs a differential run).
Scripted clocks/ids return canned values, so boundary cases (`expires_at > now`, lease expiry, fence
mismatch) are exercised with convenient timestamps that production never produces (CONFIRMED design).
No shared conformance suite that runs one behavioural assertion set against both families was found
(GUESS — limited keyword search of `crates/testkit/src`).

## Concrete hardening (v0.4.0)

1. **P1 — name the shared semantics.** Introduce a small vocabulary in `ports.rs`:
   `Idempotent<T> { value: T, created: bool }`, `Removed { removed: bool }`, and one
   `Page<T> { items, next_cursor }`. Migrate `StoredMessage`, the `bool` deletes and the two page
   types to them. Purely additive in `app`; risk = churn across use cases/testkit (R1).
2. **P2 — unify the boundary shape.** Make every paged port take `&Cursor` and `limit: u32`; every
   "delete missing" port return `Removed`. A compile error then marks the next drift (risk R1).
3. **P3 — a port-conformance harness.** Define, next to the traits, a generic async test function per
   invariant ("nonce returns the original", "second accept is a no-op", "fence mismatch is refused",
   "tenant B cannot read A's row", "seq is unique under 2 concurrent inserts"). Instantiate it once in
   `testkit` (memory) and once in `infra` (SQLx). Risk = more fixtures; derive each case from the
   doc comment, not intuition, or the test bakes in the bug (R2).
4. **P4 — cancellation tests for the atomic ports.** Use a manually-polled future (or
   `futures::future::poll_fn`) to drop each atomic port at every await point, then assert the store is
   unchanged (no half-created account, token still consumable, run still queued) (risk R3).
5. **P5 — split the fence type.** Replace `fence: Option<i64>` with an enum
   `Fence { Unconditional, Expected(i64) }` (or `Option<NonZeroI64>`), so "no fence" is an explicit
   choice. Update `set_status` and `set_steering_consumed` call sites (risk R1).
6. **P6 — wrap the event receiver.** Return a `Subscription` newtype with `recv()` that maps
   `RecvError::Lagged` to an explicit "replay needed" signal, instead of a raw broadcast receiver.
   Documents reconnect behaviour in the type (risk R4).
7. **P7 — keep doubles honest at build time.** Add a compile-time `assert_impl_all!` block listing
   every port for both `TestApp`'s store set and the production `Service`, so a new trait cannot be
   added without an implementation (risk R5).
8. **P8 — contract lint in CI.** A small script/gate that fails when a `#[async_trait] pub trait` is
   added to `ports.rs` without a matching entry in the conformance harness and both assembly points,
   and that flags two ports using different names for the same noun (`deleted`/`removed`,
   `before: i64`/`&str`) (risk R5).

## Risks / trade-offs

- R1. P1/P2/P5 change public signatures inside `app`; no wire format or golden changes, but every
  use case, fake and adapter must be updated together. Do it as one atomic PR with `cargo check`.
- R2. P3 conformance cases assert what the *docs* promise; a wrong reading turns a real deviation
  into a permanently green test. Cite the REQ id in each case's name (AGENTS.md traceability).
- R3. P4 adds focal-point drop tests that can be flaky if an adapter holds an OS resource; start with
  the in-memory store, then extend to SQLx with a test transaction that is always rolled back.
- R4. P6 changes the `EventPublisher` return type; an externally visible client behaviour (lag = replay)
  is a DEVIATION candidate — record in `docs/PARITY.md` if any client relied on the raw receiver.
- R5. P7/P8 add gates; keep them at least as strict as the current `clippy -D warnings` gate and
  document the new rule in AGENTS.md rather than silently changing CI.

## Explicit non-goals

- Do not rename public RPC fields, endpoints or JSON shapes; these ports are internal.
- Do not add a blocking/`std::sync` port; keep the current sync/async split (`Clock`, `IdGenerator`,
  `TokenGenerator`, `EventPublisher` sync) unless an ADR justifies the change.
- Do not weaken `TenantContext` discipline or allow secrets into `Debug`/logs while hardening.

## Suggested rollout

Wave A (internal, no wire change): P1, P2, P5, P7 with `cargo fmt --check`, clippy `-D warnings`,
and the existing REQ-002/004/005/012/017 tests after each step. Wave B: P3, P4 (memory first, then
SQLx), P6 with a `docs/PARITY.md` check. Wave C: P8 gate. Line refs are against
`.worktrees/before-m5`; nothing was executed in this pass.
