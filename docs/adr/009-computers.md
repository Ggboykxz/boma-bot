# ADR-009 — Computers: a provider trait with Docker-supervisor and desktop-host implementations first

Status: proposed (awaiting owner go) — this is DEC-08

## Context

A bot drives a sandboxed machine: screen, mouse, keyboard, shell, browser profile, files
(REQ-026 to REQ-031). The reference supports 14 providers. `04-nonfunctional.md` requires fail-closed
behaviour and a capability report in `/readyz`; `05-known-defects.md` D-05 requires that private
endpoint access be scoped and audited.

## Alternatives

1. **A `ComputerProvider` trait with two in-repo implementations (Docker supervisor, desktop host)
   and cloud providers as later adapters.**
2. Ship only cloud providers (E2B-class), no local option.
3. Ship only Docker.
4. No abstraction: hard-code one provider.

## Scoring

| Criterion | 1 trait + 2 impls | 2 cloud only | 3 docker only | 4 no abstraction |
|---|---|---|---|---|
| 1 fits the spec | 1 | 3 (self-hoster without a cloud account cannot use the product) | 3 (a laptop user cannot) | 4 |
| 2 maturity | 2 | 1 | 1 | 1 |
| 3 security | 1 (network policy per provider, fail-closed) | 2 | 1 | 3 |
| 4 performance | 1 | 1 | 1 | 1 |
| 5 simplicity | 3 | 2 | 1 | 1 |
| 6 typed contract | 1 | 1 | 1 | 1 |

## Decision

Option 1. The trait carries create/destroy/status/screen/input/exec/upload/download and a
capability struct (screen, audio, clipboard, gpu, max lifetime). Two implementations ship in M1:

- **docker-supervisor**: a small service over the Docker API with a fixed image, no host mounts, no
  privileged mode, an egress policy, and a per-computer network namespace. It runs on the same host as
  the worker only when the operator enables it.
- **desktop-host**: drives a local machine the operator already trusts, for the single-user case
  (REQ-055).

Selection is explicit configuration; a provider that is not configured reports `unavailable` in
`/readyz` and the `computer.status` procedure returns a typed refusal — never a silent fallback to a
fake provider (which is what the reference's `fake`/`*-emulator` selection makes possible).

## Consequences

- Cloud providers are additive adapters; adding one must not change the contract, and a provider
  matrix test runs the shared conformance suite against each implementation.
- The screen capability (ADR-005) is minted by the API, so a provider never sees a user session.
- Network egress defaults to deny with an explicit allow-list per space, closing D-05 at the source
  rather than behind a flag.
