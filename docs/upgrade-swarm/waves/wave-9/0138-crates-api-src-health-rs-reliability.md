# 0138 — `crates/api/src/health.rs` reliability proposal (v0.4.0)

Worker #138 · module `crates/api/src/health.rs` · lens: reliability · READ-ONLY review of
`before-m5` (v0.3.0-rc5). Proposals only; no product code changed. Anchors: REQ-053/054,
`spec/05-known-defects.md`, `docs/STATE.md:252-260`, `docs/upgrade-swarm/waves/wave-1/0005` and
`wave-8/0120`. "Current behavior" is CONFIRMED by reading the named files; proposals are GUESS.

## Current behavior (CONFIRMED)

- `liveness()` (`health.rs:99-101`) returns `{ok:true}` unconditionally; never touches a dependency,
  by design (`app/src/health.rs:3-8`). Golden `spec/golden/health.json` pins that exact body.
- `readiness()` (114-128) calls `state.readiness().report(readiness_timeout())`, returns the
  capability projection on success, a generic `ApiError::unavailable()` (503) on failure, and logs
  only dependency *names* (REQ-054). `ReadinessBody::from_capabilities` hardcodes `ok: true` (63).
- `internal_health()` (164-171) returns `{ok:true, last_completed_at:null}` **unconditionally**;
  the comment admits the M1 heartbeat never landed. The supervisor cannot get a red signal here.
- `Readiness::report` (`app/src/health.rs:56-76`) runs checks **sequentially**, each with its own
  `timeout` — no concurrency, no coalescing. The original coalesced concurrent polls in one
  `inFlight` promise (`re-workspace/.../health.ts:14-27`); the Rust port dropped that.
- The DB probe is `SELECT 1` under a nested timeout (`infra/src/db.rs:125-140`) inside the outer
  `timeout` in `report`. `config.rs:114-121` permits `readiness_timeout == request_timeout`.
- Router mounts probes **flat** (`router.rs:28-35`); `deny_list` (223-235) exact-matches
  `/health|/healthz|/ready|/readyz|/metrics`, so nested probe paths are NOT denied before the static
  web fallback (270-312). `boma probe` checks only the status line (`bin/src/probe.rs:42-46`).

## Failure modes

**F1 — False healthy.**
- `internal_health` is a lie shipped as a contract: a dead or hung worker is reported green forever
  with a null heartbeat. This is the worst defect in the file; v0.4.0 cannot leave it as a stub.
- Liveness green means only "TCP accepted and handler scheduled". A wedged Tokio runtime, a blocked
  sync call on the async worker, or an exhausted pool still passes. Do **not** "fix" this by probing
  the DB in `/health` — that kills healthy processes on a dependency blip, which the module doc
  explicitly forbids; the deep signal belongs in `/ready`.
- `from_capabilities` setting `ok:true` is a latent lie: safe only because the sole caller checked
  `is_ready()`. Any future projection from a failed report silently reports success.
- With `BOMA_WEB_ROOT`, a miscomposed probe path that is not on the exact deny-list can be answered
  by the static export. If a file exists at `<root>/health/health` (`resolve_static`, 173-198), it
  returns **200 HTML**; `boma probe` would call that healthy. A probe can pass without touching any
  dependency or the health handler at all.

**F2 — Dependency probe races.**
- Sequential checks cost **N × timeout**, not one. With two checks and
  `readiness_timeout == request_timeout`, the global `TimeoutLayer` (`router.rs:136-139`) can fire
  before the handler returns → `/ready` answers 408/reset instead of a truthful 503; orchestrators
  read that as flapping. This gets worse every time a dependency is added.
- No single-flight: compose healthcheck, CI probe and desktop boot probe each open their own DB
  query on a DB already struggling — exactly the "probe 56" churn (0120 F1). The original prevented
  it; the rebuild regressed.
- Two stacked timeouts can disagree: the outer `report` deadline may cancel a query whose inner
  `db.rs` deadline is still pending, flipping the logged reason between "timed out" and "rejected".
- Cancellation is not the main risk (dropping the future releases the pool permit), but a fix that
  *caches completed results* would create a fresh false-healthy window: a dependency that dies just
  after a green pass stays green for the cache TTL. Coalesce in-flight only; never cache completions.

**F3 — `/health/health` double-prefix class (Windows).**
- CONFIRMED root cause (`STATE.md:252-260`): the desktop passed a URL already ending in `/health` to
  a helper that appends `/health` again → `/health/health` → 404 forever → 90 s wait → `stopStackAt`
  killed Postgres. It surfaced on Windows only because that is where migrations first applied.
  Fixed at the caller (`0005`:15), but the *class* is unguarded server-side: the path is duplicated
  across utoipa attrs, the route table, `deny_list`, the CLI probe default, compose/Dockerfile
  healthchecks, CI curl and the desktop suffix helper — no shared constant.
- The handlers do not care about a prefix, so no handler-side fix exists. The server's contribution
  is that a miscomposed path falls into the static fallback instead of a hard JSON 404, and that no
  test enumerates exact-vs-nested probe paths.

## Concrete hardening for v0.4.0 (GUESS)

1. **Make `internal_health` truthful.** Carry heartbeat state (`last_heartbeat_at`,
   `last_completed_at`) in `AppState`; derive `ok` from a configured window and return 503 when
   stale; finally populate the contract field. Add a boot grace / "no job yet" sentinel so a fresh
   worker is not restart-looped. *Risk:* 200→503 is externally visible — DEVIATION entry in
   `docs/PARITY.md`; grace semantics need owner decision. Tests `req_053_*`, written red first.
2. **One deadline for readiness.** Run checks concurrently under a single
   `tokio::time::timeout(readiness_timeout, join_all(...))`; remove the nested `db.rs` timeout (keep
   one owner). *Risk:* parallel probes can stampede a recovering DB — ship with #3; the logged reason
   may change and confuse dashboards.
3. **Restore single-flight.** Share one in-flight report future (`Arc<Mutex<Option<...>>>`, take +
   oneshot) so concurrent polls join one pass; never cache a completed result; cap how long a late
   caller waits. *Risk:* do not hold a std lock across `.await` (AGENTS.md); a TTL cache reintroduces
   F1 false-healthy.
4. **Prefix-deny probe paths.** Make `deny_list` reject any path starting with `/health`, `/healthz`,
   `/ready`, `/readyz`, `/metrics` (and `/internal`), so a nested/miscomposed probe always gets the
   stable JSON 404 and never export HTML. *Risk:* if the export legitimately ships a `/health/*`
   asset it becomes unreachable — check the export manifest; pin with a `BOMA_WEB_ROOT` test
   asserting `/health/health` → 404.
5. **One source of truth for probe paths.** Export path constants from the API crate and derive the
   client's paths from the generated OpenAPI document; add a contract test enumerating the four
   exact paths → 200 and `/health/health`, `/ready/ready` → 404. *Risk:* aliases are deliberately
   absent from the document (`openapi.rs:72-81`) — keep them out; test both document and route table.
6. **Harden `boma probe`.** For canonical probe paths require `"ok":true` in the JSON body, not just
   ` 200` in the status line; log the final path once. *Risk:* restricts using `--path` against
   non-Boma endpoints — owner decision (GUESS); at minimum add the body check for the default path.
7. **Remove the hardcoded `ok:true`.** Have `from_capabilities` consume the passed report (or return
   a `Result`) so a healthy body cannot be built from a failed pass. *Risk:* internal signature only;
   update the golden test and all call sites.

## Constraints

- Keep golden `/health` and `/ready` shapes; no secret, connection target or PII in unauthenticated
  responses (REQ-054). Keep liveness dependency-free.
- Every new test carries the REQ-053 id and is run red first; nothing is called CONFIRMED before it
  is actually executed.
- Any observable change (worker 503, richer 503 body, alias exposure) goes to `docs/PARITY.md` and
  awaits owner approval before shipping.
