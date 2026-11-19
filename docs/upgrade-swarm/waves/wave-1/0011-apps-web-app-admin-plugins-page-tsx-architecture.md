# 0009 — apps/web/app/admin/plugins/page.tsx — architecture (v0.4.0)

Worker #009 · module `apps/web/app/admin/plugins/page.tsx` · lens: architecture · READ-ONLY
review of `rebuild-workspace/.worktrees/before-m5` (v0.3.0-rc era). Proposals only; no code
in this change. Line numbers cite the before-m5 worktree.

## Page shape observed

`page.tsx` (12 lines) is a thin server wrapper: it re-exports `<PluginsScreen />` and carries the
REQ-058/§8.7 doc comment. That shape is correct for ADR-013 (static export) and should survive
v0.4.0 untouched. The architecture mass lives one level down, in `components/admin/plugins-screen.tsx`
(168 lines, `"use client"`), `lib/api/rpc.ts`, and the Rust RPC surface
(`admin.plugins.list` / `admin.plugins.decide`, wire.rs `AdminPluginSubmissionView`).

Findings (architecture, not reliability — see worker #010/#012 for error-path detail):

- **All four jobs in one component.** `PluginsScreen` owns data fetching (`adminPluginsList`,
  abort wiring, `reloadToken`), interaction mutations (`adminPluginsDecide`), markup (nav header,
  skeleton, forbidden/error/empty/list, action-error banner), and the `Phase` state machine —
  inline in one function body. There is no seam between "the review queue as a use case" and
  "the queue as pixels".
- **Admin shell duplicated verbatim.** `access-screen.tsx` (319 lines) and `plugins-screen.tsx`
  repeat the same header/nav (`boma-target`, `max-w-5xl`, `app.name` + screen title + link to the
  sibling admin page), the same `Phase` union, the same `reload` callback, the same skeleton pair,
  the same forbidden/error blocks. `app/admin/` has no `layout.tsx`; the two pages are siblings
  that each re-declare the chrome.
- **Hand-written contract drift surface.** `lib/api/rpc.ts` is hand-written "until the OpenAPI
  procedure codegen lands (ADR-003)" and mirrors `crates/api/src/wire.rs` by convention only.
  `AdminPluginSubmissionView` here (id, source, name, status, createdAt) matches wire.rs today
  (verified), but nothing fails CI when one side moves; `status` is typed `string`, not the
  six-value union the server enforces (`pending|unconfirmed|approved|rejected|applied|revoked`).
- **Spec §8.7 vs implementation gap.** §8.7 promises "source reference, submitter contact, review
  state, and the parts the installer refused with the reason". The screen shows source, name,
  status, and drops `createdAt` on the floor; the wire view carries no submitter contact
  (migration 0027 deliberately "never returned by any API") and no refused-parts data (the install
  plan's `skipped[]` lives on `marketplace.install`/`plugin_installs`, a different flow, pre-install
  by design). The gap is partly deliberate privacy design — it is unresolved at the contract level,
  not just the UI level.
- **Decision is fire-and-forget in the component.** `decide()` filters the row out on success only;
  there is no use-case layer between the button and `rpc.ts`, no shared "admin review action"
  primitive, and the only test seam is Playwright route interception (`admin.spec.ts`, REQ-058).
- **No server-side route guard for `/admin/*`.** Defense is RPC-layer `require_admin` plus a client
  `FORBIDDEN` branch per screen; that is sound (never trust the client) but the duplicated
  FORBIDDEN handling in every admin screen is the tell that the boundary is implicit, not structural.

## Proposed upgrades for v0.4.0 (in dependency order)

### U1. Extract `AdminShell` + `useAdminQueue` — give `/admin/*` one boundary
Add `app/admin/layout.tsx` (server, static-export-safe) rendering the shared chrome
(header/nav, `max-w-5xl`, `bg-background-full`) once, and a `components/admin/admin-shell.tsx` +
`hooks/use-admin-queue.ts` encapsulating: `Phase`, abort wiring, `reloadToken`, `forbidden`/`error`
detection, and the skeleton/empty/error/forbidden slots. Both `AccessScreen` and `PluginsScreen`
compose it; `page.tsx` stays a thin wrapper. This removes the largest duplicated block and makes
`/admin` a real segment with a declared contract instead of two copy-pastes.
- Risk: markup drift vs the REQ-058 Playwright goldens and `boma-target` focus ring order; the
  two screens legitimately differ (3-section vs 2-button rows). Mitigate: extract only the
  identical chrome and phase machine; keep per-screen bodies as slots; replay goldens + axe.

### U2. Split `PluginsScreen` into `usePluginReviewQueue` (data) + presentational parts
One hook owns list/decide/reload and exposes `{ submissions, phase, decide, reload }`; pure
components render rows (`SubmissionRow`), the list, and the action-error banner. `page.tsx`
unchanged. This is the seam the v0.4.0 contract work (U3/U4) and any detail drawer will hang off,
and it makes the decision use case unit-testable without the DOM.
- Risk: behavior drift on the exact `deciding`/list-removal sequence the REQ-058 spec test asserts
  ("approves one submission and refuses the second decision"). Mitigate: move first, redesign
  never, in one PR; the existing attribute names and i18n keys stay fixed.

### U3. Type the review contract end-to-end — status union + refusal report (contract PR)
Narrow `AdminPluginSubmissionView.status` to the server union; add a read-only
`admin.plugins.preview` (or extend `admin.plugins.list` behind a flag) returning, for one
submission: parsed ref/subdir, resolved SHA, and the `skipped[]`-style refusal list the installer
*would* apply (manifest parse + no-local-code rules already exist in `build_plugin_plan`,
crates/app/src/usecases.rs). Submitter contact stays server-side: show "contact on file: yes/no" or
nothing, and file the §8.7-vs-migration-0027 conflict in docs/STATE.md as a decision for the owner —
do not silently widen the wire. Any wire change is an ADR + PARITY.md entry.
- Risk: DEVIATION if shipped unapproved; preview fetch cost on large plugins (cap file count/size
  server-side, never fetch on mount). Mitigate: owner sign-off, offline-built plan (no network
  side effects), explicit "Approving never executes" copy preserved.

### U4. Retire the hand-written mirror — generate or at least pin `rpc.ts` shapes
ADR-003 already anticipates codegen from the OpenAPI/procedure contracts. For v0.4.0 at minimum:
add a CI check (vitest or a rust test) asserting the TS interfaces in `rpc.ts` match
`wire.rs` field-for-field for the admin namespace — or land the actual codegen for the admin
procedures. The admin surface is small (members/invitations/grants/plugins, ~12 types), so this
is cheap and kills the silent-drift class of bug.
- Risk: codegen tooling churn, false-positive diffs from serialization attributes (`skip_serializing_if`).
  Mitigate: start with a struct-shape snapshot test; full codegen as a separate ADR.

### U5. Admin-only route boundary via segment check (defense in depth, no behavior change)
Add `app/admin/layout.tsx` (from U1) doing a server-side session/role assertion at the segment
level where the deployment allows it (Next.js middleware or a layout-time check against the
session cookie), redirecting non-admins before any admin HTML/JS shell ships — keeping the RPC
`require_admin` refusal as the authoritative gate. Must stay static-export-compatible (ADR-013);
if the static build cannot assert roles, document that and keep the client FORBIDDEN branch.
- Risk: ADR-013 conflict (no server runtime in the static export); leaking which routes exist.
  Mitigate: if incompatible, DROP U5 and rely on U1's shared FORBIDDEN block — do not bolt on a
  runtime just for this. Decide with an ADR, owner veto applies.

## Execution order & gate
U1 → U2 (structure, one PR theme) → U3/U4 (contract, owner-gated) → U5 (ADR-gated, droppable).
Every step: Playwright REQ-058 suite green, axe AA clean, goldens replayed, `page.tsx` diff stays
≤ a thin wrapper, BoardUI semantic tokens only (`text-text-primary`, `bg-background-full`,
`border-separator-border`, `text-title-3-semibold`, `rounded-3xl`), `cx()` for merges, icons from
`@remixicon/react` as component refs.

## What not to touch
The thin-wrapper `page.tsx` itself; the "approving never executes anything" contract and copy;
`<code dir="auto">` text-only source rendering (never HTML); the `boma-target boma-focus` ring
classes; the atomic server decide (`UPDATE … WHERE status='pending'`); i18n key names under
`admin.plugins.*`.

CONFIRMED by reading source (page.tsx, plugins-screen.tsx, access-screen.tsx, rpc.ts, wire.rs,
usecases.rs, admin.rs, migration 0027, admin.spec.ts, §8.7); risk notes GUESS until U1/U2 land and
REQ-058 tests replay.
