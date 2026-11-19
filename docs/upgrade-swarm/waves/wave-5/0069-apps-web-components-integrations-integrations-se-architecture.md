# 0069 — apps/web/components/integrations/integrations-setup.tsx — architecture lens

- Target: `before-m5` (v0.3.0-rc5), module `apps/web/components/integrations/integrations-setup.tsx` (211 lines, one `"use client"` component).
- READ-ONLY review. Spec anchors: `spec/08-ui-ux.md §8.7` (`/integrations/setup`: confirm the connection, **choose which bots may use it**, and show a partial-install report rather than a green check), REQ-032 (catalogue, space-wide tool exposure), REQ-034 (no-local-code rule, skipped parts named with reasons), REQ-048 (messaging install binds one bot at install time), BoardUI rules (components first, semantic tokens, composite type, `cx`).

## Structure today

One `IntegrationsSetupScreen` owns everything: `phase: "loading"|"ready"|"partial"`, three arrays (`connections`, `marketplace`, `installations`), and a `justInstalled` set derived on every render from the `?installed=` query param. One `useEffect` fires `Promise.allSettled([connectionsList, marketplaceList, messagingList])` under an `AbortController`, fills whatever fulfilled, and latches `partial` when any installed plugin has skipped parts. Render is header → optional confirmation banner → optional messaging/plugins/connectors sections, each an inline `<ul>` of inline `<li>` rows. All colors are semantic tokens, type is composite utilities, all static strings go through `useTranslation`. **Keep that vocabulary verbatim**: `text-text-primary/-secondary/-tertiary`, `text-foreground-icon-secondary`, `bg-background-primary-default`/`-secondary-default`, `border-border-button-default`, `border-separator-border`, `text-title-3-semibold`, `text-headline-medium`, `text-body-medium`, `text-body-2-medium`, `boma-target boma-focus`.

## Confirmed observations (read this session)

- **§8.7 half-implemented, divergence only in a comment**: there is no "choose which bots may use it" control; the header comment argues the gate does not exist (messaging binds one bot at install time; connectors/plugins are space-wide). That may be the right product truth, but it is recorded in code, not in `docs/PARITY.md`. CONFIRMED.
- **No error state, hence a possible false negative**: all three fetches can reject; `allSettled` keeps arrays empty and `phase` becomes `ready`, producing a near-blank page indistinguishable from "nothing installed". The header promise "nothing here is a false negative" does not hold. No retry affordance. CONFIRMED.
- **Two unreconciled sources of truth**: confirmation comes solely from the URL param (`justInstalled`), while the real install list comes from the RPCs; they are never cross-checked. Raw platform strings are joined with a hardcoded `", "` and rendered as the "installed" claim, so a hand-edited URL can assert an install. `workspaceName` exists in `MessagingInstallation` but is shown only in the list, not used to confirm what the platform just reported. CONFIRMED.
- **Derived state stored as state**: `partial` is a pure function of `installedPlugins.some(...)`, latched into `phase` and preserved by a functional `setPhase`. With no reload today it works, but it will go stale on any future refetch. CONFIRMED.
- **Overlapping catalogue data**: `marketplace.list` also returns `connectors` (`rpc.ts:1108`), but the screen renders only `connectionsList`. Two endpoints carry the same `ConnectorView[]`; authority is implicit. CONFIRMED.
- **Weak skipped-part contract**: `plugin.skipped` is `Array<Record<string, unknown>>` (`rpc.ts:1102`) and the screen probes `part["part"]`/`part["reason"]` with inline casts. REQ-034's "named with reason" assurance rests on untyped data. CONFIRMED.
- **External link not scheme-checked**: `href={plugin.source}` is rendered straight from marketplace data; `noopener/noreferrer/nofollow` is set, but a `javascript:` source would still be clickable. CONFIRMED (risk; GUESS whether the backend validates sources).
- **Monolith + duplication**: fetch/phase/banner/three sections in one file; the `loading|ready + AbortController + skeleton` pattern is already duplicated in `activity-screen.tsx` and `admin/access-screen.tsx`. Dead import: `useRouter`/`router` unused. CONFIRMED.
- **Accessibility/RTL gaps**: no `role="alert"`/empty-state message, `aria-busy` skeleton has no announcement, `<code>{installation.botId}</code>` and the plugin source URL have no wrap/`dir="auto"` handling at 360 px. CONFIRMED (markup read; GUESS on rendered overflow).

## Proposals (v0.4.0)

1. **Decompose into `useIntegrationsSetup()` + four section components.** Extract a hook owning the three fetches, abort controller, data arrays, and banner derivation; extract `installed-banner.tsx`, `messaging-section.tsx`, `plugins-section.tsx`, `connectors-section.tsx` as siblings under `components/integrations/`, leaving the shell/header in the screen. Return one hook object and keep markup byte-identical, with `<bdi dir="auto">` around platform names/bot ids. Risk: prop-drilling and golden churn — verify the existing REQ-032/034/048 Playwright coverage by name before refactoring (GUESS: those tests exist).

2. **Make the state machine honest: add `error` + `empty`, derive `partial`.** Replace the `loading|ready|partial` latch with `loading|ready|error`, compute `partial` via `useMemo` from installed plugins, and render a `role="alert"` retry panel when every fetch rejects and a short "nothing installed yet" message when all lists are legitimately empty. Risk: wording must stay catalogue-driven (`integrations.error`, new `integrations.empty`) with fr/he entries; test the all-reject path explicitly so the retry does not reintroduce a skeleton flash.

3. **Reconcile the confirmation with server state and §8.7.** Treat `?installed=` as a hint only: cross-check it against `messagingList`/`marketplaceList` and prefer the server-returned `platform` + `workspaceName` in the banner; announce via `role="status"`. Then either add the "choose which bots may use it" gate or record a DEVIATION in `docs/PARITY.md` explaining the bind-at-install/space-wide model. Risk: a real per-bot gate is a contract + UI change (spec edit, RPC support, trace, maintainer sign-off); recommend documenting the deviation now and gating the feature behind v0.4.0.

4. **Type and validate the data boundary, not the render.** Replace `skipped: Array<Record<string, unknown>>` with a typed `SkippedPart { part: string; reason: string }` in `rpc.ts` (generated from the OpenAPI document per stack policy) and validate `plugin.source` as `https:` at the RPC boundary. Pick one authoritative catalogue source (`connectionsList` or `marketplaceList.connectors`) and drop the other from this screen. Risk: client type change must mirror `spec/openapi.yaml` — a mismatch is a DEVIATION; dropping an endpoint call changes request volume but not visible behavior.

5. **Accessibility and completeness polish.** Add a labelled loading announcement, wrap bot ids/source URLs with `break-words`/`dir="auto"`, keep `boma-focus` rings on both links, and ensure layout survives 360 px in light/dark with all three sections and the banner present. Risk: class changes churn goldens — re-record intentionally and re-run axe; never swap semantic tokens for raw colors.

## Explicit non-goals

No per-bot assignment UI until §8.7 is reconciled and spec-gated. No client-side authorization. No plugin submission/approval flow (that is `/admin/plugins`). No BoardUI Pro components. No change to the written-out skipped-part reason text (REQ-034). No new catalogue fields beyond the existing `ConnectorView`/`PluginInstallView`/`MessagingInstallation` shapes without a contract change.

## Suggested sequencing

3 (deviation note + hint/server reconciliation) → 2 (honest phases) → 4 (typed boundary) → 5 (accessibility) → 1 (mechanical split last, once behavior is settled). Every step gated by `lint`, `typecheck`, axe at 360 px in light/dark, and the REQ-032/034/048 Playwright tests.
