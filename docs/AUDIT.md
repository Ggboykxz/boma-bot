# Audit — v0.1.0 release (post-release), run 43755912392

Date: 2026-11-15
Scope: the **published** release, not the dev tree. Verified by downloading the artifacts from
`github.com/Ggboykxz/boma-bot/releases/tag/v0.1.0`, checksumming against the release metadata,
booting the bundle against a fresh PostgreSQL 17 database, and walking the operator path end to end.

## What was checked

| Check | Result |
|---|---|
| SHA256 of all three assets vs the release metadata | OK (`9e8bbd…`, `bbc5e7…`, sums match) |
| Attestations published (Sigstore, keyless) | 1 for the server tarball subject, confirmed via API |
| Server bundle contents | binary, static export, 52 migrations, README/LICENSE/CHANGELOG/UPGRADING — all present |
| Migrations apply to a fresh database | 0001–0053 OK (`boma migrate`) |
| `serve` boots, `/healthz` `200`, `/ready` `200` with full capability payload | OK (`runtime: sandboxed`, `sandbox: none` fail-closed, `jobs: postgres`, `realtime: postgres`) |
| Sign-up, RPC `spaces.create`, `bots.create`, `threads/send` | OK (200s; JSON envelopes as per spec) |
| `runs/list` shows the queued run | OK (`status: queued`) |
| Worker drains the queue | **Blocked for 25 s without a `worker` process; instant with one (41 ms, scripted provider)** |
| Bot reply in the room (`threads/messages`) | OK (`role: bot`, `Boma Bot is ready to answer.`) |
| Compose stack starts a draining worker | **No — see the finding below** |

## Finding H-1 — HIGH — deployed containers never drain the run queue

The release ships a separate `worker` binary command; the HTTP `serve` never claims runs. Every
documented deployment path (README quick start, `compose.yaml`, `docs/UPGRADING.md`) started
`serve` alone, so production would run the API correctly and **never execute a single run**.

**Blast radius:** any deployment of v0.1.0 (Compose, systemd, manual) without a manually launched
worker.

**Root cause:** two-process design (`serve` / `worker`) documented correctly in the binary's
`--help` and in M0 notes, but the deployment artifacts were never updated to include the worker.
The CI smoke coverage stops at `/ready`.

**Fix shipped in this commit:** a `worker` service in `compose.yaml` (same image,
`command: ["worker"]`, `unless-stopped`, same env, waits on `migrate`), a step in
`docs/UPGRADING.md` for both upgrade paths, and a step in the README quick start. A deeper
question remains for the owner: should `serve` run the drain loop in-process (a config flag), or
is the two-process split the right production shape? Either is defensible; today nothing is.

**Verification:** fresh bundle + fresh DB + `boma serve` + `boma worker` → `run.execution_started`,
`run.first_response`, `run.finished`, `status: completed` in 41 ms; the bot's reply landed in the
room and showed via `threads/messages`. The smoke database `boma_v010` was dropped after the check.

## Observations, not findings (no action)

- The release binary declares a weak `GLIBC_2.39` symbol note on NixOS; it runs there and on
  Debian/Alpine containers. No user-facing issue; noted for M12 smoke parity.
- `BOMA_MASTER_KEY` enforcement is honest: mount mistake → clear startup error naming the
  format (64 hex chars), not a muted vault.
- The glibc note above, the `weakest form` of `BOMA_DEGRADED`, and the tokens-in-cookies pattern
  are all operator-facing and documented.

## Not audited (still owed before claiming a *quality* audit beyond the release)

- `/improve`-style measurements (latency percentiles, bundle budgets).
- Independent adversarial review of the money path (`REQ-018/051`) and the MCP surface.
- Store-of-record audit: whether `/ready` lies about the artifact store when `BOMA_ARTIFACT_DIR`
  is unset (ephemeral per-process directory — it answers healthy while losing bytes on restart;
  whether that deserves a loud warning at boot).

## Deferred by record (unchanged)

REQ-049 (voice, COULD), REQ-056 (mobile, SHOULD) — both recorded; `trace` stays 56/58.
