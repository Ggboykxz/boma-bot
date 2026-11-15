# ADR-024 — Operate: export/delete the account, audit export, SCIM off a switch, observability by flags, admin surfaces gated by role

Status: accepted (2026-11-15, milestone M9 — REQ-044, REQ-045, REQ-054, REQ-058).

## Context

The spec asks for four things that until M9 operators could only do with SSH into the database:

- **REQ-044** — a member exports their own data (bots, threads, messages, artifacts, routines,
  memory), imports it somewhere else, and deletes the account.
- **REQ-045** — the audit trail is exportable, sign-in failures are recorded, CSP violations are
  reported by the browser, and an enterprise directory (SCIM) provisions and deprovisions accounts.
- **REQ-054** — the system is observable: structured logs with a request/trace id, a metrics
  endpoint, correlation across the process.
- **REQ-058** — admin surfaces exist: diagnostics (already, M0), plugin-submission review,
  access grants, deployment settings; plain members are refused.

## Decision

### REQ-044 — export, import, delete

1. **The archive is a flat zip, not a directory tree.** `manifest.json` (`{version: 1, exportedAt,
   userId, counts{bots, threads, messages, memory, routines, artifacts}}`) plus one `<section>.json`
   per section (`profile`, `bots`, `threads`, `messages`, `memories`, `routines`, `artifacts`) and
   one `artifacts/<storage_key>` file per artifact whose bytes fit. Entry names are validated by the
   importer against a closed allow-list; a path that escapes is refused outright, a section whose
   JSON no longer matches the schema is named in `refused`, never silently dropped, and the rest
   still imports.
2. **Export is rendered on demand.** `export.start` mints an `exp_…` id and the row is created
   `pending`; `export.status` and `export.download` render the archive at call time (the bundle is
   small for any account that fits 32 MiB) and `mark_ready` records the byte count and the skipped
   paths. There is no background job to corrupt or orphan: the export is a pure read of the bundle
   with a writer on top.
3. **Import re-mints ids, never reuses them.** Bots land in the caller's default space, mapped by
   name; thread/message references to artifacts are re-linked only when the artifact is
   writable, otherwise they are re-created content-addressed (the bytes carry their own key).
   Per-entry and whole-archive caps (8 MiB / 32 MiB) are enforced on read, not just on render.
4. **Delete is refused, then cascades.** The account may not be deleted while any space it belongs
   to has another member, or while any owned space has an active `pro` subscription — the message
   names the space and points at billing. Otherwise every sole-member organization is deleted
   (its spaces and every tenant row cascade), the person row disappears (sessions die by FK
   cascade), and artifact bytes are removed best-effort out-of-band.
5. **The audit trail survives deletion.** `account.deleted` is appended with `space_id = NULL`
   (deployment-scoped) before the cascade; `audit_events.space_id` became nullable in 0045 with
   `ON DELETE SET NULL`, so the trail is never erased with the account.

### REQ-045 — the trail, the security events, the directory

6. **Audit is one JSONL shape, version 1.** `audit.export` answers `{header: {version, exportedAt,
   spaceId, events, truncated}, jsonl}` with the header as line one and one event per line below it;
   secret-shaped values are redacted to `[redacted]` at render time (`sk-`, `ghp_`, `xox*`, bearer
   tokens, AKIA/AIza). The cap is 4 000 rows, oldest first in the file; the header says whether it
   truncated.
7. **`security.events.list`** returns the `security.*` kinds of the caller's space (sign-in failed,
   CSP violation), newest first, 200 rows, redacted the same way. Deployment-scoped rows
   (`space_id = NULL`) are never in this list; they exist for the operator and for `audit.export`.
8. **SCIM is a switch, not a product.** `BOMA_SCIM=true` + `BOMA_SCIM_TOKEN` (both or neither,
   ≥16 chars) makes `/api/auth/scim/v2/ServiceProviderConfig|Users[/{id}]` exist; otherwise every
   SCIM route is uniformly 404. The bearer is compared over the hash, in constant time. `POST`
   provisions: with a password the person signs in immediately; without one the account is born
   disabled. `PATCH` toggles `active`; deactivating kills sessions and memberships at that instant.
   There is no Groups sync, no bulk, no password change. Audit kinds `scim.provisioned` and
   `scim.deprovisioned` carry `space_id = NULL`; `ScimDirectorySync` is an enum-ordered
   `SystemReason` like every other.
9. **CSP reports are first-class.** `POST /api/csp-report` (public, `application/csp-report` and
   `application/reports+json`) records host-only violations (query strings never persist), rate
   limited at 60/hour per IP (D-01), with a `security.csp_violation` audit row per violation and a
   `boma_csp_violations_total{directive}` counter.

### REQ-054 — observability

10. **One JSON log per event.** `boma.http` per request (method, route template, status, latency,
    `request_id`, `trace_id`, `span_id`) and `boma.runs` per run lifecycle step
    (`run.execution_started`, `run.first_response`, `run.finished`). Route templates are logged,
    never raw paths: a share token or an installation id never reaches a log line.
11. **W3C trace context.** `traceparent` is parsed strictly (`00-<32hex>-<16hex>-<2hex>`, zero
    ids refused, malformed = absent); an absent header means "mint one", never "fail the request".
    The request id is echoed in `x-request-id` and threaded into the `http.request` span.
12. **`/metrics` is a flag, not a route for everyone.** `BOMA_METRICS=true` exposes the
    hand-rolled Prometheus text format (counters only: http requests, runs started/first-response
    ms sum / total, csp violations); without the flag the route answers 404 like every other
    operator surface.

### REQ-058 — admin surfaces, server and web

13. **`SpaceRole::Owner|Admin` gates every admin procedure.** Members are refused with a
    first-class `403` (`ErrorCode::Forbidden`, fixed message). The gate lives in one helper
    (`require_space_role`) so no admin procedure silently drifts from the rule.
14. **Access grants are a deployment allow-list, not a feature switch.** `admin.grants.*` writes,
    lists and revokes rows in `access_grants` (email, reason, granted_by, created_at). Nothing
    consumes grants yet: they are the primitive the spec asks for ("a hosted deployment can end a
    person's access with one row"), wired now so M11+ can flip a kill-switch onto it. Recorded in
    the audit trail as `admin.grant_created` / `admin.grant_revoked`.
15. **The plugin-submission review queue is the spec's `approve/reject`.** `admin.plugins.list` /
    `admin.plugins.decide` (`approved|rejected` only) flip a pending row once and audit the
    decision; a second decision is a 409, never a silent no-op. The submitter is **not notified**:
    there is no outbound mail in this deployment (recorded as a gap; the review is visible in the
    trail).
16. **The two screens are real but honest.** `/admin/access` (members, pending invitations, grants
    with create/revoke) and `/admin/plugins` (queue with approve/reject). No role-change control
    and no member removal: the spec asks for the surfaces, not for those verbs, and the latter
    need the same "recorded, reversible" discipline as the grants before they ship.
17. **`last_seen_at` is `null` today.** The sessions table does not carry a "last seen"; the
    screen renders it only when the backend can answer honestly. Not faked.
18. **`deployment.settings.*` and OTLP-style forwarding stay deferred** (DEC-18). `/metrics` is the
    operator surface; no settings row exists for the "operator-editable deployment singleton"
    yet.

## Alternatives considered

1. **Retention tiers for deletion (30 days export-reminder, then erase).** Rejected: the spec says
   deleted is deleted; a customer who wants their data back uses the export first.
2. **A durable background job for the export.** Rejected: a job that does not exist is a row in a
   queue that can lie about its progress; rendering on demand is strictly more honest for the
   sizes we support.
3. **`<section>/<id>` with one file per record.** Rejected in favour of flat `<section>.json`:
   fewer entries, simpler importer allow-list, same information density.
4. **`AdminGrants` consumed by the sign-in gate.** Rejected for M9 scope: grants recorded, nothing
   consumes them yet. The spec asks for the allow-list surface ("a deployment can end a person's
   access with one row"), not for the middleware that reads it, and no untested gate should ship
   half-enforced.
5. **Outbound email notification on submission decisions.** Rejected for this milestone: no
   outbound mail adapter exists; faking a delivery would be a lie.

## Consequences

- An operator can export, import and delete without SSH.
- A signed-in member who is not owner/admin sees a 403 sentence on the admin pages and on the
  procedures; nothing admin leaks through.
- CI carries `cargo-deny` with `bzip2-1.0.6` explicitly allowed (the `zip` crate); documented in
  `deny.toml` and `docs/DEPENDENCIES.md`.
- The trace budget improves from 6 to 4 missing REQ ids (REQ-010, REQ-049, REQ-055, REQ-056).

## Named gaps (deferred, and where)

- **SCIM Groups, bulk, password change** — out of scope, stated in the design doc.
- **OTLP forwarding** (DEC-18) and `deployment.settings.*` — not wired.
- **Submission-review notifications** to the submitter — no outbound mail; recorded in STATE.md.
- **Consumption of `access_grants`** by a gate — M11+ candidate.
- **`last_seen_at`** for the members table — needs a sessions `last_seen_at` column; not shipped.
- **The two full security events list UI** — the JSON is available; the screen did not ship.
- **Retention tiers for deletion** — deleted is deleted.

## Evidence

- `crates/app/src/account_export.rs`, `crates/app/src/usecases.rs` (export, delete, audit,
  security events, SCIM helpers, `require_space_role`, admin use cases),
  `crates/app/src/ports.rs` (`AccountExportStore`, `AdminStore`).
- `migrations/0045_audit_cors.sql`, `0046_account_exports.sql`, `0047_access_grants.sql`.
- Wire: `crates/api/src/wire.rs`, `crates/api/src/rpc.rs` (`export.*`, `account.delete`,
  `audit.export`, `security.events.list`, `admin.*`), `crates/api/src/public.rs` (SCIM routes,
  `/api/csp-report`, export download + import).
- Web: `apps/web/components/admin/`, `apps/web/app/admin/`, `apps/web/lib/api/rpc.ts`.
- `docs/superpowers/specs/2026-11-15-m9-operate-design.md` — the design this ADR ratifies.
