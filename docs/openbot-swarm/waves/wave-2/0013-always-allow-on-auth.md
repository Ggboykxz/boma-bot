# 0013 — always-allow-on × auth (parity proposal, Boma Bot)

- Brief: id 13, area `always-allow-on`, lens `auth` (`briefs-1000.jsonl`).
- Status: **PROPOSAL only** — no product code, spec, golden or catalogue was changed.
- Provenance: upstream consulted as **data** (HEAD `386085ff`, 2026-11-18) in `CHANGELOG.md` and
  `docs/architecture/{structure,computer-use,agents}.md`. No upstream code, asset, wording, icon,
  colour, font, product name or brand enters Boma Bot; no Pro component, no second design system.
  Claims are CONFIRMED from files read this session unless marked GUESS.

## 1. Upstream capability, in my own words (inspiration only)

A standing approval exemption lets one decision be remembered for a whole class of tool calls:
after it is set, later calls in that class proceed without raising a prompt. Restated generically:

- The exemption is offered only in a middle permission tier: the most-open tier never shows it
  (the host answers its own prompts) and the most-closed tier keeps prompting. Where persisted,
  it is scoped to a tool and one provider account, in a user-owned config file on the agent's
  machine — the host stores no consent itself.
- Turning the grant on, or an administrator raising access, also closes prompts already waiting
  for that agent. An untrusted import downgrades an existing grant to a warning so the safer
  default wins; changing a grant also discards a cached provider session that still carries it.
- On a remote host with automation enabled the host can answer its own prompts, so no card reaches
  a client, and a client cannot grant consent on that remote host's behalf.

**Permissive defaults stay out.** We do not adopt: a host silently answering prompts so no card
reaches a client; automatic promotion to a fully-open tier from one click; "never ask" as any
default; an import silently keeping an elevated grant; or the host storing/sharing consent on a
person's behalf. In Boma Bot an exemption is an explicit, reversible server-side policy.

## 2. Authorization parity — REQ-066 principal and REQ-017 gate

`spec/01-requirements.md` REQ-066 (SHOULD): in local-first mode a sessionless request resolves as
the machine's local owner, a session cookie is honoured strictly, and the mode is refused on a
non-loopback bind ("never a silent weakening"). REQ-017 (MUST): a policy per space and per user
decides which tool calls need approval, and an approved effect runs once even if retried.

A standing exemption is **one value of a policy we already own**, not a new vocabulary or a client
switch. The two REQs constrain it as follows:

1. **Subject is the resolved principal, never the request.** Only the identity `authenticate()`
   resolves can hold or use an exemption — the local owner for a sessionless loopback request, or
   the strict session-cookie user otherwise (`crates/api/src/auth.rs`, `ensure_local_owner` in
   `crates/app/src/usecases.rs`). An unresolved cookie stays unauthorized; it never falls back to
   the local owner's exemptions.
2. **Scope is per space and per user (REQ-017).** A grant is keyed to `(space_id, user_id, tool
   key)`; row-level tenancy keeps a grant in one space from answering a call in another.
3. **Exactly-once still holds (REQ-017).** Skipping the card never skips the effect key: an
   exempted write/spend runs once under the same idempotency record as an approved call, so replay
   cannot double-send or double-charge.
4. **No silent weakening (REQ-066).** No exemption is seeded by default; a deployment-wide
   "skip approvals" setting on a non-loopback bind is a boot error, never a permissive default.
5. **No standing consent from a lower-trust source.** An untrusted import or member action fails
   closed — it cannot create, widen or carry an exemption; only the space's owner may set one.

## 3. Server-side policy checks (the auth-lens core)

The rejection of an untrusted client is this lens's point: **the no-card decision is made on the
server, never asserted by the caller.**

- Today the gate is already server-side. `crates/app/src/worker.rs` classifies each tool call by
  `ToolEffect` from the catalogue, scoped to `TenantContext::Member { space_id, user_id }`:
  `Read` dispatches, `Write`/`Spend` parks a card (`park_tool_calls`), unknown tools are refused;
  a missing effect class defaults to `Write` (fail closed). An exemption plugs in only here.
- The worker will consult a server-side grant lookup at classification time, keyed to the run's
  `(space_id, user_id)` and tool key/effect class, before parking. No request field, UI flag or
  client claim can skip it; absent or ambiguous data = park a card (conservative default).
- The grant uses the same authority context as the tool gate, so it cannot answer a call in a
  different space or for a different user, nor serve a session mapped to another principal in a
  hosted (non-local-first) deployment. Effects still flow through `apply_pending_tool_calls` and
  the effect key (REQ-017); the exemption changes only whether a card is created.
- GUESS (per AGENTS.md): no approval-policy write RPC exists today — only `notifications.*` and
  `mail.*` policy RPCs are in `crates/api/src/rpc.rs`. An owner-only policy write is a later slice
  with its own REQ id and ADR; do not invent one here.

## 4. One REQ-named test

In `crates/testkit/src/build.rs` add exactly one test, distinct from the existing
`req_017_a_write_tool_waits_for_approval` and `req_017_an_approved_tool_effect_runs_exactly_once`:

```rust
async fn req_017_a_standing_exemption_is_server_scoped_skips_the_card_and_runs_once() {
  // owner stores a server-side grant for (space A, user U, tool "send") via the internal
  // policy port; no client flag is involved.
  // 1) U in space A calls "send" (Write): no card, effect applied once, one effect-key record.
  // 2) Same call replayed: no second effect (REQ-017 idempotency holds without a card).
  // 3) Same tool for user V, or in space B: grant does not match -> a card IS parked
  //    and no effect runs (per-space/per-user scoping, no cross-tenant exemption).
  // 4) A client payload that asserts an exemption but has no stored grant: card IS parked
  //    (the decision is server-side; a caller cannot grant itself permission).
  // 5) A session cookie that does not resolve stays unauthorized even when the
  //    local owner holds a grant (REQ-066 strict-cookie case).
}
```

It pins the auth contract: server-side decision, principal/space scoping, no card under a valid
grant, no effect without one, exactly-once. Pair it with a config test asserting a deployment-wide
exemption on a non-loopback bind is a boot error (REQ-066), kept separate rather than folded in.

## 5. First slice (reversible, test-first, small)

1. Write the failing REQ-017 test above.
2. Add an internal policy port + store lookup keyed to `(space_id, user_id, tool key)`; no new
   wire field, RPC or UI control — the grant is seeded only through the test port in this slice.
3. In the `worker.rs` classification step only, consult the lookup before parking a `Write`/`Spend`
   call; keep `Read` dispatch, unknown-tool refusal and the `Write` default untouched.
4. Leave the effect path, effect key, card lifecycle and `threads.answer` guard as they are.
5. **Do not** add an owner-facing policy RPC, an exemption card/control, promotion to a broader
   tier, "close already-waiting cards", or import carry-over here — those change permission
   semantics and need a new REQ id, an ADR and owner approval (AGENTS.md). Record them in
   `docs/STATE.md` under "Decisions needed" as later slices.
6. Gates: `cargo clippy -- -D warnings`, `cargo test -p boma-testkit`, REQ-017/REQ-066 golden and
   `rpc_surface` clusters, `tools/trace_check.py`; update `docs/STATE.md` (CONFIRMED/GUESS) and
   `docs/TRACEABILITY.md`; no dependency change.

## Guardrails

- No upstream code, asset, wording or brand; inspiration only. No Pro. One design system. No
  permissive default, no client-side authority, no cross-space/tenant exemption.
- This is a proposal only: no build or test was run and no product file changed.
