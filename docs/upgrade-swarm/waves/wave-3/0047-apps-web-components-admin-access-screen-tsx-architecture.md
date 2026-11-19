# 0047 — apps/web/components/admin/access-screen.tsx — architecture lens

- Target: `before-m5` (v0.3.0-rc5), module `apps/web/components/admin/access-screen.tsx` (319 lines, one `"use client"` component).
- READ-ONLY review. Spec anchors: `spec/08-ui-ux.md §8.7` (`/admin/access`: members table name/email/role/**last seen**, pending invitations, access grants with reasons; **role changes are optimistic with rollback on failure; removing a member states what they lose**), `spec/01-requirements.md` REQ-058 (SHOULD, admin-only procedures refused for non-admins), BoardUI rules (components first, semantic tokens, composite type, cx).

## Structure today

One `AccessScreen` holds three sections plus all state: members/invitations/grants arrays, a `loading|ready|error|forbidden` phase machine, a `reloadToken` refetch trigger, grant-composer fields (`email`, `reason`), and four in-flight flags (`granting`, `grantError`, `revokingEmail`, `revokingInvitation`). Data load is a single `Promise.all` over `adminMembersList` / `adminInvitationsList` / `adminGrantsList` under one `AbortController`; mutations (`adminGrantsCreate/Revoke`, `adminInvitationsRevoke`) patch local state in place. Render: header nav → h1 → skeleton / forbidden / error+retry / three rounded-3xl bordered sections (members `<table>`, invitations `<ul>`, grants form + `<ul>`).

Confirmed observations (read this session):

- **Spec gap vs §8.7**: no role-change control at all (header comment says deliberate — "needs the same evidence flow… recorded, not optimistic guessing" — yet §8.7 mandates optimistic-with-rollback), no member removal flow with "what they lose" copy, and the members table lacks the **last seen** column. Grants-vs-role-flip asymmetry is a product decision recorded in a code comment, not in docs/PARITY.md or an ADR.
- **Monolith**: fetch, phase machine, three sections, and the grant composer all live in one component; no subcomponents, no data-hook, nothing reusable by `/admin/plugins` (same phase-machine pattern almost certainly duplicated there).
- **`useMemo(() => phase === "ready", [phase])`** — a memoized boolean of a boolean; noise, not derivation.
- **Hardcoded English**: `Name` / `Role` table headers, `placeholder="person@example.com"`, `"Why this grant exists"` — not in the i18n catalogue (spec REQ-057; `useTranslation` is used elsewhere in the file).
- **Raw `<input>`** for email and reason; BoardUI `components/base/input` (+ `select`, `textarea`) is the house pattern; react-aria wiring (label/error/keyboard) is lost.
- **Error handling asymmetry**: `adminGrantsRevoke` failure surfaces `grantError` (shared with create); `adminInvitationsRevoke` failure is swallowed silently with a comment claiming "a refresh restores truth" — but no refresh is triggered, so the row stays with a stale disabled state and zero user feedback.
- **Mutations are not abortable** and not covered by the reload token; `reload()` forces full `loading` phase (skeleton flash) instead of a background revalidate.
- **Forbidden detection is structural**: `error.code === "FORBIDDEN"` via inline object cast, duplicated knowledge of the RPC error shape; `admin.plugins.error` catalogue key reused for access-screen errors (cross-screen coupling).
- Tokens are honest throughout: `bg-background-*`, `text-text-*`, `border-separator-border`, `border-border-button-default`, composite type utilities, `dir="auto"` on emails, `role="alert"`, `aria-labelledby` sections. Keep all of this.
- `sections`/skeleton heights are magic numbers; skeleton has `aria-hidden` but no accessible label for screen-reader users in the loading phase.

## Proposals (v0.4.0)

### 1. Decompose into a data hook + three section components
Extract `useAdminAccess()` in `lib/` (or `components/admin/use-admin-access.ts`): owns the phase machine, AbortController, reload token, and per-mutation state; returns `{ phase, members, invitations, grants, reload, createGrant, revokeGrant, revokeInvitation, … }`. Split the JSX into `MembersTable`, `InvitationsList`, `GrantsSection` (each a sibling file under `components/admin/`), keeping the shell/nav in `AccessScreen`. Risk: prop-drilling creep — keep the hook return as one object; behavior must be byte-identical, verify with existing Playwright coverage for REQ-058 (GUESS: check the REQ-058 spec test names before refactoring).

### 2. Unify the phase/fetch machine across admin screens
Lift the `loading|ready|error|forbidden` + abort + retry pattern into the shared hook from (1), parameterized by fetchers; `/admin/plugins` consumes the same. Forbidden detection centralizes on a typed `RpcError` guard (`lib/api/rpc` already exports the shape — add `isRpcError(error, code)`). Risk: coupled error wording across screens; mitigate by keeping per-screen catalogue keys (`admin.access.error`/`admin.plugins.error`) at the call site. Test the network-down case explicitly (TypeError → `error`, not `forbidden`).

### 3. Close the §8.7 gap deliberately — spec reconciliation, not silent expansion
Either (a) implement last-seen column + role change with optimistic update and rollback-on-failure (server echo, revert on throw, `role="alert"` on failure) and member removal with an explicit "what they lose" confirmation, or (b) record a DEVIATION in docs/PARITY.md that role/removal ship in a later milestone, with the current screen labeled as grants-only. Do not leave the divergence implicit in a code comment. Removal must state consequences in the catalogue. Risk: (a) is real scope (RPC surface may be missing — verify against `spec/03-api-contract.md` before committing; contract change = spec edit + trace); (b) is cheap but needs maintainer sign-off. Do (b) now, (a) behind the v0.4.0 gate.

### 4. BoardUI form components + 100% catalogue coverage
Swap raw `<input>` for `components/base/input` (reason as `components/base/textarea`, maxLength wired to its counter if available); move `Name`/`Role`/placeholders into the i18n catalogue with fr/he entries or documented English fallback. Grants form gets a single `role="alert"` summary and per-field error wiring from the base component. Risk: golden snapshot churn (class strings change) and axe focus-order differences — re-record goldens intentionally, keep `boma-focus` ring behavior identical.

### 5. Revalidation and failure honesty
Make `reload()` a background revalidate (keep `ready`, spin the refresh button, `aria-live="polite"` announcement) instead of full skeleton flash; on `adminInvitationsRevoke` failure, surface the same `grantError`-style inline alert (or a per-row alert) and keep the row enabled for retry; on `adminGrantsRevoke` failure keep the grant visible (already the case) but announce via the same live region. Risk: optimistic state divergence on partial failure — anchor all mutations to server-echoed rows and reconcile on revalidate; abort semantics for in-flight mutations need explicit handling on unmount.

## Explicit non-goals
No client-side authorization (server refuses FORBIDDEN; screen only reports it). No optimistic role flip without rollback. No new admin screens beyond §8.7. No BoardUI Pro components. No pagination changes (lists are small by construction).

## Suggested sequencing
5's two one-line failure-honesty fixes → 1 (mechanical split) → 4 (component swap) → 2 (shared hook) → 3(b) immediately, 3(a) as its own REQ-gated slice. Each step gated by `lint`, `typecheck`, axe, and the REQ-058 Playwright tests.
