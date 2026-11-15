# M9 — Operate (REQ-044, REQ-045, REQ-054, REQ-058) — design

**Intent.** Let the operators and auditors run the product without SSH into the database:
take the data out, take control back, and see what the system is doing. Nothing here grants
new product surface to members; every new path is admin-gated or operationally scoped.

**REQ-058 note.** "space admin" is `SpaceRole::Owner|Admin`; REQ-058 is scoped per space for
the "access" half, and deployment-scoped for the "plugins" half (the submission queue has no
space context, by design).

## Goals

1. **Account export / import / delete (REQ-044).** A member exports their account archive:
   one zip, `manifest.json` (`{version, exportedAt, userId, counts}`), then one entry per
   section — `bots/`, `threads/` (with `messages/` nested under each thread), `artifacts/`,
   `routines/`, `memory/` — each referenced by the manifest. Import into a new
   account: entry names validated against the archive root (no zip-slip), ids remapped once,
   per-section best-effort with a report of what was skipped. Delete the account:
   sessions revoked, active paid subscription refused (pointed at billing), data removed by the
   cascade policy stated in the doc (no retention by default).
2. **Audit export (REQ-045).** `GET /internal/audit?spaceId=…` is admin-gated JSONL:
   `{id, spaceId, actorUserId, kind, target, metadata, createdAt}`, oldest first, `version: "1"`,
   no secret material — the redaction of metadata values that look like keys happens at render
   time (values matching known key shapes are replaced by `[redacted]`). Security events join
   the append-only set as new `AuditKind`s (sign-in failures, password resets, deprovisions,
   csp violations, export/deletion).
3. **SCIM (REQ-045).** `/api/auth/scim/v2/Users` + `ServiceProviderConfig` — bearer-auth,
   deployment-configured (`BOMA_SCIM=true` + `BOMA_SCIM_TOKEN`, all-or-nothing, ≥16 chars),
   404 otherwise (the established public-surface rule). Content-Type `application/scim+json`:
   `POST` creates an account (invited state, random unguessable password, must reset),
   `GET` lists/filters (`userName eq "…"`), `PATCH { active:false }` = deprovision: sessions
   revoked and memberships removed (the AC's "like a manual departure"); `active:true` re-enables
   the account (sessions are not re-minted). No `Groups` sync. Audit rows: `scim.provisioned`,
   `scim.deprovisioned`.
4. **Observability (REQ-054).** Structured, one-JSON-log-per-event with request id, trace id,
   span id (generated or parsed from `traceparent`; echoed in `x-request-id`); the three run
   lifecycle events with named fields and durations (`run.execution_started`, `run.first_response`,
   `run.finished`); redaction in the log path; `/metrics` Prometheus exposition (hand-rolled,
   no dependency: runs counters, http request counters, lease reclaim counter, metric naming
   documented); `POST /api/csp-report` (public, `application/csp-report` and
   `application/reports+json`) logs and audits (`security.csp_violation`, rate-limited by D-01),
   mirroring the plugin-submission counter. OTLP export stays optional and **not wired** this
   milestone: absence of configuration means off, the native JSON path is the operational truth.
   (Proposal DEC-18.)
5. **Admin surfaces, server half (REQ-058).** `admin.*` RPC: `members.list` (role-gated),
   `members.role.set`, `members.remove`, `grants.list|grant|revoke` (access grants with a
   reason, owner/admin only), `pluginSubmissions.list|approve|reject` (deployment-admin scoped:
   a space Owner/Admin of the deployment operatorspace only, or any space with the flag —
   decided at build time; see calls), `pluginSubmissions.refuse`-reason recorded. Every one
   of these returns 404/403-deny for plain members (REQ-058 AC).
6. **The two screens (REQ-058).** `/admin/access` (members table, role change optimistic with
   rollback, grants with reasons, last-seen) and `/admin/plugins` (queue, refusal reasons shown,
   approve/reject) in `apps/web`, hand-built RPC client extension, without code execution
   anywhere; the submitter sees the state change (email notification stays out — no mail
   infrastructure for outbound, the queue just flips state and records audit).

## Non-goals

- No SCIM Groups; no group push/pull. No bulk operations.
- No OTLP wiring (DEC proposed; absent config = off by definition).
- No admin screens outside the two named routes.
- No deletion retention tiers: the policy is stated and simple — deleted is deleted.
- No email notifications for review decisions (would be a lie; there is no outbound mail).

## Anti-goals

- No admin path reachable by members, at any layer (envelope, role, UI disabled).
- No secret material in exports, logs, audit rows or error messages.
- No silent success: partial imports/exports report what was skipped; refused admin calls say so.

## Constraints

- Tests-first; the API property tests must stay green; CI is the evidence path for the PG legs.
- Zip secure import (entry names validated); artifact bodies are content-addressed, copy with
  their bytes.
- All new public routes keep the uniform-404 rule; `/api/csp-report` is rate-limited (D-01).
- Existing pattern for flags: absent flag ⇒ 404 for humans and machines alike.

## Approach

One commit per REQ slice (mirroring M5–M8), docs + STATE + TRACEABILITY at the end. PG legs
proved against the probe role. The `audit_events.kind` CHECK constraint stops covering
`mail.dropped` and `messaging.private_endpoint` (two latent defects — the app writes both,
the CHECK admits only `mcp.private_call`; the PG paths never wrote them in tests) — fixed in
migration 0045 with a closed, documented set.

## What the builder decides

- Manifest schema version `1`; entries layout `<section>/<id>` under a single root folder.
- Zip writer: `zip` crate (pure Rust, MIT); import cap 32 MiB, entry cap 8 MiB.
- Deletion: order (export-exempt) = Stripe subscription cancel-or-refuse → sessions → audit
  append `account.deleted` → spaces/users cascade → per the cascade, artifacts on disk reclaimed.
- Admin plugin review without an owner role: any Owner/Admin of any space reviews (documented,
  same risk model as board create). Owner can veto.
- `/metrics` renders text/plain; counters only, no histograms this milestone.

## Named gaps (deferred, recorded)

- SCIM Groups and bulk; OTLP (DEC-18); outbound notification of submission outcomes; admin CLI
  diagnostics beyond the existing `internal/health` and the export itself; deletion retention
  tiers; pseudonymisation pipeline beyond `[redacted]`-on-shape.
