# Worker #212 — `crates/domain/src/audit.rs` reliability proposal (v0.4.0)

Lens: reliability. Posture: READ-ONLY analysis; no source changed here.
Evidence (CONFIRMED): read `crates/domain/src/audit.rs` (149 lines), `crates/infra/src/stores/audit.rs`,
`crates/app/src/ports.rs` (2239-2286), the export/list call sites in `crates/app/src/usecases.rs`
(8505-8573) and `crates/api/src/rpc.rs` (3462-3483), `crates/app/src/accounts.rs` (150-174),
`spec/02-data-model.md`, `spec/04-nonfunctional.md`, `spec/05-known-defects.md` and `AGENTS.md`.

## Role of this module
Pure, IO-free domain logic: the closed `AuditKind` enum with stable `as_str()` wire values, a
case-sensitive value denylist `SECRET_TOKENS`, `looks_like_secret`, `REDACTED = "[redacted]"`, and
`redact_audit_value` (recursive walk of strings/arrays/objects). It is the last line of defence
before an audit row reaches `audit.export`, `security.events.list` or an account archive. A
reliability defect here is either a credential that escapes, an event that disappears, or a
redaction pass that silently misses. `spec/04-nonfunctional.md:83-84` requires that logs, audit
rows and exports never contain secret material; the module's own docstring promises archives do
not carry a live credential "even when one was logged".

## Failure modes

### 1. Secret leakage
- FM-1 **CONFIRMED — redaction is value-only and key-blind.** `looks_like_secret` (88-90) scans
  string *values* for a fixed prefix list; it never inspects the key. `{"password": "hunter2"}`,
  `{"authorization": "dXNlcjpwYXNz"}`, a JWT (`eyJ...`), a 40-char AWS secret access key or a
  `postgres://user:pw@host` DSN all pass unchanged. The file admits "Deliberately conservative
  strings" — the gap is by design, so the guarantee is narrower than the docstring implies.
- FM-2 **CONFIRMED — denylist drift.** The list (79-81) names `xoxa-/xoxb-/xoxr-/xoxs-` but not
  `xapp-`/`xoxp-`, `github_pat_`, `glpat-`, `SG.`, `npm_`, `ya29.`; `Bearer ` is capitalised, so a
  lowercase `bearer ` is missed. New provider token shapes leak until someone edits this file.
- FM-3 **CONFIRMED — `target` is never redacted.** `NewAuditEvent.target` is documented as "server
  name, tool, command — never the secret" (`ports.rs:2251`), but both read paths pass it verbatim:
  `audit_export` (`usecases.rs:8561`) and `security_events_list` (`rpc.rs:3477`) redact only
  `metadata`. A token embedded in a command/tool name is exported in clear.
- FM-4 **CONFIRMED — no redaction at rest.** `PostgresAuditStore::append` binds raw `event.metadata`
  and `event.target` (`infra/.../audit.rs:113`). The domain backstop runs only on read, so the
  `audit_events` table and any future forwarding sink hold live material; a DB reader or a new
  consumer bypasses the only safeguard.

### 2. Dropped events
- FM-5 **CONFIRMED — a security event is explicitly discarded.** `accounts.rs:154` does
  `let _ = self.record_sign_in_failure(now).await;`; a DB error on a failed sign-in audit is
  swallowed, so brute-force evidence silently vanishes while the caller still returns
  `InvalidCredentials`.
- FM-6 **CONFIRMED — silent page caps.** `list_for_space` is called with `4_000` (export) and `200`
  (`security.events_list`). The export flags truncation only via `len == 4_000` (`usecases.rs:8542`),
  which falsely flags a space with exactly 4,000 rows; the 200-row security view has **no**
  `truncated` flag at all, so an operator cannot tell that older security events are hidden.
- FM-7 GUESS — no outbox/retry: `append` is a single `INSERT` with no idempotency key beyond the
  generated `id`. A transient DB failure at the write boundary loses the event for good; there is
  no replay path. (No forwarder or outbox was found in this workspace.)

### 3. Redaction / correctness gaps
- FM-8 GUESS — unbounded recursion: `redact_audit_value` has no depth budget. Parsed JSON is capped
  by serde_json's recursion limit, but a `Value` assembled in app code could be deeper and overflow
  the stack (an abort, not a catchable panic) on the hottest read path.
- FM-9 **CONFIRMED — no inverse/canonical check.** `AuditKind` has `as_str()` but no `from_str`; a
  malformed or injected `kind` in the append-only table is exported verbatim, with no canonical
  vocabulary enforced on read. No test asserts wire values are unique, so a future variant could
  silently collide.
- FM-10 **CONFIRMED — non-string secrets pass through.** Numbers/booleans are cloned untouched
  (115); a token encoded as a byte array of numbers is not detected. GUESS: low likelihood today.

## Concrete hardening (with risks)
| # | Change | Risk / cost |
|---|---|---|
| H1 | Make redaction key-aware: redact the *value* when a key matches a sensitive-name set (`token`, `password`, `authorization`, `secret`, `cookie`, …), case-insensitively, in addition to `SECRET_TOKENS`. | Broadens blanking — could empty harmless fields and change golden output; needs a reviewed key list and a DEVIATION check in `docs/PARITY.md`. |
| H2 | Extend `SECRET_TOKENS` with `xapp-`, `github_pat_`, `glpat-`, `SG.`, `npm_`, `ya29.` and lowercase `bearer `; centralise as one documented constant. | Denylist still drifts; pair with H1/H4. Wire output changes only where a secret is present. |
| H3 | Pass `target` through `redact_audit_value` in both read paths (and constrain `target` at write time). | Changes an externally visible field → DEVIATION; golden tests must be updated with approval. |
| H4 | Redact at write time in `AuditStore::append` (or in a domain constructor for `NewAuditEvent`) so the DB never stores raw secrets; keep read redaction as backstop. | Changes stored data and the app port; migration/backfill of existing rows may be needed; ADR. |
| H5 | Stop discarding the sign-in audit: log a `tracing::warn!` (no secret material) when `record_sign_in_failure` fails, or propagate the error. | Propagating changes auth behaviour; warning is safer — but check no credential is logged. |
| H6 | Return a `truncated: bool` from `security_events_list` and compute export truncation as `len >= cap` (or have the store report total). | Adds a DTO field → DEVIATION; `>=` still cannot detect capping exactly at the boundary without a total count. |
| H7 | Add `AuditKind::from_str` + a uniqueness test over `as_str()` wire values; canonicalise on read. | New public API; unknown kinds need an explicit policy (drop vs. preserve) — decide in ADR. |
| H8 | Give `redact_audit_value` a depth budget that substitutes `REDACTED` past the limit. | Adds a parameter/constant; pick the bound from serde_json's limit, not by guess. |
| H9 | Add proptest coverage over random nested `Value`s: no input contains a planted secret after redaction, and output is deterministic. | Test-only; strongest first gate, no behaviour change. |

## Verification (test-first; REQ id in the test name)
- `req_045_redaction_masks_sensitive_keys_not_only_known_prefixes` — FM-1/FM-2.
- `req_045_redaction_masks_lowercase_bearer_and_xapp_tokens` — FM-2.
- `req_045_export_redacts_the_target_field` — FM-3.
- `req_045_sign_in_failure_audit_error_is_warned_not_discarded` — FM-5.
- `req_045_security_events_list_reports_truncation` — FM-6.
- `req_045_audit_kind_wire_values_are_unique_and_round_trip` — FM-7/FM-9.
- `req_045_redaction_is_total_and_deterministic_over_nested_values` — proptest, FM-8/FM-10.

## Open decisions (GUESS where unresolved)
- H1/H3/H4/H6 change external shapes or stored data and are likely DEVIATIONS under `AGENTS.md` —
  record in `docs/PARITY.md` and get owner approval before shipping; H5/H9 are safe first slices.
- The authoritative sensitive-key list and the depth bound must come from spec/ADR, never invented.
- Decide whether redaction at rest (H4) is in v0.4.0 scope or a later slice with a backfill ADR.
