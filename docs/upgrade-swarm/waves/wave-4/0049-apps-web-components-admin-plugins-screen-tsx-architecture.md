# 0049 — apps/web/components/admin/plugins-screen.tsx — architecture lens

Worker #049 · module `apps/web/components/admin/plugins-screen.tsx` (168 lines, one `"use client"` component) · lens: architecture · READ-ONLY review of `before-m5` (v0.3.0-rc era). Proposals only; no code changed here. Reliability/error-path detail is out of scope (sibling worker). Spec anchors: `spec/08-ui-ux.md §8.7` (`/admin/plugins` = submissions queue with source reference, submitter contact, review state, refused parts + reason; approving never executes code), `spec/01-requirements.md` REQ-058 (SHOULD; admin-only procedures refused for non-admins; state changes + submitter notified), BoardUI rules (components first, semantic tokens, composite type, `cx()`), ADR-003 (hand-written RPC mirror pending codegen), ADR-013 (static export).

## Structure today

`PluginsScreen` is the whole review queue in one function body. Four jobs share one scope: (a) fetch (`adminPluginsList({ status: "pending" })` under one `AbortController`, `reloadToken` refetch trigger, `loading|ready|error|forbidden` phase machine); (b) mutation (`adminPluginsDecide`, optimistic row removal on success, single `deciding` id, one `actionError`); (c) chrome (header/nav, h1, skeleton, forbidden/error/empty/list, action banner); (d) state. There is no seam between "review queue as a use case" and "queue as pixels", and no test surface short of Playwright route interception.

Confirmed observations (read this session):

- **Admin chrome duplicated verbatim.** `access-screen.tsx` repeats the same header/nav (`boma-target boma-focus`, `max-w-5xl`, `app.name` + title + sibling link), `Phase` union, `reload` callback, skeleton pair, and forbidden/error blocks. `app/admin/` has no `layout.tsx`, so both pages re-declare identical shell markup and phase logic instead of composing a segment boundary.
- **Forbidden detection is structural duplication.** Inline `error.code === "FORBIDDEN"` object cast appears in both screens — knowledge of the RPC error shape copied into two components; `lib/api/rpc.ts` exports no typed guard.
- **Contract is a hand-written mirror.** `AdminPluginSubmissionView.status` is typed `string` while the server enforces a six-value union (`pending|unconfirmed|approved|rejected|applied|revoked` per the earlier page review); `createdAt` is carried but never rendered. Nothing fails CI when `wire.rs` moves.
- **§8.7 view gap is partly deliberate, partly implicit.** No submitter contact and no refused-parts/reason data in the view (migration 0027 deliberately never returns contact; refused parts live on a different, pre-install flow). The gap is documented in a code comment, not reconciled in `docs/PARITY.md` as a DEVIATION.
- **Catalogue keys exist but are unused.** `admin.plugins.queue`, `.name`, `.source`, `.status`, `.loading` are defined in `lib/i18n.ts` (en/fr/he) yet the body renders raw `submission.status` with no labels and an `aria-hidden` skeleton with no `loading` announcement. This screen's copy is otherwise fully through `useTranslation()` — keep that.
- Tokens are honest throughout: `bg-background-full`, `bg-background-secondary-default`, `text-text-primary/-secondary/-tertiary/-error-primary`, `border-separator-border`, `border-border-button-default`, `border-border-error-default`, `text-foreground-icon-primary`, composite type utilities, `rounded-3xl`/`rounded-md`, `<code dir="auto">` text-only source, `role="alert"`. Keep all of it.

## Proposals (v0.4.0)

### 1. Extract `usePluginReviewQueue` + presentational parts (data/pixels seam)
Move fetch, phase machine, reload, and decide into one hook exposing `{ submissions, phase, decide, reload, deciding, actionError }`; split JSX into `AdminNav`, `SubmissionRow`, `QueueList`/empty state, `ActionError`. `PluginsScreen` becomes a thin composer; `page.tsx` stays the thin wrapper it already is. Order: move first, redesign never, in one PR — keep i18n keys, attribute names, and the `deciding` → row-removal sequence byte-identical so the REQ-058 Playwright test still passes. Risk: prop-drilling creep (pass the hook object as one prop, not six) and golden drift; replay goldens + axe after the split.

### 2. Give `/admin/*` one boundary: `app/admin/layout.tsx` + `AdminShell` + generic `useAdminQueue`
Extract only the *identical* chrome and phase machine into a server-safe `layout.tsx` (ADR-013 static export) plus `AdminShell` with per-screen body slots; parameterize the hook by fetcher so `/admin/plugins` and `/admin/access` consume the same `loading|ready|error|forbidden` + abort + retry logic. This is the largest duplicated block and the clearest structural win. Risk: the two screens legitimately differ (3-section vs 2-button rows) and goldens assert exact markup — extract chrome/phase only, keep bodies as slots; verify `boma-target boma-focus` ring order and the 360 px light/dark axe runs. Do **not** add a runtime route guard here without an ADR (static export cannot assert roles).

### 3. Type the review contract and centralize the RPC error guard
Narrow `AdminPluginSubmissionView.status` to the server union and add `isRpcError(error, "FORBIDDEN")` to `lib/api/rpc.ts`, replacing both inline casts. Keep per-screen catalogue keys at the call site (`admin.plugins.forbidden` vs the access-screen key) so wording stays decoupled. Pair with a cheap field-for-field snapshot test of the admin TS shapes against `wire.rs` (ADR-003's codegen end state). Risk: any wire change is a DEVIATION — owner sign-off + `docs/PARITY.md` entry; false-positive diffs from serde attributes; snapshot test is the safe first step, full codegen a separate ADR.

### 4. Reconcile the §8.7 view gap explicitly (contract PR, owner-gated)
Either (a) render the already-carried `createdAt` and add labels from the unused catalogue keys, and surface submitter-contact presence (`on file: yes/no`) / refused-parts reason if the wire ever provides them, or (b) file a DEVIATION in `docs/PARITY.md` + a decision in `docs/STATE.md` that contact and refused-parts ship later. Do not silently widen the wire for submitter contact — migration 0027 is a deliberate privacy boundary. Risk: (a) needs server support for refused parts (new wire fields = ADR + contract test); (b) is cheap but needs maintainer veto. Recommend (b) now, (a) behind the v0.4.0 contract gate.

### 5. Make the catalogue and semantics the single source of truth in the body
Use `admin.plugins.queue/.name/.source/.status` for row/column labels and wire `admin.plugins.loading` to an `aria-live="polite"` region (the current skeleton is `aria-hidden` and announces nothing). Keep `<code dir="auto">` as text-only source (never rendered HTML) and keep status text LTR-isolated under RTL since en/fr/he catalogues are all present. Risk: golden snapshot churn and axe focus-order differences; re-record goldens intentionally, preserve the focus ring and the "approving never executes" copy unchanged.

## Explicit non-goals
No client-side authorization (server `require_admin` is authoritative; the screen only reports FORBIDDEN). No code execution on approve. No new admin screens or BoardUI Pro components. No hand-built lookalikes for installed BoardUI components. No change to the atomic server decide (`UPDATE … WHERE status='pending'`) or to `admin.plugins.*` key names. Reliability fixes (abortable mutations, background revalidate) belong to the sibling worker.

## Sequencing & gate
1 → 2 (one structure PR theme) → 3 → 4/5 (owner-gated). Every step: REQ-058 Playwright suite green, axe AA clean at 360 px in light/dark and `dir=rtl`, goldens replayed, `page.tsx` diff ≤ thin wrapper, BoardUI semantic tokens only, `cx()` for merges, icons from `@remixicon/react` as component refs.

CONFIRMED by reading this session: `plugins-screen.tsx`, `spec/08-ui-ux.md §8.7`, `spec/01-requirements.md` REQ-058, `lib/api/rpc.ts` (admin namespace), `lib/i18n.ts` (en/fr/he keys). Contract/spec-gap detail (six-value status union, migration 0027 privacy intent, refused-parts flow) inherited from the earlier page review — GUESS until re-verified against `wire.rs`/ADR-003.
