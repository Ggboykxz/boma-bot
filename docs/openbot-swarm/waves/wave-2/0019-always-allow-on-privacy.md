# 0019 — always-allow-on × privacy (parity proposal, Boma Bot)

- Brief: id 19, area `always-allow-on`, lens `privacy` (`docs/openbot-swarm/briefs-1000.jsonl`).
- Status: **PROPOSAL only.** Upstream was read as data (HEAD `386085ff`, 2026-11-18); no product
  code, spec, golden, catalogue, asset, wiring or brand changed. No BoardUI Pro, one design system.
  Claims are **CONFIRMED** (read here this session) or **GUESS** (needs the owner's decision).

## 1. Upstream capability, in my own words (inspiration only)

CONFIRMED (read only, `/tmp/opencode/openbot-parity/docs/architecture/{structure,computer-use,agents}.md`
and `CHANGELOG.md`): some desktop agent hosts let a person turn a single approval into a **standing
permission** for a class of action, so later actions of that class proceed without a new prompt:

- The host itself **keeps no record**; the choice lives in the person's own local config, keyed per
  tool and provider account, and survives a restart. Raising access or granting the standing
  permission also resolves approval prompts **already waiting** for that agent.
- A less-trusted import **downgrades** a remembered standing permission to a warning, so a fresh
  agent never starts elevated; changing the permission discards a cached session with the old one.
- Secret-bearing values (driver credentials, environment, socket details) are **not written** into
  the saved entry; the approval view masks known credentials, and the outbound HTTP boundary redacts
  values. Sign-in, interactive forms and screen takeover always stay manual.

The pattern worth carrying: the decision is **local and person-owned**, the host retains nothing,
secrets are excluded from the grant and its presentation, and a state change is honest. We adopt none
of the permissive framing ("never ask" by default, promotion from one card click, silent import).

## 2. Privacy parity is grounded in local-first (CONFIRMED)

REQ-066 (`spec/01-requirements.md:524`) makes the operator's machine the only trust anchor: the
reader is the machine owner, the boundary is loopback, no sign-in round trip. That fixes the scope:

1. **Local, not remote or machine-wide.** A standing permission is one **value of the policy we own**
   — per space and per user (REQ-017, `:137`), optionally per tool class — held by the local server.
   Never a launcher flag, desktop-wide switch, wire field or remote-origin setting; `stack.ts` boots
   `BOMA_LOCAL_FIRST=true` on loopback and `main.ts` writes no grant.
2. **No inherited grant.** Import downgrades a remembered standing permission to a warning needing
   fresh confirmation, so a fresh local agent never starts elevated (same rule as §1, our wording).
3. **Superseded, not fabricated.** A policy flip while a card waits closes it as *"superseded by
   policy"* with no content-bearing notice — never retroactively minted as "Approved" (REQ-017).
4. **Exceptions stay interactive.** Secrets, sign-in, takeover and forms are never absorbed;
   exactly-once effects and tenancy stay authoritative.

**GUESS:** no approval-policy RPC exists today (only `notifications.*` / `mail.policyGet|Set`,
`crates/api/src/rpc.rs:4319`); where the value is stored is an owner decision.

## 3. Secrets are never logged (CONFIRMED)

REQ-046 (`:373`) requires a secret be available to the tool but "not returned in any message or
log"; REQ-053 (`:435`) requires readiness to report capabilities "without leaking secrets".
`crates/domain/src/audit.rs:74-147` (`SECRET_TOKENS`, `looks_like_secret`, `REDACTED`,
`redact_audit_value`) is the existing backstop. Privacy parity pins:

- A grant record holds only a **stable effect class**, actor, space, scope and timestamp — **no
  arguments, placeholders, credentials or conversation text**. Redaction is a backstop, not license
  to log inputs.
- Enabling always-allow **widens no log surface**: with no card the effect still runs once under the
  unchanged effect key (REQ-017/018); any secret stays out of logs, audit rows, pushes and the UI.
  No secret-shaped token may appear in a grant, audit row or notification-log entry.

## 4. Content-free notifications under a standing permission (REQ-064, CONFIRMED)

REQ-064 (`:512`) allows only content-free pushes; `crates/app/src/push.rs:5,266-284` pins the payload
to exactly `{v,kind,title,tag,url}` with a fixed title catalog (`:127`); REQ-065 (`:610`) sends the
same text to destinations, "never tool names, never message text, never user names"
(`docs/STATE.md:630-631`). With always-allow on a call raises no card, so the **"parks waiting on a
person" trigger does not fire** for that class — completion/failure pushes still fire and **stay
five-key and content-free**: the standing permission must **never enrich** a push (no thread name, tool name,
arguments, secret, user name or effect payload — only the opaque `run:` tag and fixed title); policy, daily
cap and dead-subscription deletion are unchanged; one fan-out path serves both sinks.

## 5. One REQ-named Playwright test

New `apps/web/tests/always-allow-privacy.spec.ts` (`seedRpc` from `fixtures.ts`), distinct from
`notifications-panel.spec.ts` (REQ-065) and `approval-card.spec.ts` (REQ-007/017):

```ts
test("REQ-064 an always-allowed run sends a five-key content-free push and logs no secret", async ({ page }) => {
  const pushes: unknown[] = [];
  await seedAlwaysAllowedPolicy(page, "bot_golden", "tool.write"); // GUESS: local policy seed
  await capturePushes(page, pushes);                               // GUESS: test push hook
  await seedRunCompleting(page, { secretArg: "sk-live-SECRET-VALUE" });
  await page.goto("/app/bot_golden/");
  expect(pushes).toHaveLength(1);                                  // completed, no waiting push
  expect(Object.keys(pushes[0] as object).sort()).toEqual(["kind", "tag", "title", "url", "v"]);
  const raw = JSON.stringify(pushes[0]);
  for (const x of ["sk-live-SECRET-VALUE", "thread", "tool", "args", "Alice"]) expect(raw).not.toContain(x); // no secret
  expect(await page.locator("body").innerText()).not.toContain("sk-live-SECRET-VALUE");
});
```

Pins the lens: one content-free push, no leaked secret, no rendered content. `REQ-064` in the name is counted by `tools/trace_check.py`.

## 6. First slice (test-first, smallest safe step)

1. Write the failing REQ-064 test above; prove the content-free invariant against the local stack.
2. **No product-code change:** add only a contract note in `docs/STATE.md` (M16 dependency) and
   `docs/UI.md` — a standing permission is a local per-space/per-user policy value, stores no
   arguments or secrets, never enriches a push. The runtime already emits five keys, so no
   `docs/PARITY.md` DEVIATION is needed (no externally visible change).
3. **Do not** add a policy-write RPC, always-allow control, import change, or alter REQ-017
   evaluation — those need a new REQ id, an ADR and owner approval (AGENTS.md:63).
4. Gates: web typecheck/lint/build, Playwright + axe (zero critical/serious, light/dark, RTL,
   360 px), `tools/trace_check.py`; no dependency change.

## 7. Non-goals / guardrails

No upstream code, asset, wording, identifier, colour, font or brand; upstream is an oracle, never a
source to copy (AGENTS.md:70). No Pro, no second design system, no remote/machine-wide grant, no
"never ask" default, no logged secret, no content-bearing push, no silent gate change (AGENTS.md:27).
Any disclosure UI uses BoardUI semantic tokens only (`docs/UI.md`), never palette/`dark:` literals.

## 8. Evidence

CONFIRMED — `spec/01-requirements.md` REQ-017/018/046/053/057/064/065/066; `crates/domain/src/audit.rs:74-147`;
`crates/app/src/push.rs:5,127,266-284`; `crates/api/src/rpc.rs:4319`; `apps/desktop/src/{stack,main}.ts`;
`apps/web/tests/notifications-panel.spec.ts`, `fixtures.ts`; `docs/STATE.md:606,630-631`; `docs/UI.md`;
`AGENTS.md:27/63/70`. Upstream data only — `docs/architecture/{structure,computer-use,agents}.md`, `CHANGELOG.md`
(person-owned config, per-tool/account, waiting-card resolution, import downgrade, excluded credentials, redaction).
GUESS — policy storage, disclosure copy, test capture hooks.
