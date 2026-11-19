# 0048 · apps/web/components/admin/access-screen.tsx · LENS=reliability · v0.4.0 proposal

## Scope

Single `"use client"` `AccessScreen` (319 lines). Read this session: the component,
`lib/api/rpc.ts:737-774` (`admin*` fetchers/mutations, all accept an optional `AbortSignal` but the
screen passes one only to the three list calls), and the sibling architecture proposal 0047.
Spec anchors: `spec/08-ui-ux.md §8.7` (members with name/email/role/last-seen, pending invitations,
grants with reasons; role changes optimistic **with rollback on failure**, removals state what is
lost), REQ-058 (admin-only procedures refused for non-admins). Manifest markers are GUESS unless
tagged CONFIRMED (read-only review; no command executed).

## Failure modes

1. **Partial load is all-or-nothing.** CONFIRMED: `Promise.all` over the three list calls
   (`lines 51-78`) means one slow/failing endpoint (e.g. `grants.list` 500) discards the two that
   succeeded and drops the whole screen into `error` — or, if any call returns `FORBIDDEN`, the
   misleading `forbidden` phase. The forbidden test (`line 71`) cannot tell *which* endpoint refused,
   so a deployment-scoped grants refusal is reported as a total access denial. A transient blip on
   one section blanks all three and forces a full reload.
2. **Stale roster / lost-update races.** The lists load once and are never revalidated (`reload()`
   only fires on the explicit error Retry). There is no cross-tab or concurrent-admin invalidation:
   a member's role or the grant set can change server-side while this screen keeps showing the old
   roster. The planned role change (§8.7, currently absent per the header comment) makes this acute:
   two optimistic flips on the same member, or a flip racing a revoke, resolve out of order and the
   last response — not the last user action — wins, silently overwriting the newer state. The
   rollback path must not revert a *newer* successful change when an older request finally rejects.
3. **Mutation races and divergent local state.** CONFIRMED: `granting`, `revokingEmail` and
   `revokingInvitation` are single values, not sets. Two concurrent revokes: the first to finish
   clears the shared flag while the second is still in flight, re-enabling a button whose RPC is
   pending → double-submit. Concurrent `createGrant` for the same email appends two rows keyed by
   `grant.email` (React duplicate-key collision) even though the server likely holds one. Mutations
   pass no `AbortSignal`, so unmount leaves them writing into state via a still-live closure; the
   reload token does not cover them. `revokeInvitation` swallows its error (`lines 127-129`) with a
   comment claiming "a refresh restores truth" — but no refresh is triggered, so the row stays with
   a stale disabled flag and zero feedback.
4. **Audit write failures.** The header defers role flips because they must be "recorded, not
   optimistic guessing", and grants carry `reason`/`grantedBy` as the audit intent. If the server
   commits the access change but the audit/evidence write fails (or is a separate call), the client
   sees an error yet the grant/role is live — the UI keeps the row and shows nothing has happened,
   or rolls back a change that actually persisted. There is no compound-operation acknowledgement
   telling the client whether the audit record and the mutation succeeded together, and no
   idempotency key to make a safe retry after an ambiguous failure.

## Proposed hardening (v0.4.0)

- **A. Per-section load isolation.** Replace the single `Promise.all` with three independently
  settled states (`members/invitations/grants` each `idle|loading|ready|error`), or use `allSettled`
  and render each section from its own slice. A `FORBIDDEN` on one endpoint marks only that section;
  keep a full-screen `forbidden` only when the member-list call refuses. Risk: three partial-failure
  banners instead of one — mitigate with one shared retry control that refetches only failed slices.
- **B. Typed, per-endpoint error guard.** Replace the inline `error.code === "FORBIDDEN"` cast with
  an `isRpcError(error, code)` helper in `lib/api/rpc`, and use distinct catalogue keys per section
  (not the reused `admin.plugins.error`). Risk: touches shared RPC surface — coordinate with 0047 §2
  so the two proposals converge on one guard.
- **C. Mutation serialization + version guard.** Track in-flight work as a `Set<string>` keyed by
  target (email/invitationId) so each row's button disables independently and double-submits are
  impossible. Attach a monotonic `rosterVersion` (or per-key revision) to every optimistic patch;
  on response, apply only if the response's version is >= the current one, so a late stale rejection
  cannot roll back a newer accepted change. Pass the effect's `AbortSignal` to mutations and ignore
  results after unmount. Risk: revision semantics need server support (GUESS — verify in
  `spec/03-api-contract.md`); without it, fall back to "on any mutation error, revalidate the whole
  section from the server" instead of blind revert.
- **D. Revalidate on focus/interval + after mutation.** Refetch failed/stale slices when the window
  regains focus and after any successful mutation, while staying in `ready` (background refresh,
  per 0047 §5) — this is what actually cures the stale-roster failure, not just the explicit Retry.
  Risk: extra RPC traffic and flicker if echoed rows differ from server rows; anchor to server echo
  and diff before replacing to avoid transient list reordering.
- **E. Audit-aware compound result + idempotency.** Require the role/grant mutation RPC to return one
  acknowledgement covering both the access change and its audit record (`{ applied, auditWritten }`
  or an atomic server transaction). On `auditWritten: false` (or ambiguous network error), surface a
  non-blocking "change saved but could not be recorded — contact an admin" notice and revalidate
  rather than reverting; do **not** silently retry a blind write. Send a client-generated idempotency
  key per mutation so a retry after timeout cannot double-apply. Risk: requires backend contract
  change (DEVIATION — record in docs/PARITY.md and get sign-off); if the server cannot expose it,
  the honest minimum is to treat any post-mutation error as "unknown outcome", force revalidation,
  and never claim local rollback.
- **F. Failure honesty for invitations.** Route `revokeInvitation` errors through the same
  `role="alert"`/`aria-live` channel as grants, keep the row enabled for retry, and stop passing no
  signal. Risk: more visible alerts; keep copy calm and one per row.

## Explicitly rejected / deferred

- Client-side authorization or hiding controls by role — server refuses FORBIDDEN; the screen only
  reports it.
- Silent optimistic role flip without rollback/version guard — that is the exact race in mode 2.
- Polling on a tight interval — focus + post-mutation revalidation is sufficient for small rosters.

## Success criteria / tests

- One endpoint down degrades only its section; other two stay usable and render server data.
- Rapid double-flip / double-revoke on the same target produces one RPC and a state matching the
  last server acknowledgement; a late stale response cannot overwrite it.
- Mutation in flight at unmount writes nothing (no React "update on unmounted component").
- `revokeInvitation` failure surfaces a visible alert and a retryable row.
- Audit-write failure yields an "unknown outcome" notice plus a revalidated roster, never a silent
  revert. Extend the REQ-058 Playwright suite with route-mocked cases for each; keep existing tests green.
