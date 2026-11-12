# 00 — Overview

**Purpose.** The product gives a person or a team persistent AI teammates ("bots") that keep one
conversation each, remember across sessions, work on schedules, call integrations, and operate a
real computer (browser, shell, desktop) inside a sandbox. It runs self-hosted: a long-running HTTP
API, a background worker, Postgres, and one or more computer providers, behind a web app, an
Electron desktop shell and an Expo mobile app.

**Users and roles.**
- **Owner / member of an organization** — signs up, creates spaces, invites people, owns bots and
  pays for model usage.
- **Space member** — uses the bots in a space, sends messages, answers approvals, takes control of a
  computer. Roles: owner, admin, member (role strings live on `Member.role`).
- **Space admin** — manages members, invitations, integration and approval policy, plugins.
- **Server operator** — deploys with environment configuration, runs migrations, reads health and
  audit surfaces. **Not** a product role: signing up never grants server administration.
- **Bot** — an actor inside the system: it has a user id, a thread, a memory, a computer, secrets and
  tools; it acts only through the run executor and its approval rules.
- **External systems** — model providers, connectors, MCP servers, messaging platforms, inbound
  e-mail relays, Stripe, push notification services.

**Scope of this specification.** Everything a rebuild must reproduce to be a functional equivalent:
identity and tenancy, conversations and runs, the agent loop and its tools, approvals and spending,
memory, routines, computers and takeover, connectors/MCP/plugins/skills, artifacts and shared pages,
channels (e-mail, messaging, voice, webhooks), billing and model management, and the operational
surface (health, audit, logs, configuration). The web, desktop and mobile clients are in scope as
surfaces with defined screens and states (see 08).

**Explicit non-goals** (do not rebuild):
- The marketing and documentation website (`apps/www`) and the three translated READMEs. Parity does
  not depend on them; only the in-product documentation links matter.
- The project's own build/test tooling as user-facing behavior: `packages/testkit`, the eval and
  performance harnesses, screenshot pipelines, smoke scripts. A rebuild needs its own test strategy,
  not these scripts.
- Provider-specific emulators and fakes used for tests (`fake-*`, `*-emulator`, `ScriptedAgentRuntime`).
  They describe a testing technique, not required behavior.
- The legacy Graphile run-delivery path kept for rolling upgrades. The rebuild needs one dispatch
  mechanism with the same observable guarantees.
- Brand assets, the domain, the app-store identities and the original's wording. The rebuild ships
  under its own name.

**Interfaces.** The product exposes two external interfaces: a JSON-RPC-style API over `POST /rpc`
(oRPC contract, 52 namespaces, 283 procedures) and a set of plain HTTP endpoints for browsers, webhooks,
public links, assets and health. Both are specified in 03 and `openapi.yaml`.
