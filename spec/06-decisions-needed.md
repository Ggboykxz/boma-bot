# 06 — Decisions needed

Nothing in this file is a requirement. Each entry is either a **GUESS** about the reference (inferred,
not read) or a **choice the rebuild must make** that the reference cannot answer for us. Requirements
(01) contain only CONFIRMED facts; anything that needed an assumption was pushed here instead.

Format: the question, why it blocks us, the options, and a recommendation. `Owner` is who can decide.

---

## A. Guesses about the reference (verify with a runtime pass)

### DEC-01 — Does a reconnect mid-stream replay events, and from where?

- **GUESS**: the SSE endpoint resumes from the last `seq` the client acknowledged, so no event is lost
  between reconnects.
- **Not confirmed because**: realtime was never observed (`runtime: no`).
- **Blocks**: REQ-005's "no gap after reconnect" clause is written as a requirement on the rebuild's
  own transport; if the reference replays differently, the compatibility test differs.
- **Verify**: open a thread, kill the connection mid-run, reconnect with `Last-Event-ID`, compare the
  rendered transcript to a second clean client.
- **Recommendation**: the rebuild implements resume-by-seq regardless of what the reference does; it
  is the stronger guarantee and is already required by our own contract.

### DEC-02 — What exactly does the lease fence protect against under concurrent workers?

- **GUESS**: every write that changes run status is conditional on `(runId, fence, status)` so a stale
  worker's update is a no-op rather than an error.
- **Not confirmed because**: no concurrency testing was possible.
- **Blocks**: REQ-012 and REQ-013 accept either "stale write rejected" or "stale write ignored"; the
  observable difference shows up in logs and in retry counts.
- **Verify**: two workers with the same run, expire the first lease, assert the second completes and
  the first's writes do not appear.
- **Recommendation**: ignore, and log at warn with the fence that lost.

### DEC-03 — Is `waiting_takeover` really queue-holding?

- **GUESS**: yes — `QUEUE_HOLDING_RUN_STATUSES` includes `waiting_takeover` but not `waiting_input`
  (`packages/core/src/run-state.ts:19-23`), so background triggers wait behind a takeover but are not
  blocked by an unanswered question. This was read from code and is therefore CONFIRMED for the code,
  but the *intent* is a guess.
- **Blocks**: REQ-007 and REQ-024 both depend on which paused states hold the queue.
- **Recommendation**: keep the code's behavior; it is internally consistent and documented in 02.

---

## B. Decisions the rebuild must make

### DEC-04 — Surface scope for the first release

- **Question**: web + API + worker only, or also desktop (Electron) and mobile (Expo)?
- **Blocks**: `08-ui-ux.md` declares screens for all three; `docs/PLAN.md` needs a milestone boundary.
- **Options**: (a) web only, desktop/mobile later; (b) web + desktop, mobile later; (c) all three.
- **Recommendation**: (b). Desktop is the surface that makes the computer feature usable (native
  passkeys, partitioned session, local stack); mobile adds device pairing and push, which are the
  least-defined contracts in the reference.
- **Owner**: product.

### DEC-05 — Tenancy enforcement model

- **Question**: port Postgres RLS, or enforce tenancy in the service layer?
- **Blocks**: the data-access layer in the first milestone; ADR-002; REQ-043's test strategy.
- **Options**:
  - (a) RLS + tenant-scoped connection: strongest, matches the reference, but the Rust stack needs a
    deliberate session-per-tenant pattern and every system path needs a justified escape.
  - (b) Service-layer scoping with a mandatory repository API: simpler, but one forgotten filter is a
    cross-tenant leak, and there is no second line of defence.
  - (c) Both: RLS on, plus the repository API.
- **Recommendation**: (c). Cost is one migration per table; the failure mode of (b) is unacceptable.
- **Owner**: engineering + security. **Blocks the plan**: yes.

### DEC-06 — Agent runtime strategy

- **Question**: port the TypeScript agent loop, wrap it as a service, or replace it?
- **Blocks**: REQ-016 through REQ-021 (tools, approvals, steering, streaming, delegation).
- **Options**: (a) reimplement the loop in the rebuild's language with the same tool catalogue;
  (b) keep a small TypeScript sidecar that owns the loop and speak to it over RPC; (c) replace with a
  third-party agent framework.
- **Recommendation**: (a) for the loop and tool dispatch (they are the product), (b) only if a model
  SDK is materially better in TypeScript. (c) is rejected: the approval and effect-idempotency
  semantics are the differentiator and would have to be re-implemented anyway.
- **Owner**: engineering. **Blocks the plan**: yes.

### DEC-07 — Realtime transport

- **Question**: keep SSE with snapshot + `seq`, or move to WebSocket?
- **Blocks**: REQ-005, REQ-020; the web client's data layer.
- **Options**: (a) SSE down, HTTP up (reference behaviour, trivial through proxies, resume by
  `Last-Event-ID`); (b) WebSocket both ways (bidirectional, one connection, needs sticky sessions or a
  fan-out layer); (c) SSE plus a separate WebSocket only for computer screen streams.
- **Recommendation**: (c). Chat uses SSE for its resume semantics and proxy friendliness; the screen
  stream already needs its own transport.
- **Owner**: engineering.

### DEC-08 — Computer providers for the first release

- **Question**: the reference supports 14 providers plus three in-repo ones (Docker supervisor,
  desktop host, fake). Which ship first?
- **Blocks**: REQ-026, REQ-027, REQ-031; the sandbox capability reported by `/ready`.
- **Options**: (a) Docker supervisor only; (b) Docker + one cloud provider (E2B-class); (c) Docker +
  desktop host.
- **Recommendation**: (a) plus the desktop host, because the desktop host is what lets a self-hoster
  run without a cloud account, and the provider interface is what the plan must keep stable.
- **Owner**: engineering.

### DEC-09 — Billing, credentials and voice in the first release

- **Question**: payment provider integration, model credits, BYOK credentials, provider OAuth and
  voice are all present. Which are in scope?
- **Blocks**: REQ-015, REQ-018, REQ-049, REQ-051, REQ-052.
- **Options**: (a) BYOK only (no billing, no credits, no voice); (b) BYOK + billing; (c) everything.
- **Recommendation**: (a) for v1 with the *interfaces* from (c) in place, so the plan does not have to
  restructure later. Billing without a real customer is a tax on the first release.
- **Owner**: product.

### DEC-10 — Secrets and key management

- **Question**: content encryption with a master key and pluggable KMS in the reference. What is the
  rebuild's guarantee?
- **Blocks**: REQ-046, and anything that stores a connector token.
- **Options**: (a) single master key from the environment, AES-GCM per space; (b) envelope encryption
  with a key-encryption key in a KMS/HSM and rotation; (c) delegate to an external secret manager.
- **Recommendation**: (b) with (a) as the default self-hosted profile, plus a documented rotation
  procedure and a test that a rotated key still reads old ciphertext.
- **Owner**: engineering + security. **Blocks the plan**: yes (it is a schema decision).

### DEC-11 — Locales to ship

- **Question**: the reference ships 15 locales. Which does the rebuild ship at v1?
- **Blocks**: the UI milestone, the translation pipeline.
- **Options**: (a) English only, structure ready; (b) English + French; (c) all 15.
- **Recommendation**: (b), with the catalogue pipeline and RTL support proven by one RTL locale
  (Hebrew) before adding the rest. Shipping 15 untranslated catalogues is worse than shipping two.
- **Owner**: product.

### DEC-12 — Asset and licence reuse

- **Question**: the reference is Apache-2.0, but screenshots, fonts, icons and the brand are separate.
  What may be reused?
- **Blocks**: the UI milestone and any design token extract.
- **Recommendation**: reuse nothing visual. Reimplement the interaction model from the specification
  and use the product's own design system. Keep a dependency inventory with licences.
- **Owner**: product + legal.

### DEC-13 — Does the rebuild keep the RPC procedure names?

- **Question**: 283 procedures across 52 namespaces are the reference's public contract. A rebuild can
  keep them (cheap client compatibility) or design its own API.
- **Blocks**: 03 and every golden example.
- **Options**: (a) keep the names and shapes for the documented core, free rein elsewhere;
  (b) design a fresh API.
- **Recommendation**: (a) for the procedures listed in 03; it costs nothing at design time and makes
  the golden examples meaningful.
- **Owner**: engineering.

---

## C. Open verification items (not decisions)

- Runtime observation of the whole stack is still missing. Everything marked `CONFIRMED (doc)` rests on
  reading code. A `/har` and `/ui-probe` pass on a local instance would upgrade those claims.
- The reference's OpenAPI/JSON Schema coverage was not validated against the implementation at runtime.
- Provider-side behaviour (what a bot can reach from inside a sandbox, whether the supervisor is
  reachable from a bot computer) is unverified and matters for 04.5.
