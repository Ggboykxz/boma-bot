# ADR-003 — Public interface: procedure-style JSON over `POST /rpc/{namespace}/{procedure}` with generated OpenAPI for the HTTP surface

Status: proposed (awaiting owner go) — relates to DEC-13

## Context

The spec fixes two interfaces (03): a procedure interface with 52 namespaces / 283 procedures, and a
set of plain HTTP endpoints described by `openapi.yaml`. Clients (web, desktop, mobile) already speak
the procedure shape, and the golden fixtures replay request/response pairs against procedure names.
AGENTS.md requires a type-safe contract with the Rust backend and "generate clients from the OpenAPI
document, never hand-write duplicates"; a contract test compares the generated document with
`spec/openapi.yaml`.

## Alternatives

1. **Hand-written Axum routes for the HTTP surface + a typed procedure dispatcher for `/rpc`, with
   `utoipa` generating the HTTP OpenAPI document and a small generator producing the TypeScript
   client from both.**
2. gRPC/protobuf for everything, with a gRPC-web gateway for the browser.
3. Plain REST for everything (one resource per entity), plus a separate WebSocket API for actions.
4. A single GraphQL endpoint, with the HTTP endpoints kept as-is.

## Scoring

| Criterion | 1 proc+utoipa | 2 gRPC | 3 REST | 4 GraphQL |
|---|---|---|---|---|
| 1 fits the spec | 1 | 3 (browser needs a gateway; shapes differ) | 4 (283 operations become bespoke) | 5 |
| 2 maturity | 1 | 1 | 1 | 2 |
| 3 security | 1 (schema-validated boundary) | 1 | 2 (authorization per route, easy to miss) | 4 (query depth abuse) |
| 4 performance | 1 | 1 | 1 | 2 |
| 5 simplicity | 2 | 5 | 2 | 4 |
| 6 typed contract | 1 | 1 | 2 | 3 |
| 7 licence | 1 | 1 | 1 | 1 |

## Decision

Option 1.

- `/rpc/{namespace}/{procedure}` is one Axum route. A registry maps `(namespace, procedure)` to a
  handler with typed `Request`/`Response` types; unknown pair → `NOT_FOUND`, schema violation →
  `BAD_REQUEST`, both in the stable error envelope from 03.
- Every procedure declares its input and output with `serde` (`deny_unknown_fields` where the spec
  says strict) and, where useful, `garde`/`validator` rules evaluated before the handler.
- The plain HTTP surface is declared with `utoipa` annotations and served as a generated
  `openapi.json`; a contract test diffs it against `spec/openapi.yaml`.
- Clients are generated: a TypeScript client from the OpenAPI document plus a thin procedure-codegen
  from the OpenAPI extensions (`x-rpc-namespace`, `x-rpc-procedure`) so the web app never hand-writes
  a URL.

## Consequences

- The procedure registry is the single place where authorization is attached, so an unauthenticated
  procedure is a deliberate annotation rather than an omission.
- Golden replay becomes trivial: a fixture names a procedure and two JSON documents.
- We keep the reference's procedure names (DEC-13, recommendation (a)) so fixtures and any existing
  client keep working; new capabilities we add must be additive.
- Streaming procedures (`threads.subscribe`) are the exception: they negotiate `text/event-stream`
  and are documented in 03 rather than in OpenAPI.
