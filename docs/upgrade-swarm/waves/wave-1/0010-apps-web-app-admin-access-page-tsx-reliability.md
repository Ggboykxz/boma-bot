# 0008 — `apps/web/app/admin/access/page.tsx` — Reliability (v0.4.0)

Scope: `app/admin/access/page.tsx` (thin wrapper) + `components/admin/access-screen.tsx`
("use client", REQ-058, spec §8.7). Read-only; no code changed. Tokens cited are those the
screen already uses (`bg-background-full`, `border-border-*`, `text-text-*`,
`text-foreground-icon-primary`, `border-border-error-default`).

## Today (CONFIRMED by reading)

`useEffect` fires `Promise.all([members, invitations, grants])` behind an `AbortController`;
one phase machine (`loading|ready|error|forbidden`); three mutations with hand-rolled pending
flags. `lib/api/rpc.ts`: bare `fetch`, **no timeout**; only unmount aborts; 401 → global
`unauthorizedHandler`; else `RpcError{code,status}`.

## Failure modes

1. **Network down / API unreachable.** `fetch` rejects or hangs forever → `phase="error"` unless
   `code==="FORBIDDEN"`. Retry button exists, but a half-open connection (captive portal, dead VPN)
   never surfaces an error; skeleton sits forever. (`reloadToken` cleanup does abort the old call.)
2. **Partial roster — one of three calls fails.** `Promise.all` is all-or-nothing: grants failing
   wipes members+invitations; user sees the generic error card, not two usable sections and one
   failed. Independent resources coupled to one fate — biggest gap on the page.
3. **Stale role / stale roster.** No focus revalidation, no polling, no `staleTime`. An admin
   demoted mid-session keeps a working-looking screen until a write 403s (writes handle that
   badly — see 4). Changes from another tab are invisible until manual retry; no "last updated".
4. **Forbidden mid-session.** Reads map FORBIDDEN → `forbidden` (correct). Writes don't:
   `revokeGrant` falls back to `t("admin.plugins.error")`; `revokeInvitation` swallows the error
   **silently** ("a refresh restores truth" — nothing triggers one). Demoted admin sees a no-op
   revoke that looks like success.
5. **Optimistic create / double-insert.** `adminGrantsCreate` inserts the server-echoed row
   (honest), but no dedupe by email and no idempotency key — a retry after timeout-but-committed
   double-inserts client-side. `revokeGrant` filters the row after the call resolved; no
   reconciliation if state diverges.
6. **Forbidden sniffing.** `error.code==="FORBIDDEN"` is checked inline at one callsite; fragile,
   not reusable by writes. Non-RpcErrors fall to `error` (acceptable fallback).
7. **401 vs 403.** 401 fires the global handler, but the catch still runs; if the handler doesn't
   redirect, phase=`error` with a retry that 401s forever, unexplained.
8. **Empty vs failed.** Empty roster ("—" fallback, em-dash null) is indistinguishable from a
   backend returning `[]`; section-level errors (2) would make this diagnosable.
9. **Slow revoke / double-trigger.** Pending flags disable only the clicked button; no
   `aria-busy` on rows/buttons; Enter can re-trigger before re-render in theory.
10. **Nav staleness.** `/admin/plugins` link renders even in `forbidden`; a demoted admin gets a
    working link into another admin surface. Minor, but a stale-role smell.

## Hardening (v0.4.0)

- **Split the load.** `Promise.allSettled` + per-section state (`members|invitations|grants`,
  each `loading|ready|error|forbidden`). Sections render independently; the failed one shows an
  inline `role="alert"` row with its own retry (`RiRefreshLine`, `variant="secondary"`). Same
  board tokens, one tint, no new palette.
- **Timeout every call.** `AbortSignal.timeout(10–15s)` composed with the unmount signal
  (`AbortSignal.any`) in `call()` or at the screen; on timeout → error state + retry. Keep
  skeleton `aria-hidden`, add `aria-busy` on the container and a visually-hidden `role="status"`
  ("Loading access overview…") — skeletons are silent to screen readers.
- **Stale-role revalidation.** Refetch on `focus` + `visibilitychange`, throttled ≥30s with
  in-flight dedupe, plus a manual header refresh. Any FORBIDDEN from any read **or write** flips
  the whole screen to `forbidden` via one shared mapper (never inline-sniff again). Show
  `aria-live="polite"` "Updated HH:MM" caption (`text-text-tertiary text-caption-1-semibold`).
- **Writes consistent with reads.** Route all three mutations through the same mapper: FORBIDDEN →
  `forbidden`; 401 → global handler; else inline `role="alert"` **and keep the row visible** with
  retry — never swallow (fix `revokeInvitation`). Prefer server-confirmed rows over more optimism.
- **Idempotency + dedupe.** Client nonce on `adminGrantsCreate` (if server ignores it, harmless);
  dedupe inserted grants by email (replace, don't append); disable submit while `granting` +
  `aria-disabled`/`aria-busy`.
- **Copy + i18n.** `RpcError.code` → translation-key map; stop reusing `admin.plugins.error`;
  translate hardcoded `Name`/`Role` headers and both `placeholder`s; section failure copy says
  what failed ("Couldn't load grants. Members and invitations shown.").
- **Layout/CLS.** Skeleton heights already match sections; keep `min-h-dvh` and 360px
  `overflow-x-auto` table (`min-w-[560px]`). Keep static skeletons (no new motion; honor
  `prefers-reduced-motion`).
- **Accessibility.** `role="alert"` on error/forbidden panels; on phase transition to
  error/forbidden, move focus to the panel heading (`ref`, `tabIndex={-1}`, once per transition);
  keep `aria-labelledby` headings and `scope="col"`; destructive buttons get `aria-describedby`
  naming the target; `text-text-tertiary` empty states verified ≥4.5:1; error/forbidden never
  color-only (icon + text, as today).

## Risks

- `allSettled` refactor is on the only admin-access surface; needs Playwright for
  loading/ready/error/forbidden × partial combos before merge.
- Focus revalidation can double-fetch on alt-tab spam — throttle + dedupe mandatory.
- Timeout too tight on poor networks → false errors; 10–15s is a guess, measure first.
- Idempotency key needs server support; if absent it is a no-op (log it, don't block).
- Focus-on-error can trap keyboard users if applied per render — transition-only.
- Rollback UX for optimistic revoke stays unresolved; prefer server-confirmed mutation.
- Hiding the `/admin/plugins` link requires role client-side; if unavailable, leave it visible —
  do not fake role checks.
- Spec creep: all of this hardens REQ-058 only; any copy change is user-visible → keep minimal.

## Verification

Playwright + axe: full outage→error+retry recovers; grants-only 500→sections render, grants shows
inline error; FORBIDDEN on mount→forbidden panel; FORBIDDEN on revoke→screen flips; >15s hang→
error not skeleton; keyboard-only create/revoke path; visible focus at 360px light+dark; zero
serious/critical axe findings.

CONFIRMED: wrapper, screen, rpc.ts (no timeout; 401 global; FORBIDDEN sniffed per-callsite).
GUESS: server idempotency support; role not exposed to client today.
