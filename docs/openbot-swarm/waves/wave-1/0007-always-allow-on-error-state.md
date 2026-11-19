# 0007 — "Always allow on": error state and recovery parity

Brief 7 (`area: always-allow-on`, `lens: error-state`). Inspiration only: upstream is read as data; no
upstream code, asset, wording, brand or Pro component enters Boma Bot. Product name and strings stay
Boma Bot. Claims marked CONFIRMED were checked in `./spec`, `AGENTS.md` or the local test suite this
session; GUESS marks proposals the spec is silent on.

## 1. Upstream capability, in my own words (CONFIRMED: read as data)

The analysed product has a *saved tool-consent grant*: when a person answers a tool-call approval with
the grant option, that choice is remembered for the same tool and provider/server, so later matching
calls run without a fresh card. Its guard rails, from the architecture notes:

- The grant is kept only when the tool's MCP server lives in a durable, file-backed config layer; a
  thread-scoped config alone is not enough, so a transient grant is not persisted.
- The saved per-tool setting is part of the session fingerprint. When the policy changes, the next
  turn replaces the loaded provider session that held the old policy — a stale grant does not linger.
- A fully privileged agent does not ask: consents are accepted once and nothing is saved.
- If the config registration fails, the thread stops with repair steps (or with the option to turn the
  feature off); it does not silently continue under a weaker policy.
- Importing an agent that carried such a grant downgrades it to a warning, so a file cannot silently
  raise permissions. A change in grants or access can resolve approvals already waiting.

None of this is copied; it is only the behaviour to answer with our own design.

## 2. What it maps to in Boma Bot (GUESS — propose, do not assume)

Boma Bot already has the two halves: a per-space/per-user policy decides which calls need approval,
and an approval executes its effect once via a recorded effect key (REQ-017, CONFIRMED). The missing
piece is a *saved auto-approve rule*: a matching tool call skips the pending card and records the same
effect key, so replay still cannot double-run. This is not in `spec/01-requirements.md`; write the gap
in `docs/STATE.md` as a decision with the simplest safe default — rules are **off** by default,
narrowest scope (one tool + one connector), and never cover purchases (REQ-018) or takeovers (REQ-029).

## 3. Error/recovery parity, grounded in our error envelope

The stable envelope is `{ error, code }` with codes `UNAUTHORIZED | FORBIDDEN | NOT_FOUND |
BAD_REQUEST | CONFLICT | TOO_MANY_REQUESTS | INTERNAL`; messages are user-safe, internal detail never
crosses, and cross-tenant access is `NOT_FOUND` (03, openapi.yaml — CONFIRMED). Screen copy follows
§8.9: one sentence of what happened, one of what to do, no stack/ids/provider text, correlation id
behind "Details", text plus icon never colour alone. No toast for field errors (sign-in precedent).

| Failure condition | Code | Where it surfaces | Recovery |
|---|---|---|---|
| Rule points at a tool/connector removed or disabled for the space | `NOT_FOUND` (GUESS; missing config may be `BAD_REQUEST`) | Rule row in the settings panel, inline above the field | "This tool is no longer available." + "Remove or reselect it."; keep form values, mark rule inactive, never silently drop it |
| Create/update while space is read-only or actor lacks authority | `FORBIDDEN`; other tenant's resource `NOT_FOUND` | Inline error in the panel | Explain authority, keep draft, no privilege escalation; re-check on submit |
| Two members edit the rule / answer the same card | `CONFLICT` | Card shows first result (REQ-007); panel reloads current state | "Someone changed this first." + "Reload."; effect key still prevents a second execution (REQ-017) |
| Auto-approved tool hits a provider 429/limit | `TOO_MANY_REQUESTS` | Failed run-status row in the transcript | Retryable classification + retry (REQ-021, REQ-012 vocabulary); "Details" carries the correlation id |
| Config/rule store temporarily unavailable | `INTERNAL` (generic text) | Thin "not live" bar + reconcile on reconnect; if a turn dies, a failed row | Re-apply saved rule on reconnect, no duplicate rows; repair steps, never raw provider text |
| Import/duplicate carries an auto-approve rule | no RPC error — review warning (GUESS) | Import review list, rule off until confirmed | Explicit opt-in per rule; default off, so a file cannot raise authority |
| Rule revoked while a run is already relying on it | none | Next tool call raises a fresh pending card | Same call re-prompts; prior effect key still blocks replay (REQ-017) |

Approvals stay modal for the effect only: transcript readable, other threads reachable, focus on the
card (REQ-007); errors are announced through the polite live region (§8.9).

## 4. UI surface and BoardUI tokens (CONFIRMED names from AGENTS.md / components tree)

- Settings are **a panel inside `/app`**, never a page (§8.1). Add an "Auto-approve rules" section to
  the existing settings panel using installed BoardUI `Switch`/`Checkbox` from `components/base/`;
  no new design system, no Pro.
- Error text: `text-text-error-primary`, `text-body-regular` / `text-caption-1-semibold`; border
  `border-border-error-default`; surface `bg-background-secondary-default`; focus
  `outline-none focus-visible:ring-2 focus-visible:ring-border-focus-ring`. Merge with `cx()`;
  icons from `@remixicon/react` as component references; status chips use text + icon. Dark mode rides
  the tokens — no `dark:` literals, no raw palette classes.
- Run failure and offline bars reuse the transcript status row (§8.3) and live region (§8.9).

## 5. One Playwright test (test-first; REQ id in the name)

Add to `apps/web/tests/approval-card.spec.ts`, using the existing `seedRpc` fixture (CONFIRMED file):

```ts
test("REQ-017 an auto-approve rule for a removed tool shows a recoverable NOT_FOUND", async ({ page }) => {
  // seed a saved rule whose connector is absent + an Error envelope {code:"NOT_FOUND", error:"..."}
  // assert: rule row names the missing tool; inline message has one cause + one action;
  //         status chip is text+icon (not colour alone); "Details" hides the correlation id;
  //         no toast; switch stays off; keyboard reaches "Remove rule" and removes exactly one row;
  //         axe: no critical/serious finding.
});
```

The name carries `REQ-017` so `tools/trace_check.py` counts it; it asserts state, not a golden click
path. The 429 variant can reuse `REQ-021` in a later slice.

## 6. First slice

1. Write that test first (red). Replay `spec/golden/thread-answer.json`, `unauthorized.json`,
   `cross-tenant-not-found.json` for the envelope contract.
2. Backend: typed `thiserror` variants mapped to the stable envelope; rule CRUD validates the target
   tool/connector exists and is enabled, rejects read-only spaces (`FORBIDDEN`), hides other tenants
   (`NOT_FOUND`); reuses the REQ-017 effect key; changes invalidate any cached consent policy.
3. Frontend: settings-panel rules list with the token-based inline error, live-region announcement,
   keyboard path and 360 px/dark/reduced-motion checks; no Pro, no new library.
4. Done when the test is green, goldens replay, `cargo fmt --check`, `clippy -D warnings` and Playwright
   pass; record CONFIRMED results and the "rule off by default" decision in `docs/STATE.md`.
