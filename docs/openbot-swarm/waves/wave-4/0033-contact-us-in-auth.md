# 0033 — contact-us-in × auth (parity proposal, Boma Bot)

- Brief: id 33, area `contact-us-in`, lens `auth` (`docs/openbot-swarm/briefs-1000.jsonl`).
- Status: **PROPOSAL only** — no product code, spec, golden or catalogue was changed.
- Provenance: upstream read as **DATA ONLY** at HEAD `559d5acd` (2026-11-19): `docs/architecture/team-api.md`
  (phone-feedback/feedback-withdraw sections), `docs/architecture/channels-and-messaging.md` (capability
  table), the agent tool-router (report/withdraw flows), the approval attention registry (answer rules),
  the phone-feedback team-API route, the feedback contracts module, the mobile support screen. No upstream code, asset, wording, address or
  brand enters Boma Bot; no Pro; one design system. CONFIRMED = read/ran this session; GUESS = owner's call.

## 1. Upstream capability, in my own words (data only)

- A person inside the product can reach the operator: a secondary Support surface, plus an agent-mediated
  flow where a bug report or feature idea is composed and sent to the vendor's issue tracker.
- The flow starts only from a person's own message in their own chat or a thread they opened. The agent
  never reports unaided — an agent-found issue still waits for a person's go-ahead (and an untrusted
  channel/remote-member message never triggers it).
- Nothing is sent before an approval card, and that card is **owner-only**: a non-owner answering it is
  refused and the card keeps waiting. Withdrawing a sent report is owner-only approval too.
- Phone-surface bug reports carry the phone's own version, system, network class and redacted log;
  a plain "accept" without that device data is refused. The device, not the desktop, answers.
- If the product cannot send (signed out, account-side limit), the person gets a prefilled public
  issue-form link to submit themselves. There is no quiet send path.
- Enforcement sits in the server's approval-answer/route layer (capability advertised, agent visible,
  `chat` right, owner role, request still waiting) — never in a client flag; the client only renders.
- Generic principle: the reporting surface holds the evidence; the owner decides to release it; the
  server re-checks authorization at answer time against live state (role, tenancy, staleness).

## 2. Where Boma Bot stands (CONFIRMED this session)

- `crates/api/src/auth.rs::authenticate` — a session cookie is honoured **strictly** (unresolved → 401);
  a sessionless loopback request resolves as the local owner only in `local_first` mode (REQ-066). The boot
  refuses local-first on a non-loopback bind (`crates/api/src/config.rs`; `req_066_local_first_on_a_non_loopback_bind_is_a_boot_error`).
- `Actor { user, context: TenantContext::Member { space_id, user_id } }` — no role in context, but
  `SpaceStore::membership(space_id, user_id) -> Option<SpaceRole>` exists; `SpaceRole::Owner > Admin > Member`.
  The local owner's membership row is written with role `owner` (`crates/infra/src/stores/accounts.rs`).
- `usecases::answer_card` — any authenticated tenant member may answer a Pending card today; a replay gets
  `Conflict`; an Approval card answered `approve` records effect key `approve:{card_id}` exactly once (REQ-017).
  There is **no owner-only card concept** yet.
- The RPC surface has no contact/support/feedback procedure; `spec/openapi.yaml` advertises sign-in
  capabilities (methods, passkeys, `localFirst`) and billing only; the sidebar `Support` entry is a static `NavItem` with no destination
  (`apps/web/components/application/dashboard/dashboard-sidebar.tsx`) — a hook, not a channel.
- Tenant isolation at the data layer holds (REQ-043, RLS; cross-tenant ids → not-found, never forbidden).

## 3. Authorization parity — REQ-066 principal, REQ-017 gate

1. **Subject is the resolved principal, never the request.** Only the identity `authenticate()` resolves
   may answer a contact-report card: the local owner (loopback bring-up) or the strict session-cookie user
   otherwise. An unresolved cookie stays unauthorized even if the local owner could answer (REQ-066).
2. **The send is an external side effect → the REQ-017 gate.** It parks an Approval card; the client never
   asserts consent; the effect key guarantees exactly once, so a replay or double-answer never double-sends.
3. **Owner-only, enforced server-side.** The contact-report card carries a server-internal owner policy:
   answering requires a live `membership(space_id, user_id) == Owner`. In local-first mode the single
   principal holds `owner`, so the rule holds by construction; in hosted mode a Member/Admin answer is
   refused (403-class), the card stays Pending, nothing is sent.
4. **Trigger provenance fails closed.** Only a person's own message in their own thread may start the flow;
   channel fronts (REQ-070), mail routines and system paths cannot trigger it. Owner role at answer time
   is re-read from the store, never cached from connect time.
5. **No client claim grants consent.** No wire field in this slice; a client-asserted "approved" is ignored;
   unknown space → not-found, wrong tenant → not-found (REQ-042/043), never silently upgraded.

## 4. Server-side policy checks (the auth-lens core)

- Enforcement point: `answer_card`, after the tenant-scoped card lookup and the Pending check, **before**
  recording the effect key: read `SpaceStore::membership`; if the card is owner-policy and the caller is not
  Owner → refuse, no mutation. If membership is absent/ambiguous → refuse. The decision is made once, on the
  server, at answer time.
- Parking point: the future contact/report tool (its own slice) parks through `park_tool_calls` with the
  same owner policy; the worker never dispatches the external send itself.
- The check shares the tool gate's `TenantContext::Member` scope, so it cannot answer a card of another
  space or act for another principal (REQ-043).
- Degraded paths (GUESS, later slices): signed-out or account-limited send falls back to a prefilled
  external form link the person submits; rate limits live in the deployment's account service, not the UI.
- Surface-bound evidence: any telemetry a report may need is gathered on the environment that owns it; a
  client can never attach or assert it (phone section stays a proposal until a phone surface exists).

## 5. One REQ-named test

In `crates/testkit/src/build.rs`, distinct from the existing `req_017_*` tests:

```rust
async fn req_017_a_contact_report_card_is_server_owner_gated_and_runs_exactly_once() {
  // space A: owner O, member M; space B: owner P; seeded via the internal policy port, no wire field.
  // 1) a person's message in O's chat parks the report card (owner policy, server-internal);
  //    M answers "approve" -> refused (403-class), card stays Pending, no effect key recorded.
  // 2) O answers "approve" -> one effect key `approve:{card_id}`, one external send; replay -> Conflict.
  // 3) P acting in space B answers the same card id -> not found (REQ-043); card still Pending for A.
  // 4) sessionless loopback local-first resolves as the local owner ('owner' row) and may answer;
  //    an unresolvable cookie -> unauthorized even then (REQ-066 strict case).
}
```

REQ-066's boot-refusal and capability-report halves are already pinned (`req_066_*` in
`crates/api/src/config.rs`, `authcap.rs`); this test pins the answer-time gate.

## 6. First slice (test-first, small)

1. Write the failing REQ-017 test above (seeded spaces + membership roles).
2. Add a server-internal owner policy on the card (test port seeds it; no wire field, no migration, no UI).
3. In `answer_card` only: after the Pending check, if the card carries the owner policy, require a live
   `membership == Owner`; refuse otherwise. Effect key, `Conflict` replay and approval kind untouched.
4. Defer: Support `NavItem` destination, the contact transport/ticket backend, device-evidence binding,
   rate-limit/form fallback, an audit `AuditKind`, and any wire/capability field — each needs its own REQ id
   + ADR; record in `docs/STATE.md` under "Decisions needed". Owner/Admin answer scope is GUESS (owner's call).
5. Gates: `cargo clippy -- -D warnings`, `cargo test -p boma-testkit`, REQ-017/REQ-066 clusters,
   `tools/trace_check.py`; update `docs/STATE.md` (CONFIRMED/GUESS) and `docs/TRACEABILITY.md`; no dependency
   change, no golden change, no DEVIATION expected.

## Guardrails

No upstream code, asset, wording, address or brand; no Pro; one design system. No wire change, migration or
golden-data change in this slice. No client-side authority, no permissive default, no standing consent, no
auto-answered card in local-first mode. Proposal only: nothing here authorises a build.
