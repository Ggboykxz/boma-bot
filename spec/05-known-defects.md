# 05 — Known defects

Defects observed in the reference implementation, with the requirement each one weakens and the
obligation a rebuild inherits. Source: `notes/validation-audit.md` (§2). All are **CONFIRMED** from
code and **not reproduced at runtime** (`runtime: no`).

Severity uses the audit scale: critical = remote compromise without auth; high = cross-tenant or
privilege escalation; medium = real abuse or data exposure under a plausible configuration;
low = hardening.

A rebuild does not have to reproduce a defect. Where the reference is wrong, the rebuild must instead
satisfy the obligation in the last column and prove it with a test.

| # | Defect | Severity | Weakens | Rebuild obligation |
|---|---|---|---|---|
| D-01 | Anonymous rate limits live in process memory | medium | REQ-053 (abuse resistance), 04.3 | Counters shared across replicas (database or cache), with a documented reset semantic and a test that two instances enforce one limit |
| D-02 | `GET /api/dev/emails` exposes every captured mail, tokens included | medium | REQ-046, REQ-054 | No such route in the rebuilt API; test asserts the route set contains no development mail reader |
| D-03 | Attachment type is trusted from the client, no magic-byte check | medium | REQ-036 | Sniff content on upload; store the sniffed type; a test uploads HTML declared as PNG and expects rejection or a corrected type |
| D-04 | Trusted proxies default to every local network inside a container | medium | REQ-043, REQ-054 | Default-deny forwarded headers unless `TRUSTED_PROXIES` is set; a test from a non-listed peer must not change the resolved client address |
| D-05 | MCP can reach private endpoints through a deployment-wide flag | medium (low by default) | REQ-033 | Scope the allowance per space, require an explicit allow-list, emit an audit event each time it is used |
| D-06 | Unauthenticated cross-origin POST to plugin submissions | low | REQ-034 | Require an account or a challenge, and use the shared counter from D-01 |
| D-07 | 16 MiB cap is global to every RPC procedure | low | 04.3 | Per-procedure caps; upload procedures opt into the large body, everything else stays small |
| D-08 | Auth endpoints are delegated wholesale to a library with no additional schema | low, unresolved | REQ-040, REQ-041 | Assert the auth configuration in a test (session lifetime, cookie attributes, linking policy) instead of inheriting defaults silently |
| D-09 | `runAsSystem` is a free-text escape hatch with no static gate | low (process risk) | REQ-043 | Allow-list of reasons, a lint/CI check on new call sites, and a test that a system path cannot be entered with a user-supplied id |

## Notes on scope

- **D-03** is mitigated in the reference by CSP and a sandboxed viewer, so it is not exploitable
  through the web app today. It becomes exploitable the moment a consumer trusts the stored type
  (export, mail attachment, connector). The rebuild closes it at upload.
- **D-04** and **D-01** compound: spoofable client addresses defeat per-IP throttles. Fixing one
  without the other leaves the abuse path open, so both obligations are MUST for the rebuild even
  though the reference rates them medium.
- **D-08** is an absence of evidence rather than a known flaw. The rebuild must produce the evidence.
- The audit's negative results (no SQL injection reachable, no command injection, no path traversal,
  no link or webhook enumeration) are properties to keep, not defects. They are asserted by
  REQ-043 and 04.5.

## Defects deliberately not fixed by the rebuild

- **Brand and asset licensing** is out of scope (see `00-overview.md`, non-goals); nothing from the
  reference's visual identity is imported.
- **The legacy Graphile dispatch path** exists only for rolling upgrades in the reference. The rebuild
  ships one dispatch mechanism and therefore cannot reproduce its known duplicate-delivery window.
