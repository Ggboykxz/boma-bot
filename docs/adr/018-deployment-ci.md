# ADR-018 — Deployment, CI and release

Status: proposed (awaiting owner go)

## Context

Self-hosted, one binary plus Postgres plus an object directory (ADR-001, ADR-010). The reference
shipped Compose with eight services and a web proxy. Quality bar requires reproducible builds, a
migration path, checksums and a CI that runs every gate on every push.

## Alternatives

- **Runtime**: (a) Docker Compose with three services (db, app, optional supervisor); (b) Kubernetes
  manifests; (c) bare-metal binary + systemd unit.
- **Release**: (a) `cargo-dist` with signed archives and a container image; (b) hand-rolled scripts;
  (c) publish only a container image.
- **CI**: (a) GitHub Actions with a Postgres service container; (b) self-hosted runner; (c) no CI.

## Scoring

| Criterion | Compose | k8s | systemd only |
|---|---|---|---|
| 1 fits the spec (self-host, one command) | 1 | 3 | 1 |
| 5 simplicity | 1 | 4 | 1 |
| 3 security | 1 | 2 (more surface to misconfigure) | 2 |

| Criterion | cargo-dist | scripts | image only |
|---|---|---|---|
| 5 simplicity | 1 | 3 | 2 |
| 7 licence | 1 | 1 | 1 |
| DX (checksums, changelog) | 1 | 4 | 3 |

## Decision

- Compose file with `db`, `app` (which runs `migrate` then `serve`), and an optional `supervisor`
  profile; a documented systemd unit for bare metal. Both serve the same static web build from the
  binary.
- `cargo-dist` releases signed archives (Linux x86_64/aarch64, macOS) plus a multi-arch image;
  `Cargo.lock` committed and `--locked` everywhere.
- GitHub Actions jobs: fmt, clippy `-D warnings`, tests (Postgres service), `cargo deny`, `machete`,
  coverage with a floor, `trace_check.py`, golden replay, contract diff against
  `spec/openapi.yaml`, secret scan, UI build + Playwright + axe.
- Migrations run under an advisory lock, forward-only, with a documented backup step before upgrade.

## Consequences

- "One command to run" stays true: `docker compose up` or `boma migrate && boma serve`.
- Compose is the reference deployment, not the only one; nothing in the app assumes Docker.
- CI minutes are the main cost (a Postgres service per job); jobs are cached and sharded by crate.
