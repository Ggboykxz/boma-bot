# ADR-001 — Data store: PostgreSQL with SQLx

Status: proposed (awaiting owner go)

## Context

The spec needs: strong multi-tenant isolation (REQ-043), a run queue with leases and fences
(REQ-012, REQ-013), append-only revisions for memory and artifacts (REQ-022, REQ-036),
monotonic per-thread sequence allocation (REQ-005), transactional outbox-style event fan-out
(REQ-020), full-text search over messages (`search` namespace), and JSON columns for message
blocks and provider payloads. `04-nonfunctional.md` fixes "Postgres is the only required
datastore; migrations run out of band".

## Alternatives

1. **PostgreSQL 16+ with SQLx (compile-time-checked queries, offline metadata committed).**
2. SQLite (single file, embedded, no server) with SQLx or rusqlite.
3. MySQL 8 with SQLx.
4. A document store (MongoDB-class) with an application-level queue.

## Scoring against the criteria (1 best – 5 worst)

| Criterion | PG+SQLx | SQLite | MySQL | Document store |
|---|---|---|---|---|
| 1 fits the spec | 1 | 4 (no concurrent writers, weaker JSONB, no RLS) | 2 | 4 |
| 2 maturity | 1 | 1 | 1 | 2 |
| 3 security record | 1 | 2 | 3 | 4 |
| 4 performance on small hosts | 1 | 1 (reads) / 4 (writes) | 2 | 3 |
| 5 simplicity for one maintainer | 2 | 1 | 3 | 2 |
| 6 typed contract | 1 (SQLx macros, newtype mapping) | 1 | 2 | 4 |
| 7 licence | 1 (PostgreSQL licence) | 1 (public domain) | 1 (GPLv2, fine as a server) | varies |
| 8 a11y/i18n | n/a | n/a | n/a | n/a |

## Decision

PostgreSQL with SQLx. Migrations are plain SQL under `migrations/`, applied by a dedicated binary
(`boma-migrate`), never at request time. Queries use `sqlx::query!`/`query_as!` so a schema drift is a
compile error, with `.sqlx` metadata committed for offline builds. An SQLite profile is explicitly
rejected: it cannot provide row-level security, and REQ-043's guarantee is the reason this project
exists.

## Consequences

- Deployment needs a Postgres instance; Compose ships one, and the README documents the minimal
  setup. This matches the reference, so operators are not surprised.
- RLS becomes available as the second line of defence (ADR-002), which is why the store choice and
  the tenancy choice are linked.
- SQLx compile-time checks require either a live database or committed offline metadata in CI; both
  are configured in M0.
- JSON-heavy columns (message blocks, provider payloads) stay `jsonb` with a typed deserializer at
  the domain boundary, never raw `serde_json::Value` in business code.
