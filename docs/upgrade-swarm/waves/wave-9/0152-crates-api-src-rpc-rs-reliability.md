# 0152 — `crates/api/src/rpc.rs` reliability proposal (v0.4.0)

Module: `crates/api/src/rpc.rs`
Lens: reliability
Spec anchor: ADR-003 (one route, one dispatcher), spec/03-api-contract.md §3.1 (stable error
envelope), REQ-038/REQ-048/REQ-051 (optional surfaces), REQ-041/REQ-043 (auth and tenant
isolation), REQ-054 (request ids, no secrets in logs), D-07 (artifact body cap).

Status of claims: "Current behavior" is CONFIRMED by reading this session — `rpc.rs`
(`router` 266-277, `rpc_inner` 311-730, `procedures` 135-264, `parse` 1846-1851), `error.rs`
(`IntoResponse` 216-230, `explained` 93-99, `unavailable` 164-170), `auth.rs:66-88`,
`router.rs:135-154`, `correlation.rs:1-11`, and the single test module at `rpc.rs:4522`. Proposals
and risks are GUESS (design inference, not implemented or tested).

## Current behavior (CONFIRMED)

- `router()` exposes two routes: a dedicated `/rpc/artifacts/create` with a larger body cap
  (D-07) and the catch-all `/rpc/{namespace}/{procedure}`. Both funnel into `rpc_inner`;
  `router.rs` adds a global `DefaultBodyLimit`, `TimeoutLayer`, security headers, and
  `correlation::log_http`.
- `rpc_inner` authenticates **first** (318-321), then applies three feature gates (boards/tickets,
  messaging, billing → 404, 323-337), then runs one ~385-arm `match` on `(namespace, procedure)`
  (339-724) whose default arm is `_ => Err(ApiError::not_found())` (723). A final `match` returns
  `Ok(response)` unchanged or converts `Err(ApiError)` into the envelope (726-729).
- `procedures()` (135-264) is a public discovery list. It is not used by the router, the match, or
  authorization; it is documentation only.
- Auth is a single all-or-nothing gate. Per-procedure authorization is decentralized: every
  handler passes `auth.actor.context` to its use case, where membership (→ 404) and role checks
  live. `invitations.accept` (3738-3748) also requires a signed-in caller whose address the token
  binds.
- `parse` (1846-1851) uses `serde_json::from_slice` with `deny_unknown_fields` + camelCase request
  structs; failures are `tracing::debug!` plus a generic 400. Errors serialize as exactly
  `{"code","error"}` with `Cache-Control: no-store` (error.rs:216-230); `x-request-id` is added by
  correlation middleware (router.rs:152-154), not by the envelope.

## Failure modes

1. **Unknown method.** An unknown namespace/procedure is a 404 only *after* authentication,
   full body buffering, and feature-gate checks. It is deliberately indistinguishable from a
   disabled surface (good anti-enumeration) but produces no distinct log/metric, so "client typo"
   and "endpoint probe" are unobservable, and junk paths still force session/actor DB lookups —
   a cheap amplification vector.
2. **Auth per method.** One global gate means there is no declarative policy per procedure:
   an admin/arm addition that forgets its in-use-case role check is reachable with no dispatcher
   test to catch it. Because `procedures()` is not authoritative, no invariant ties "listed",
   "dispatched", and "authorized" together. The sign-in requirement of `invitations.accept` is
   a per-method assumption that a future token-only flow could silently break.
3. **Error envelope.** Two codes share a status (`Internal`→500 vs `unavailable`'s 503 with code
   `Internal`); `explained` (error.rs:93-99) forwards bounded third-party text. Arms returning a
   raw `Response` (`threads.subscribe` via realtime, `export.download`) bypass `ApiError`'s
   conversion, so their failures may not share the envelope. Parse errors reveal no field path,
   carry no request id in the body, and log at debug only; correlation logs the route template
   `/rpc/{namespace}/{procedure}`, collapsing every procedure into one log label.
4. **Wire drift.** The `procedures()` list, the match arms, `wire.rs` DTOs, and
   `spec/openapi.yaml` are four independent sources with no dispatcher-level consistency test
   (only one test exists, at 4522). A listed procedure without an arm 404s at runtime; an arm
   without a list entry is reachable but undiscoverable. Strict `deny_unknown_fields` is intentional
   parity but means an additive field from a newer client gets a 400, while `#[serde(default)]`
   optional fields make a removed field silently become `None` — both are compatibility cliffs.

## Concrete hardening for v0.4.0 (GUESS)

1. Derive `procedures()` from the dispatcher (a small macro or const table that also generates
   the match arms) and add a contract test: every listed procedure has an arm, every arm is listed,
   and both equal the operations in `spec/openapi.yaml` (reuse the existing contract harness).
   *Risk:* refactoring a ~385-arm match can change behavior silently; keep arms byte-identical and
   verify with golden fixtures. Any change to the discovery API is a DEVIATION (approval gate).
2. Add a declarative per-procedure policy table (required role / enabled flags) with a test that
   each `admin.*` entry declares a role check. Keep enforcement in the use cases as defence in
   depth; start as assertions, not runtime gating. *Risk:* two authorization sources can drift the
   other way — make the table the single tested source and have handlers reference it, or skip
   runtime enforcement entirely for v0.4.0.
3. Reject unknown `(namespace, procedure)` early via a const lookup, before body buffering and
   auth, while preserving the exact 404 + `no-store` response and disabled-surface
   indistinguishability. Emit a metric/log for "unknown procedure" with the request id. *Risk:* the
   special artifact route and the raw-`Response` arms must stay on the same path; verify no status
   or body change before enabling.
4. Unify error handling: route raw-`Response` arms through a helper that still converts their
   `ApiError`s to the envelope, and attach `namespace`/`procedure` plus the correlation request id
   as span fields in `parse` and `rpc_inner` (keep the body at exactly two fields). *Risk:* touching
   streaming/SSE or download paths can alter semantics — do logging/metrics first, response-path
   changes second, behind a DEVIATION review.
5. Add dispatcher-level tests (none exist today): unknown procedure → 404, disabled surface → 404,
   unauthenticated → 401 for every listed procedure, malformed body → 400, envelope has two fields
   and `no-store`. Pin that a newer-client extra field is refused and document it as a known
   compatibility boundary. *Risk:* a table-driven auth test needs a seeded app/DB; prefer
   router-level `oneshot` with minimal state and keep test names carrying REQ ids (trace_check).

## Constraints

- Preserve the stable two-field envelope, 404 indistinguishability (unknown = disabled = foreign
  tenant), and strict deserialization. Any observable change goes to `docs/PARITY.md` as a
  DEVIATION and waits for approval.
- No secret, token, or PII in logs or the error body; request ids stay in `x-request-id` and
  structured logs only (REQ-054).
- Keep the artifact body cap (D-07) and the router's global body limit, timeout, and security
  headers intact; the dispatcher must not add blocking work to the async path.
