# 0021 — apps/web/app/app/board/page.tsx — architecture lens

Target: `before-m5` (v0.3.0-rc5), module `apps/web/app/app/board/page.tsx` (16 lines) + the client it
mounts, `components/board/board-screen.tsx` (406 lines, `"use client"`). Read-only review; nothing
edited. Sister files read: `spec/08-ui-ux.md` §8.6 (spec §8.6), `spec/01-requirements.md` REQ-038
(COULD, feature-flagged, board route), `crates/api/src/router.rs` (static fallback, ADR-013),
`crates/{api,app,domain}` board seams, `apps/web/{lib/api/rpc.ts,lib/i18n.ts,tests/board.spec.ts}`,
`next.config.ts` (`output: "export"`), `docs/STATE.md` (M6 slice 4, rc5 notes).
Status of claims: CONFIRMED = read in this tree; GUESS = inferred, needs verify.

## The page's role, today

`page.tsx` is exactly right and should survive v0.4.0: a server component, five meaningful lines,
delegating to `<BoardScreen />` inside `<AppShell>`. Routing and (absent) metadata belong in `app/`;
rendering in `components/`. The spec comment says the flag gate lives "at the deployment"
(`BOMA_BOARD`); in reality the static export serves the page to everyone and the **client** probes
`boards.get` — HTTP 404 from the RPC maps to the `absent` phase, which renders a not-found-shaped
message inside the normal app shell with a 200 (CONFIRMED). REQ-038's "not found even when signed
out" therefore holds only at the UI-shape level, not at the HTTP level. All architecture debt is one
level below the page.

## Structure after the assignee-filter and BoardUI form landing (rc5)

The create form now uses BoardUI `Input`/`Textarea`/`Select`; an assignee `Select` filter narrows the
four columns and per-column counts recompute from the filtered set; rows stagger in via the motion
pass (REQ-057); catalogues carry 14 locales. What that landing did **not** fix:

- **Two divergent fetch paths.** The mount `useEffect` and the post-mutation `load()` are copies with
  subtly different semantics (the effect returns early on a rejected board probe and never applies
  the bots result; `load()` applies bots and clears tickets on any rejection). Bots can end up unset
  depending on which path ran (CONFIRMED).
- **Silent failure.** A non-404 `boards.get` rejection renders "ready" with an empty board — no error
  phase, no retry, indistinguishable from "no tickets" (CONFIRMED).
- **Stale selection.** `selected` is a copied `TicketView`; `load()` refetches the list but never
  re-derives the open ticket, so the dialog shows a comment count and `updatedAt` that drift after any
  mutation (move patches it; comment does not) (CONFIRMED).
- **Hand-rolled modal and raw comment control.** The detail dialog re-implements focus trap, Escape,
  and backdrop (no BoardUI modal primitive is installed; `settings-modal` is likewise hand-rolled).
  Focus is moved in on open but never restored to the triggering card on close (WCAG: modal pattern).
  The comment box is a raw `<textarea>` even though the create form was swapped to BoardUI (CONFIRMED).
- **Column model is lossy.** `parseColumn` coerces any unknown server state to `backlog`; the current
  column's move button stays enabled (no-op RPC); card shows `commentCount`, not the spec's "last
  comment"; no comments list and **no background runs with status** are rendered at all —
  spec §8.6 requires both, and the RPC surface exposes only `tickets.comment` (append) and
  `runs.list` (by `botId`); assigning a ticket to a bot still does not queue a run (named M11 gap in
  `docs/STATE.md`) (all CONFIRMED).
- One 406-line client component owns phase machine, forms, filter, dialog, and data access.

## Proposals for v0.4.0

### 1. Make the flag-off board actually absent at the HTTP layer
Gate `/app/board/` (and its deep links) in the Rust static fallback next to `PREFIX_FALLBACKS` in
`crates/api/src/router.rs`: when `BOMA_BOARD` is off, serve `404.html` with 404 for those paths,
before the `/app/` prefix fallback can land them on the generic shell. Keep the client probe as
defense in depth. Risks: (a) static export means the gate cannot live in `page.tsx` as a per-request
check — a build-time `process.env.BOMA_BOARD` read would fork the artifact per deployment and is
rejected; (b) touches the shared static handler and its `http_surface` tests — wrong guard ordering
could 404 legitimate deep links; (c) ADR-013 already owns the fallback semantics — this needs an ADR
note, not a silent edit; (d) signed-out behavior finally matches REQ-038, so the Playwright absent-path
test should assert HTTP 404, not just the not-found shape.

### 2. Extract a `useBoard` data layer and split the screen into feature components
One typed hook (`loading | ready | absent | error`) owning the RPC wiring, aborts, the single fetch
path, and merge-on-mutation semantics (no full `load()` reload; fold `ticketsCreate`/`ticketsMove`/
`ticketsComment` replies into state). Split `board-screen.tsx` into `board-column`, `ticket-card`,
`ticket-detail-dialog`, `create-ticket-form` under `components/board/`; `page.tsx` stays a five-line
server delegate. Risks: (a) behavior-preserving refactor only — Playwright REQ-038, goldens, and the
axe suite must stay green with an unchanged DOM/role map; (b) merging instead of refetching can hide
server-side ordering (key order) — re-sort on merge or refetch in background; (c) over-splitting into
context providers — keep it props and one hook, no new state library.

### 3. Error and stale-state honesty
Add a real `error` phase with a Retry button for non-404 failures (today: silent empty board). Derive
the open dialog from `selectedId` + `tickets.find(…)` so comment count / `updatedAt` never drift; if
the ticket vanishes, close the dialog honestly. Disable the move button for the current column.
Risks: (a) new `board.error.load`/`board.retry` keys across 14 locales (fallback rule applies);
(b) Retry needs an accessible announcement path — re-run axe; (c) list-derived selection drops dialog
state if a future filter hides the open ticket — apply the filter to columns only, never to the dialog.

### 4. Close the spec §8.6 gap honestly: comments list + run status
The detail panel must show comments and the background runs attached to the ticket with their status.
v0.4.0 should add the missing procedures — `tickets.comments` (list) and either `runs.list` accepting a
`ticketId`/`trigger` filter or `tickets.runs` — plus `tickets.create`/assignment actually queueing a
`tickets`-trigger run (the named M11 gap; `RunTrigger::Tickets` exists in the domain, the worker claim
path does not). Card "last comment" can ride as a `lastCommentPreview` field on `TicketView` to avoid
N+1. Risks: (a) scope and contract — new RPC surface means `rpc.rs` + `wire.rs` + `rpc_surface.rs` +
OpenAPI regeneration; needs owner sign-off, it is additive, not a DEVIATION; (b) until the worker path
lands, render "no runs yet" honestly rather than a fake progress bar; (c) bot-authored comments need
author labelling and ordering tests; (d) largest item here — sequence it after 1–3 or split across
m6/m7.

### 5. Kanban integrity pass
Stop coercing unknown `state` values to `backlog`: parse into `COLUMNS | "unknown"`, render an explicit
"needs triage" bucket (or quarantine with a visible badge) and log. Keep click-to-move (keyboard- and
SR-friendly); mirror column order in RTL with logical properties and keep the stagger/reduced-motion
behavior from REQ-057. Optional: keyboard-operable move menu on the card. Fix focus restore to the
invoking card on dialog close, and swap the raw `<textarea>` for BoardUI `Textarea` to match rc5.
Risks: (a) an unknown-state bucket is visible product surface — needs catalogue keys and an empty-state
decision; (b) RTL column order with `xl:grid-cols-4` — logical styles only, verify `dir=rtl` Playwright
path (known local RTL artefact is green on CI only); (c) more motion on cards re-triggers the REQ-057
animation-name assertion — update tests, don't weaken them; (d) drag-and-drop is explicitly out of
scope (keyboard parity cost, no spec demand).

## Explicit non-goals

No drag-and-drop, no realtime sync, no multiplayer cursors, no BoardUI Pro components, no spec edit, no
change to the flag default (off), no public API change beyond the additive procedures in #4. Verify with
the REQ-038 Playwright spec, the axe suite, and a fresh rpc_surface/OpenAPI diff; mark every claim in
docs/STATE.md CONFIRMED or GUESS as today.
