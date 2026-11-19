# Parity proposal — always-allow-on, performance lens

Worker 006 · brief `id: 6` (`area: always-allow-on`, `lens: performance`) · target milestone M16
(`docs/PLAN.md` §5). Research only: upstream clone read as data; no upstream code, asset, wording or
brand is reproduced here. Only this document is written.

## 1. The capability, in our own words

Some assistants can be told, once, to stop asking before a class of side effect. A person flips a
setting for an agent/tool scope; from then on matching tool calls and publishes run without a
confirmation card, and cards already waiting for that scope are answered automatically. The switch
is scoped (which agent, which tool family, which provider account) and persists across restarts and
reconnects, so the "we already agreed to this" fact is not re-negotiated on every turn. Taking the
switch away must not leave a stale grant: previously auto-approved permissions degrade back to an
explicit check. A few sensitive categories — signing in, filling interactive forms, and handing a
person control of a remote screen — deliberately stay interactive and are never swept up by the
blanket grant. Only an owner/admin-level actor may change the scope.

The interesting part for this lens is that the decision is a *small, durable fact consulted on a hot
path*: it is checked at approval time, it must not add a round trip on a weak connection, and it
must survive a dropped stream without double-executing the effect it authorized.

## 2. Parity changes for low-end hardware and poor networks

Grounded in `docs/PLAN.md` §4 budgets (read p95 ≤ 150 ms @ 2 vCPU, SSE first frame ≤ 300 ms, stream
frame p95 ≤ 150 ms, RSS idle ≤ 150 MB, JS initial ≤ 200 KB gzip, LCP ≤ 2.5 s / INP ≤ 200 ms / CLS
≤ 0.1 on throttled 4G) and `spec/04-nonfunctional.md` §4.3/§4.8 (bounded history; degrade to a
visible "not live" state when realtime drops):

1. **Evaluate the grant server-side, not in the browser.** The scope→decision lookup lives in
   `domain`/`app` at the approval gate (REQ-017) as a fail-closed pure rule. No rules table, matcher
   or rule editor ships in the client, so the parity adds zero bytes to the critical JS path. This
   is the single most important low-end choice: the toggle is UI, the policy is not.
2. **Carry the decision on state the client already receives.** Put the resolved scope flag on the
   card/run snapshot written into the TanStack Query cache by `(threadId, seq)` (ADR-014). A client
   that reconnects on a poor link learns "always allowed" from the snapshot it already fetches — no
   second RPC, no extra SSE subscription, no polling. Total shipped JS in the current static export
   is already ~369 KB gzip across 36 chunks; add no chunk and no dependency.
3. **Auto-answer pending cards in the granting transaction.** Flipping the switch resolves waiting
   cards for that scope in the same DB transaction, reusing the existing `effect_key` exactly-once
   ledger (REQ-017). A replay or duplicate answer after a dropped uplink cannot run the effect
   twice, and the approval read path stays inside the 150 ms p95 because it is one indexed RLS
   lookup keyed by `(space_id, bot_id, tool_key)` — not a scan.
4. **Optimistic, single-toggle UI on the existing card.** Reuse `CardBlock` in
   `apps/web/components/thread/message-blocks.tsx` and its `threads.answer` pattern: one secondary
   "always allow" action sets local answered state immediately, then reconciles from SSE, exactly as
   approve/deny already do. The composer unlocks on the local action, so a slow uplink never blocks
   typing. No new animation: no per-frame style recalc or layout thrash on a weak GPU, and existing
   reduced-motion handling is inherited.
5. **Stay honest offline.** If the stream is unavailable, the toggle shows a pending state and the
   "not live" indication from §4.8; it never claims a grant that has not been recorded. No cached
   authenticated response is reused to fake a decision (§4.8).

## 3. BoardUI tokens (free tier only, no Pro)

Reuse the card's current semantic styling — no raw palette classes or hex. Surfaces
`bg-background-secondary-default` / `bg-background-primary-default`; text `text-text-primary`,
`text-text-secondary`, `text-text-tertiary`; separators `border-separator-border` / `border-border-*`;
icons via `@remixicon/react` coloured with `text-foreground-icon-*`. Type uses composite utilities
`text-body-medium`, `text-body-2-regular`, `text-caption-1-semibold` (not `text-sm font-medium`).
Merge classes with `cx()` from `@/utils/cx`. Dark mode flips via tokens, never `dark:` colours. Keep
44 px targets, visible focus ring, and logical (RTL-safe) spacing for the he/ar locale path.

## 4. One REQ-named gate

`REQ-017_always_allow_grants_and_auto_answers_once`

Integration test (real Postgres, name carries the id per `docs/PLAN.md` §7 / `tools/trace_check.py`):
with a waiting approval for scope S, call `approvals.setAlwaysAllow(S)`; assert the card closes, no
new card appears for a second matching call, the recorded effect fires exactly once even after a
replayed answer, and a client rebuilt from the snapshot needs no extra RPC to see the grant.
Supporting build gate (not a second REQ gate): the existing JS budget check asserts no new initial
chunk and initial JS stays ≤ 200 KB gzip.

## 5. First slice

- `domain`: add `ApprovalScope` + a pure `resolve(scope) -> Grant | Ask` with a fail-closed default.
- `app`: `setAlwaysAllow` use case; flip policy and auto-answer waiting cards in one transaction;
  reuse `effect_key`; surface the flag on the run snapshot.
- `api`: one RPC `approvals.setAlwaysAllow` (owner/admin check, RLS by `TenantContext`).
- `web`: one secondary "always allow" button on the existing `CardBlock` with optimistic answered
  state and SSE reconcile; ICU strings in the shared catalogue; zero new dependencies.
- Exit: the REQ-named test green, JS budget gate green, axe zero critical/serious, reduced-motion and
  RTL smoke paths clean.

Explicitly out of this slice: what-if previews, a general rules engine, un-scoping sign-in / form /
takeover categories, any Pro component, and any upstream code, asset, wording or brand.
