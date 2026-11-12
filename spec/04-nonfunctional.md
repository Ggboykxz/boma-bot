# 04 — Non-functional requirements

These are the constraints a rebuild must satisfy to be operationally equivalent. Each one is derived
from code in `target/rakazo`; none relies on runtime measurement (see `notes/validation-audit.md`,
`runtime: no` on every row).

## 4.1 Deployment shape

- **Self-hosted, single repository, multiple processes.** `apps/api` (HTTP), `apps/worker` (run
  executor), `apps/web` (React SPA, served statically and proxying `/api`, `/rpc`, `/internal`),
  plus optional `apps/desktop` (Electron) and `apps/mobile` (Expo). Postgres is the only required
  datastore; Redis is optional (`packages/adapters/src/realtime.ts` falls back to in-process).
- **Configuration is environment-only** (`apps/api/src/env.ts`, `apps/worker/src/env.ts`), validated
  at boot; an invalid environment fails the process instead of degrading. 228 distinct environment
  keys exist in the reference. A rebuild must validate at boot and must fail closed on unknown
  required values.
- **Migrations run out of band** (253 SQL migrations under `packages/db/prisma/migrations`), never at
  request time. A fresh boot without migrations must fail readiness, not serve partial traffic.

## 4.2 Concurrency and correctness

- **One executor per run.** Leases carry an owner and a monotonically increasing fence
  (`Run.leaseOwner`, `Run.leaseFence`, `Run.leaseExpiresAt`); every state transition is conditional on
  the fence it read (`packages/core/src/run-state.ts`, `packages/db/src/run-lease.ts`). A stale
  executor must be unable to write.
- **At most one active run per bot thread.** Enforced before enqueue, not after
  (`packages/db/src/run-queue.ts`).
- **Exactly-once external effects.** `ExternalEffect.effectKey` is unique per run; an approval that
  fires twice produces one effect.
- **Idempotent client mutations.** `clientNonce` on message send and artifact create is unique per
  (thread|space, user); replay returns the original row.
- **Monotonic per-thread sequence.** `Thread.nextMessageSeq` / `nextEventSeq` are allocated under the
  same transaction that inserts the row; no gaps are observable to clients and no reordering is
  possible between a message and its event.
- **Recovery.** Orphaned leases expire and return the run to `queued` (REQ-014). The recovery sweep
  must be safe to run in every replica at once.

## 4.3 Performance envelope

The reference does not ship numeric SLOs; these are the structural limits a rebuild must preserve
(values are the reference's own caps):

| Surface | Limit | Evidence |
|---|---|---|
| Auth request body | 64 KiB | `apps/api/src/request-body-limit.ts:5` |
| RPC request body | 16 MiB, scaled to `max(16 MiB, base64(upload) + 1 MiB)` | `request-body-limit.ts:6,73` |
| Attachment bytes | `ATTACHMENT_MAX_BYTES` from contracts, count and MIME allow-listed | `packages/core/src/attachments.ts:33-101` |
| Screen-target proxy | 16 KiB | `apps/api/src/screen-proxy.ts:12` |
| Plugin submission | 32 KiB | `apps/api/src/plugin-submissions.ts:37` |
| Bot webhook | 64 KiB | `apps/api/src/webhook.ts` |
| Inbound mail | 25 MiB | `apps/api/src/email-inbound.ts` |
| Messaging webhook | 256 KiB | `apps/api/src/routes/messaging.ts` |
| Billing webhook | 512 KiB | `apps/api/src/billing/webhook.ts` |
| Page read | 120 / min / IP | `apps/api/src/shared-pages.ts:19` |
| Plugin submission | 10 / hour / IP | `apps/api/src/plugin-submissions.ts:139-154` |

Streaming: replies must reach the client incrementally (token chunks, tool cards, run status), not
at the end of the turn (REQ-020). History paging is cursor-based and bounded; the client must never
need to load a whole thread to render it (REQ-005).

## 4.4 Reliability

- **Runs survive a deploy.** A rolling restart must not lose in-flight work: leases are renewed and
  expire into recovery (REQ-013, REQ-014).
- **Dispatch coalesces.** N pending triggers on one thread produce one queued turn, and a burst of
  routine fires collapses to the next sensible slot rather than N runs (REQ-024).
- **Retries are bounded and visible.** `failed → queued` is the only return from a terminal state, and
  the retry count is user-visible (REQ-021).
- **Degraded readiness is explicit.** `/ready` reports each subsystem (`sandbox`, `integrations`,
  `messaging`, `email`, `billing`, `jobs`, `realtime`) and returns `503` when the database probe fails
  (`apps/api/src/health.ts`), so an orchestrator removes the instance instead of serving errors.

## 4.5 Security and privacy

- **Tenant isolation is enforced below the service layer.** Postgres RLS is installed by migrations
  (26 of 253) and every query runs inside `runAsTenant`; the only escape is an explicit
  `runAsSystem(reason)` call, which must stay rare, reviewed and justified in code
  (`packages/db/src/row-security.ts`). A rebuild that picks service-layer tenancy instead must prove
  the same guarantee with tests that attempt cross-tenant reads (see ADR-002).
- **Uniform error surfaces.** Resources of another tenant answer `NOT_FOUND`, never `FORBIDDEN`; bot
  webhook auth failures are indistinguishable from unknown bots (REQ-043).
- **Secrets are write-only.** Sealed at rest with AES-GCM, key per space wrapped by a master key; no
  procedure returns plaintext (REQ-046). Logs, audit rows and exports never contain secret material
  (`packages/logging/src/redaction.ts`).
- **Untrusted content is contained.** Bot-authored HTML renders under a strict CSP with no
  `unsafe-eval`, `object-src 'none'`, `frame-ancestors 'none'`, in a separate sandboxed document
  (`packages/contracts/src/content-security-policy.ts:34-70`).
- **SSRF is denied by default.** Schemes, credentials, non-standard ports, private and metadata
  addresses are rejected after DNS resolution, and redirects are re-checked
  (`packages/adapters/src/web-ssrf.ts:52-150`).
- **Local command execution fails closed.** stdio MCP only starts binaries from
  `MCP_STDIO_ALLOWED_COMMANDS`, empty by default (`apps/api/src/env.ts:360`).
- **No anti-features in production.** `GET /api/dev/emails` must not be mounted when
  `NODE_ENV !== "development"` (V-02); a rebuild should not ship the route at all.

## 4.6 Observability

- **Structured logs with correlation.** One JSON log per event; request id, trace id and span id
  generated or parsed from `traceparent`; job payloads carry correlation through the queue
  (`packages/logging/src/correlation.ts`, `ids.ts`, `jobs.ts`).
- **Redaction and pseudonymisation are in the log path**, not left to call sites
  (`packages/logging/src/redaction.ts`, `pseudonym.ts`).
- **Optional OTLP tracing** (`@opentelemetry/sdk-trace`, `exporter-trace-otlp-http` —
  `packages/adapters/package.json:46-49`) and optional Sentry/Axiom sinks; absent configuration must
  mean "off", not "degraded".
- **Health endpoints** `/health` (liveness) and `/ready` (dependency + capability report) are part of
  the contract (REQ-053); `/internal/health` reports worker progress for the supervisor.
- **Audit and security events are append-only rows** with actor, space, target and metadata, exportable
  as JSONL without secrets (REQ-045).

## 4.7 Internationalization and accessibility

- **15 locales ship** (`packages/i18n/locales`: de, en, es, fr, he, hi, ja, ko, nl, pl, pt-BR, ru, tr,
  zh-CN, zh-TW) via compiled Lingui catalogues. Two of them are right-to-left (he, ar-style layout
  must be mirrored even though `ar` is not shipped).
- **No user-visible English inside components**: strings come from the catalogue, and errors are
  mapped to a localised run-error message (`packages/i18n/src/run-error.ts`).
- **Accessibility**: every interactive control is reachable by keyboard, cards that ask for input move
  focus, live regions announce run status, and colour is never the only signal for a state.
- **Layout must survive latency**: skeletons for threads, activity and artifacts instead of blank
  panels.

## 4.8 Browser and client support

- Web: current evergreen browsers with ES2022 support; SSE required; no service worker that caches
  authenticated responses.
- Desktop (Electron) and mobile (Expo) clients consume the same RPC contract and must be able to
  authenticate without a browser session (device pairing, `apps/api/src/devices.ts`).
- Every client must degrade when realtime is unavailable: polling or refetch on focus, with a visible
  "not live" indication rather than silent staleness.

## 4.9 Compatibility and data growth

- **Migrations are forward-only and additive** in the reference; a rebuild keeps that rule and adds a
  documented downgrade story for the operator instead.
- **Retention**: agent homes and artifacts are reclaimable; deletion is recorded (`HomeDeletionNotice`)
  rather than assumed.
- **Export must stay readable across versions**: the account export carries a `version` field
  (see 02, "Import / export formats") and the importer rejects unknown versions instead of guessing.
