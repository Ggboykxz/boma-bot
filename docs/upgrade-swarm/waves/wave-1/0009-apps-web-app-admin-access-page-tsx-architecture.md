# 0007 — apps/web/app/admin/access/page.tsx — architecture lens

- Target: `before-m5` (v0.3.0-rc5), module `apps/web/app/admin/access/page.tsx` (12 lines).
- Read-only review; no code edited. Sister file `apps/web/components/admin/access-screen.tsx` (319 lines) read because the page is a pure delegate — the substance, and the debt, lives there.
- Spec anchors: `spec/08-ui-ux.md §8.7` (Administration), `spec/01-requirements.md` REQ-058 (Administration surfaces, SHOULD), `docs/UI.md` (REQ-058 → `/admin` operator-only, components `table`/`badge`, states operator / non-operator), `docs/UI.md` copy conventions (every user-visible string via the catalogue).

## The page's role, today

`/admin/access` is one of the two operator surfaces (with `/admin/plugins`) that REQ-058 carves out of the conversation shell: their own screens, space-admin-only, server-refuses-everyone-else. The page itself is exactly what it should be — a thin server component delegating to `<AccessScreen />`. That shape is correct and should survive v0.4.0: routing, metadata and any server-side gate belong in `app/`, rendering in `components/`. The current weakness is that *everything else* was pushed down into one client component, so the page is a formality and the architecture is accidental rather than designed.

Confirmed observations (read this session):

- `AccessScreen` is `"use client"`, monolithic: three sections (members table, pending invitations, grants create/list/revoke) plus data loading, error typing and the page chrome in one file.
- The admin header/nav (Boma Bot wordmark → `/`, link to `/admin/plugins`, `max-w-5xl` container) is hand-rolled **identically** in `plugins-screen.tsx` — duplicated chrome with no shared `app/admin/layout.tsx`.
- Error handling sniffs `error.code === "FORBIDDEN"` structurally (`typeof error === "object" …`) instead of using the typed `RpcError` (`lib/api/rpc.ts`), while `instanceof RpcError` is used elsewhere (`message-blocks.tsx`). Same pattern duplicated in `plugins-screen.tsx`.
- Hardcoded English ships in the JSX: `Name`, `Role`, `placeholder="person@example.com"`, `placeholder="Why this grant exists"` — against the catalogue rule in `docs/UI.md` ("Every user-visible string goes through the catalogue").
- Raw `<input>` elements are used where `docs/UI.md` mechanics say form components build on `react-aria-components` and BoardUI form components should be extended, and where the installed `Input` pattern elsewhere in the app would apply.
- Grants create/revoke is optimistic but honest (server-echoed row inserted, filter-on-revoke); revoke-invitation silently swallows failure with no message — inconsistent feedback discipline between the three mutations.
- Spec §8.7 items are absent: no `last seen` column on the members table, no optimistic role change with rollback, no member-removal flow stating what the member loses (the file documents its own deliberate deferral: "a role flip needs the same evidence flow, recorded, not optimistic guessing" — that is a sound GUESS, but it is recorded only in a comment, not in `docs/STATE.md`).
- No route-level `loading.tsx`/`error.tsx`, no `metadata` export (page title/tab says nothing admin-specific).
- Authorization is UI-by-courtesy only: the server refuses the calls, which is correct, but nothing on the client avoids even rendering the form for a known non-admin (acceptable; the server is the boundary — keep it that way, do not "fix" this by trusting client state).

## How it should evolve in v0.4.0

Decompose along the three sections the spec names, type the failure modes the contract already defines, and hoist shared admin chrome into a layout so the page shrinks to routing + metadata. Keep the page as a server component; keep the server as the only authorization boundary; keep visible strings in the catalogue.

## Proposals

### 1. Hoist shared admin chrome into `app/admin/layout.tsx` (server component)
Move the duplicated header/nav, the `max-w-5xl` container ground, and an `<h1>`-adjacent slot (or at least shared metadata) into a route-group layout used by both `/admin/access` and `/admin/plugins`; add `export const metadata` ("Access — Admin") in the pages. Risks: (a) the two screens currently differ in nav link placement — merging forces a decision on which is canonical; (b) a server layout cannot host client-only affordances, so any future "sign out admin" control must stay a client island; (c) low but real regression risk in the axe-gated REQ-058 Playwright path — mitigate by running `npm run lint`, the REQ-058 spec and the axe suite before merge. Upside: one chrome, one focus order, one place for the admin `robots: noindex` (admin surfaces should never be indexed).

### 2. Replace the ad-hoc phase machine with a typed admin-resource helper + shared `AdminGate`
Introduce `isRpcErrorCode(error, "FORBIDDEN")` (or `error instanceof RpcError && error.code === …`) in `lib/api/rpc.ts` (or a small `lib/admin/` module), a `useAdminSection<T>` hook (loading/ready/error/forbidden, AbortController, reload token — exactly the shape already duplicated twice) and a presentational `AdminGate` for the forbidden/error/retry states. Both admin screens consume it. Risks: (a) shared code couples the two screens — a change for one can regress the other's forbidden wording; (b) the structural sniff currently tolerates non-`RpcError` throwables (e.g. fetch TypeErrors) — tightening to `instanceof` funnels everything else to generic `error`, which is correct per the error-envelope contract but must be tested with a network-down case; (c) hook extraction must preserve `controller.abort()` on unmount or it reintroduces set-state-after-unmount. This is the highest value-to-risk ratio of the three.

### 3. Decompose `AccessScreen` and close the spec gap deliberately (not opportunistically)
Split into `MembersTable`, `InvitationsPanel`, `GrantsPanel` (props in, callbacks out; no fetching inside children), swap raw `<input>` for BoardUI `input`/`Input` (react-aria foundation, error states for free), move the five hardcoded strings into the catalogue (`admin.access.members.name`, `admin.access.grants.*.placeholder`), surface grant-create failure through one consistent `role="alert"` region shared by all three mutations, and record in `docs/STATE.md` (not only a comment) the deferred §8.7 items: `last seen` column, optimistic role change with rollback, member removal with a "what they lose" confirmation. Risks: (a) widest diff — reviewer load and axe/golden churn; (b) the `last seen` column needs a server-side field that may not exist yet (GUESS — verify against `spec/03-api-contract.md` before adding); optimistic role flip without a server echo is explicitly out of scope until the evidence flow exists; (c) catalogue additions require translations for fr/he or documented English fallback, per `docs/UI.md`. Do not ship 3 as one commit; land it behind the REQ-058 Playwright gate in slices.

## Explicit non-goals
No client-side role caching, no "hide the form for non-admins" security theater, no new admin surfaces beyond the spec, no BoardUI Pro components. The server-refuses model stays; the UI only describes it honestly.
