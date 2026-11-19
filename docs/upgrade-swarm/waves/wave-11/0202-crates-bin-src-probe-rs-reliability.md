# Worker 0202 — `crates/bin/src/probe.rs` reliability proposal (v0.4.0)

Lens: reliability. Scope: read-only review of `probe.rs` plus its callers (`cli.rs`, `main.rs`,
`apps/desktop/src/stack.ts`) and `crates/api/src/health.rs`; no code changed, no tests run.
Every finding below is **CONFIRMED by source inspection**; forward-looking items are marked RISK.
Target: keep `boma probe` a dependency-free, one-shot loopback check, but make failures loud,
bounded and unambiguous.

## Failure modes (as read)

**FM1 — `/health/health` double-prefix.** `boma probe --path` is a free-form string (`cli.rs:51`,
default `/ready`) that `probe.rs:30` interpolates verbatim into the request line; the probe owns no
suffix and normalises nothing. **CONFIRMED:** the desktop's `isDeploymentReachable` *does* append
`/health` itself (`stack.ts:243–253`) and its own comment (`238–241`) records that passing an
already-suffixed URL makes it probe `/health/health` forever. `STATE.md:252–260` confirms the
incident: 90 silent retries, then the app killed Postgres. The Rust probe has the same sharp edge —
any caller that appends a suffix to a path already ending in `/health` (or `/ready`) gets a 404 that
looks like "not ready yet", not "misconfigured". No regression test covers a repeated suffix.

**FM2 — timeouts are per-phase, not whole-probe.** `PROBE_TIMEOUT` (10 s, `probe.rs:16`) is applied
three separate times — connect (`25`), write (`31`) and read (`37`) — so the doc claim "upper bound
on the whole probe" is false: worst case ~30 s. `read_to_end` (`37`) also has **no byte cap** and
waits for EOF; a server that ignores `connection: close` burns the full 10 s even after sending
`200 OK`, and a hostile/buggy peer can fill memory inside the window. There is no outer deadline and
no test for slow-connect, slow-write or slow-read paths (only 200/503/closed-port, `77–105`).

**FM3 — exit codes are implicit and unclassified.** `main.rs:18` returns `anyhow::Result`, so any
probe failure exits `1` and success exits `0`; clap usage errors exit `2`. Every cause — refused
connection, timeout, non-200, bad path — collapses to the same code, so an orchestrator cannot tell
"wait longer" from "wrong configuration". **CONFIRMED:** probe success is logged with
`tracing::info!` (`probe.rs:47`) but the probe branch calls `cli::run` without `load_config`/
`telemetry::init` (`cli.rs:77` vs `93–97`), so no subscriber is installed and the message is lost.
Error text is `anyhow` Debug on stderr only (no client leak), but names address/path/status
inconsistently (`bail!` at `45` vs the connect/write/read contexts).

**FM4 — reachability verdict is fragile.** `status_line.contains(" 200")` (`probe.rs:44`) is a
substring test: it would accept `HTTP/1.1 2000 …` or a non-HTTP banner containing " 200", and it
never verifies the `HTTP/1.x` prefix or that the 3-digit code is exactly `200`. The probe ignores
the body, so it cannot see the `degraded` capability flag an orchestrator reads (`health.rs:53`,
golden `ready.json`); with a 200 that is **by design** (503 is the readiness refusal,
`health.rs:114–127`), but it should be documented. Finally, a 404 from a wrong path is reported
only as an error string — nothing marks it as "not a transient readiness miss", which is why the
desktop loop above retried it.

## Concrete hardening for v0.4.0 (with risks)

1. **Make the suffix unambiguous (FM1).** Keep the `--path` flag, but have `probe` validate the
   path once: must start with `/`, no CR/LF/whitespace, and collapse/reject a repeated terminal
   `/health` or `/ready` segment with an explicit `tracing::warn!`. Add a `req_053_*` test asserting
   the request line for `--path /health/health` is normalised or refused, never sent twice.
   RISK: changing how a supplied path is interpreted is externally visible — record a DEVIATION
   in `docs/PARITY.md` if a deployment relies on the literal value. Longer term, prefer the
   desktop's model: probe takes a *base origin* + liveness/readiness kind and owns the suffix.

2. **One outer deadline + bounded read (FM2).** Wrap connect+write+read in a single
   `tokio::time::timeout(PROBE_TIMEOUT, …)` so the doc becomes true; read only up to `\r\n` (or
   `take(8 KiB)`) instead of `read_to_end`, then parse the status line and stop. Add tests for
   slow-connect, slow-write and a server that never closes after `200 OK`.
   RISK: stopping at the status line means we no longer drain the response; with `connection:
   close` and a small status line this is safe, but a peer that writes a large body first could see
   a broken pipe — harmless for a probe, yet verify against the real server.

3. **Parse the status exactly (FM4).** Replace `contains(" 200")` with a check that the first line
   starts with `HTTP/1.` and whose second whitespace token parses to exactly `200`. Add tests for
   `2000`, a bare non-HTTP banner, and `HTTP/1.1 200` without a reason phrase.
   RISK: none expected — a stricter parser can only reject inputs that were never valid HTTP.

4. **Classify failures + legible exit contract (FM3).** Introduce a small typed error enum at the
   binary edge (`Unreachable`, `TimedOut { phase }`, `BadStatus(u16)`, `BadPath`) and map it to
   distinct exit codes: `0` healthy, `1` unhealthy/unreachable, `2` usage/config (clap already uses
   2). Include address, path and observed status in every message. Install a minimal JSON
   subscriber for the probe branch (or drop the dead `info!`).
   RISK: exit-code meaning is externally visible — DEVIATION + owner approval per `AGENTS.md`;
   confirm the desktop/container health checks only test zero-vs-nonzero before splitting.

5. **Teach callers that 404/400 are not "still starting" (FM1/FM4).** Cross-team item for
   `isDeploymentReachable`/`waitForHealth`: pass the base origin only (already documented) and
   surface the last status in the terminal error instead of looping to a 90 s kill. Add a test that
   a wrong suffix reports "unexpected 404" immediately.
   RISK: touches the desktop TS surface, not this crate; coordinate with its owner.

## Sequencing / cross-cutting risks

- Land #2 and #3 first (mechanical, no wire change); run `cargo fmt --check`, `clippy -D warnings`
  and the `req_053_*` suite after each step.
- #1 next — needs a DEVIATION entry if path handling changes. #4 last — needs the exit-code ADR
  and owner sign-off. #5 is a separate desktop change.
- Preserve the "no TLS, no redirects, no client library, no DB config" design and every existing
  `req_0xx_*` test name (traceability gate). Mark every status in this file CONFIRMED-by-source
  until the implementing session actually runs the new tests.

## Evidence consulted

`crates/bin/src/probe.rs` (full); `crates/bin/src/cli.rs:45–53,74–97`; `crates/bin/src/main.rs`;
`crates/api/src/health.rs:22–56,103–128`; `apps/desktop/src/stack.ts:238–253,443–451`;
`spec/01-requirements.md:433–439`; `spec/golden/ready.json`; `spec/05-known-defects.md`;
`docs/STATE.md:252–260`; sibling proposal `wave-11/0197-…-architecture.md` (format only).
