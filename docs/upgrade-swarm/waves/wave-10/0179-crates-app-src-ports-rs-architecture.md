# Wave 10 — Worker 0179 — `crates/app/src/ports.rs` (architecture) — v0.4.0 proposal

Scope: read-only architecture review of `crates/app/src/ports.rs` (4,601 lines). All claims below are CONFIRMED by direct reading unless marked GUESS. No behavior change is proposed unless noted. Nothing else was modified.

## Current trait / ports boundary structure (CONFIRMED)

The file is the application layer's entire outbound boundary: ~60 public traits plus 100+ payload/read-model structs, all flat in one module. Two families share one namespace:

- Persistence ports — `*Store` (e.g. `AccountStore`, `RunStore`, `ArtifactStore`, `NotificationStore`): SQLx/Postgres-backed.
- Gateway ports — `*Transport`, `*Runtime`, `*Provider`, `*Runner`, `*Fetcher`, `*Client`, `*Generator`, `*Hasher`, `Clock`, `EventPublisher`, `EndpointSafetyChecker`: HTTP / CLI / crypto / in-process adapters.

Structs follow a consistent shape: `New*` write payloads, `*Summary`/`*Record`/`*View` read models, `*Spec` desired state, `*Handle`/`*Outcome`/`*Decision` results. Rustdoc is thorough and REQ-tagged; preserve it verbatim.

Two hard invariants are encoded in signatures and must survive any refactor: (1) sequences are store-allocated, never caller-computed (REQ-005/020); (2) a multi-table write is one port call (REQ-004).

## What is fragile (CONFIRMED)

- Monolith: one 4.6k-line file, no sub-module boundaries; every adapter and use case imports from one flat namespace.
- Two id regimes: older ports use domain newtypes (`BotId`, `ThreadId`, `TenantContext`); newer ones (REQ-036+: Artifact/Page/Ticket/Mail/Messaging/Model/Billing/Admin/Notification) use raw `&str`/`String` for ids.
- Closed sets encoded as `String` in read models (`plan`, `status`, `auth_state`, `visibility`, `policy`, `platform`).
- `TenantContext` convention is uneven: present on some methods, absent on system-scoped ones, qualified two ways (`TenantContext` vs `boma_domain::TenantContext`), and in varying parameter positions.
- Secret-reveal methods sit beside CRUD (`SecretStore::reveal`, `McpServerStore::reveal_bearer`/`reveal_client_secret`, `ConnectorStore::connected_credential`, `MessagingStore::reveal_token`/`reveal_webhook_secret`, `NotificationDestinationStore::destination_webhook`), each carrying a "never logged / returned" warning — a rule the type system does not enforce.
- God traits: `RunStore` (~11 methods, mixing member CRUD, worker claims/fences and activity projection), `McpServerStore` (~12, mixing CRUD, OAuth sessions, secret reveal and tool catalogues), plus `ArtifactStore`, `MessagingStore`, `NotificationStore`. `record_skip` already needs `#[allow(clippy::too_many_arguments)]`.
- Sync ports (`Clock`, `IdGenerator`, `TokenGenerator`, `EventPublisher`) sit alongside async ones with no grouping.
- Two methods share the name `catalogue_tools` with different return types (`McpServerStore`, `ConnectorStore`).

## Proposed upgrades for v0.4.0

1. Split the flat file into a `ports/` module tree behind a facade.
   - `ports::identity` (Account, User, Session, PasswordReset, Invitation, Admin, SignIn); `ports::conversation` (Bot, Thread, Message, Run, Event/EventPublisher, Card, Reaction, ReadState, Roster, Effect, PendingToolCall); `ports::resources` (Space, Group, Memory, Routine, Artifact+Content, Page, Ticket, Mail, Notification+Destination); `ports::integrations` (McpServer/McpTransport, Connector/ConnectorRunner, PluginFetcher/Install, OAuthClient, Messaging*, ExternalAgent, AcpAgent); `ports::security` (SecretStore, TokenGenerator, IdGenerator, PasswordHasher, EndpointSafetyChecker); `ports::platform` (ComputerRuntime/Store, ScreenCapability, Model*/Billing*, RateLimit, Audit, Tool); `ports::support` (Clock).
   - Keep `ports.rs` as a `pub use` facade so existing `crate::ports::X` paths still compile.
   - Risk: LOW — mechanical, no behavior change; import churn in app/infra. Mitigate with the facade and one module per commit.

2. Extract a dedicated secret-reading capability port.
   - Move every `reveal_*` / `*_credential` / `destination_webhook` method into `ports::security::CredentialReader` (or per-domain `SecretReader`); CRUD stores keep only write/list/delete. A use case must take the read bound explicitly.
   - Risk: MEDIUM — touches every secret-bearing infra adapter and DI wiring; preserve `AppError` semantics and keep plaintext in `secrecy::SecretString`. Payoff: the "never logged / returned" rule becomes greppable and reviewable.

3. Converge ids and closed sets on domain newtypes/enums, keeping wire strings identical.
   - Add/extend domain newtypes (`ArtifactId`, `PageId`, `TicketId`, `InstallId`, …) and enums (`Plan`, `SubscriptionStatus`, `McpAuthState`, `PageVisibility`, `MailPolicy`); retain exact serde wire values via `#[serde(rename)]`.
   - Risk: MEDIUM-HIGH — wide mechanical change across domain/app/api; any wire-string drift is a DEVIATION needing approval. Land per aggregate, behind existing golden/contract tests.

4. Split god traits by responsibility.
   - `RunStore` → `RunStore` (member CRUD) + `RunQueue` (`claim_next_queued`, `retry`, `set_steering_*`, fences) + `ActivityProjection` (`list`). `McpServerStore` → CRUD + `McpOAuthStore` (session store/take, secret reveal) + `McpCatalogue` (`catalogue_tools`). Rename the two colliding methods to `mcp_catalogue_tools` / `connector_catalogue_tools`.
   - Risk: MEDIUM — use cases gain a second trait bound; update where-clauses and mocks incrementally. No behavior change beyond naming.

5. Standardize the `TenantContext` contract and separate sync support ports.
   - Rule: every tenant-scoped method takes `&TenantContext` first; system-scoped methods move to a clearly named `System*` trait or are documented as such; use the single imported alias consistently. Document in `ports::support` that those ports are sync and in-process.
   - Risk: LOW-MEDIUM — signature reordering touches all implementations and mocks; do it module by module, compiling after each.

## Sequencing and gates

Order: 1 → 5 → 4 → 2 → 3 (cheapest / internally-visible first; wire-affecting change last). Each step: `cargo fmt --check`, clippy `-D warnings`, then tests; record any wire change in `docs/PARITY.md` before shipping. Preserve all REQ-tagged rustdoc and the two store-allocation invariants throughout.

GUESS: trait and method counts are from a manual read, not a compiler pass.
