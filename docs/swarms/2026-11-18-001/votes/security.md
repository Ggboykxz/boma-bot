---
swarm: 2026-11-18-001
worker: security
weight: 1.3
choice: A
confidence: 0.85
---

# Security vote — A (contextual approval card, REQ-007/017)

## Rationale

The approval card *is* the security control. REQ-017 says no effect happens until a member approves: one click stands between a tool call and a real side effect (payment, send, privileged tool). But `CardBlock` (`message-blocks.tsx`) shows only generic copy — "Card", "An action is blocked until you decide" — bare Approve/Deny, and a transient "Approved"/"Denied" label. It names neither effect, reason, reversal, nor authorizer: the operator authorizes blind. This is a consent failure, not cosmetic.

All three security-relevant sector proposals name this surface: 007 retail-banking ("a plain-language consequence and a durable record"), 058 cybersecurity ("read the exact effect and actor", "no secret echoed back"), 015 compliance ("every privileged act must be attributable", name "action, consequence and authorizer"). SYNTHESIS.md ranks it priority 1 (72 sectors); it is the only slice with a documented security-relevant defect chain: STATE.md records the answered state is not on the messages wire, so reload shows it pending and a second answer returns 409 — a stale live gate for a second actor, and a broken evidence trail.

B (board/triage) and C (locale-count) matter — REQ-057 is a gate, issue #16 real — but neither sits on the authorization path. A mis-styled row or missing locale never authorizes a side effect. Fix the gate first.

## Concrete risks

- **Secret leakage.** Effect summaries carry tool arguments (URLs, tokens, commands, amounts). Render as inert text, never HTML; show key status, never the value (REQ-046 rule); expose nothing to the accessibility tree that the visual UI masks.
- **Action attribution.** Bind the answer to the named member and card/effect key; the answered state must survive reload, not live only in `answerGiven`. Reject the second answer and show the first (REQ-007).
- **Audit.** The confirmation must be durable and replayable from the transcript/golden, not just an `.animate-check-draw` animation, so export (REQ-044/045) can cite who approved, what and when.

## First REQ-named test (Playwright, test-first)

`test("REQ-017 a consequential card names its effect, reason and authorizer and records one attributable answer", ...)` — seed a pending high-impact card with an inert payload; assert effect/reason/reversal and authorizer in accessible names, no key echoed, high-impact marker as icon+word not colour; approve once → attributable confirmation, second answer rejected. Extend the `REQ-007-pending-card` golden with a seeded answered state, never a click path. Charter-safe: BoardUI tokens, `cx()`, no upstream assets, self-host only.
