# 0472 — `migrations/0029_mcp_servers.sql` — reliability proposal (v0.4.0)

Worker #472 · lens: reliability · READ-ONLY review of the `before-m5` tree. Proposals only; no code
changed. **Additive only** — every item below is a new statement in a new migration, never an edit to
0029. **CONFIRMED** = read in this tree this session; **GUESS** = inference needing a runtime test.

Anchors: `0029:8-23` (table), `:25-29`/`:40-56` (RLS), `:32-38` (session); `0031_mcp_credentials.sql:6-13`
(sealed columns); `0002_m1.sql:26-42` (helpers); `crates/infra/src/stores/mcp.rs:261-294,401-506,542-579`;
`spec/02-data-model.md:135-136,157-158`; `spec/01-requirements.md:271-277` (REQ-033).

## Scope note

0029 creates `mcp_servers` (space-scoped registry, transport-shape CHECK, unique name, FORCE RLS) and
`mcp_oauth_sessions` (one pending PKCE attempt per server, verifier sealed, FORCE RLS via correlated
EXISTS on the parent). Sealed credentials arrive in 0031 and are read only via `reveal_*`
(`mcp.rs:261-294,508-540`). All fixes below are later migrations; the file itself is untouched.

## What is already right (keep)

- FORCE RLS + closed-form policies fail closed: no `boma.space_id` ⇒ NULL ⇒ false; only an explicit
  `boma.system_reason` crosses tenants (`0002_m1.sql:26-42`). CONFIRMED.
- Transport CHECK (`:21-22`) makes url/command exclusive; `auth_state`/`transport` enums and
  `allow_private NOT NULL DEFAULT false` are sound. CONFIRMED.
- `ON DELETE CASCADE` FKs (`:10,33`) prevent orphans; `UNIQUE(space_id,name)` also indexes tenant scans.
- PKCE verifier is sealed and single-use via `DELETE ... RETURNING` (`mcp.rs:472-502`). CONFIRMED.

## Failure modes

F1 — **Secret shape (CONFIRMED).** `sealed_verifier`/`_nonce` are only NOT NULL (`:35-36`); the 0031
`bearer_token`/`sealed_client_secret` are each independently nullable. `bytea` accepts zero-length
values, so a truncated write stores "successfully" and fails only at `open_for_space`
(`mcp.rs:283,495,529`) as a 503. A half-written pair (ciphertext set, nonce NULL) makes `reveal_bearer`
return `None` (`:280-282`): the server shows `connected` yet is silently unusable. No CHECK pairs the
columns or enforces GCM sizes.

F2 — **Credential/state mismatch (CONFIRMED).** Nothing stops a bearer token on a stdio server, a client
secret on a plain URL server, or `auth_state='connected'` with no usable token.

F3 — **Mutable tenant breaks key locality (CONFIRMED, highest impact).** The verifier is sealed under
the DEK read from the *server* at write time (`mcp.rs:425,562`) and opened under the DEK read from the
server at use time (`:464-468,553-557`). `mcp_servers.space_id` has no immutability guard: a system
reassignment flips the DEK ⇒ decryption 503, or cross-space exposure if keys were ever shared. The
session has no `space_id`, so it inherits this drift entirely.

F4 — **RLS hot path / over-broad (CONFIRMED).** Each session row evaluates two correlated EXISTS
subqueries over RLS-filtered `mcp_servers` (`:43-55`); under `is_system()` the EXISTS is true for any
server, so a mis-scoped system write can attach a session cross-space. Any space member has full DML —
the app's `id`/`server_id` predicates are the only real boundary. No `boma.current_user_id()` helper
exists yet for a per-actor policy.

F5 — **GUC trust (CONFIRMED, cross-cutting).** `boma.current_space_id()` reads a custom GUC any SQL role
can `SET`; isolation rests on the app role being the only DB principal. A SQL injection issuing `SET
boma.space_id` would defeat RLS on both tables — fix at the connection/role layer.

F6 — **Litter (CONFIRMED).** Expired sessions are deleted only when a matching `take_oauth_session`
runs (`mcp.rs:483-493`); abandoned flows leave old verifiers forever. No expiry index or sweep.

F7 — **Weak value domains (CONFIRMED).** `id`, `name`, `state`, `url`, `command` accept empty/oversized
strings; `tool_count` accepts negatives shown in listings (`:17`); `expires_at` accepts past/far-future
values (`:37`) making a session permanently single-use or effectively immortal; `updated_at` has a
DEFAULT but no bump trigger.

F8 — **Mechanics / races (CONFIRMED low / GUESS).** No `IF NOT EXISTS`, so a replay fails on CREATE
TABLE. Editing 0029 in place changes its SQLx checksum and breaks startup on deployed DBs — the reason
all fixes are additive. `store_oauth_session`'s `ON CONFLICT DO UPDATE` has no version guard
(`mcp.rs:426-433`); two concurrent flows last-write-wins (needs a concurrency test to confirm impact).

## Concrete hardening — additive, in one new migration (e.g. `0060_mcp_servers_hardening.sql`)

P0 (F1) — `ADD CONSTRAINT` checks: each sealed pair is both-null-or-both-non-null, and when present
`length(nonce)=12`, `length(ciphertext)>=16` (AES-256-GCM), plus `length(state)>0`. **Risk:** existing
short/partial blobs fail the migration — run a read-only audit query first and clean bad rows.

P0 (F3) — `BEFORE UPDATE` trigger on `mcp_servers` rejecting any change to `space_id` (or allowing it
only via a documented rekey flow that re-seals all blobs). **Risk:** blocks a legitimate system
move-to-space; provide the rekey path so it is deliberate, not worked around in raw SQL.

P1 (F2/F7) — CHECKs for transport-consistent credentials (no bearer/secret on stdio; `connected`
requires a bearer or OAuth token URL), `tool_count >= 0`, length bounds and `^https?://` URL / non-empty
command shapes. **Risk:** callers storing non-HTTP private URLs would be rejected — verify none do
(GUESS); bound generously.

P1 (F4) — Add `space_id text NOT NULL` to sessions + a composite FK `(server_id,space_id) →
mcp_servers(id,space_id)` (add a UNIQUE on that pair), replacing the correlated EXISTS with
`space_id = boma.current_space_id() OR boma.is_system()`. Add a `RESTRICTIVE` per-actor policy once
`boma.current_user_id()` exists. **Risk:** backfill from the parent; every session write path
(`mcp.rs:426,485`) must pass the matching `space_id` in the same release.

P1 (F6/F7) — `CHECK (expires_at > now())` is impossible (volatile), so use a `BEFORE INSERT` trigger
bounding `expires_at` to a sane window, `CREATE INDEX ... ON mcp_oauth_sessions(expires_at)`, and a
batched, idempotent system-context sweep of expired rows. **Risk:** batch to avoid bloat; run under
`boma.is_system()`; confirm no product meaning attaches to old sessions.

P2 (F8) — Document the downgrade (`DROP CONSTRAINT/POLICY/TRIGGER/INDEX/COLUMN`) per spec 04 §4.9; add
a CI gate forbidding edits to applied migrations; add a concurrency test before considering a version
column. Tests (testcontainers, REQ-033 in the name): truncated blob rejected at write; half-written
credential rejected; space reassignment blocked; cross-space session blocked under a member role;
bad expiry rejected; expired row swept; cascade on server delete. Wire shape/goldens unchanged.

## Risks and trade-offs

- Blob-shape CHECKs are the highest-value fix but the only one that can reject *existing* data — ship
  the audit first. All items are independently revertable; no gate, lockfile or applied checksum
  changes. Extra policies/triggers add negligible overhead on a low-volume registry.
- A `space_id` column + composite FK changes the table shape but no API/DTO; it touches all session
  write paths, so coordinate them in one release.

## Open questions (route to docs/STATE.md)

- Do deployed rows already carry partial/short sealed blobs (F1)? Need a read-only probe.
- Is server `space_id` ever legitimately reassigned (F3), and who owns the rekey flow?
- Does `boma.current_user_id()` land in v0.4.0, or is per-actor RLS deferred?
- Confirm no caller relies on non-HTTP URLs or negative `tool_count` before P1 (GUESS).

Status: findings CONFIRMED by source reading except F9 impact and GUESS caller assumptions; no
runtime/DB/test command was run (READ-ONLY). No file other than this proposal was written.
