# 0021 — contact-us-in × onboarding parity proposal (Boma Bot)

- Brief: id 21, area `contact-us-in`, lens `onboarding` (`docs/openbot-swarm/briefs-1000.jsonl`).
- Status: **PROPOSAL only**. Product code, spec, goldens and RPC surface are untouched.
- Provenance: the brief's upstream DATA-ONLY clone path was empty in this session; the analysed
  upstream tree (`re-workspace/target/openbot`) was read as **data** instead, per AGENTS.md. No upstream
  code, asset, wording, address or brand is carried over; no Pro component is used.
- **CONFIRMED** = read from spec/code/this session; **GUESS** = recommendation needing the owner's call.

## 1. Upstream capability, in my own words

- A signed-in app offers a low-key "reach the operator" strip near the top of the shell and, for a
  first-run local evaluation, a dedicated onboarding step to arrange a walkthrough or scope a trial.
  It is a supporting channel, never an agent approval or side effect (CONFIRMED: upstream CHANGELOG
  4191-4212, 4772; `app/src/components/layout/self-host-banner.tsx`; `app/src/routes/_authed/onboarding.tsx`).
- It appears only after deployment capability **and** per-person dismissal state are known, so it never
  flashes on and off; a deployment serving its own people hides it by config, and a paid plan hides
  the promotional variant. Restated generically: opt-in by deployment, not by the end user's rights.
- Dismissal is persisted per user across devices with optimistic removal and rollback if the save
  fails. A slow or failed scheduling load degrades to a plain outbound link plus a status notice;
  a skip action advances onboarding without contacting anyone. A failed capability lookup must neither
  show the channel to an organization's employee nor strand an evaluator: step selection and saved
  progress are decided once on arrival and retried silently on the next move.
- This is a shell/onboarding copy-and-gating concern, not a transport, permission or data-format one.

## 2. Where Boma Bot stands

- CONFIRMED — `/onboarding/:step?` is three independently skippable steps (space name, first bot +
  one-sentence purpose, model), resumes on reload, finishes on `/app/:botId` with intro streaming;
  `/invite?token=<token>` shows space name + inviter, signs in or creates the account, then accepts,
  with a specific expired/revoked message and a request-new path; token is a query param because the
  app is a static export (spec/08 §8.2:61-69, ADR-013).
- CONFIRMED — REQ-042 governs space membership and token-bearing, expiring invitations: a different
  address is refused and a removed member is unauthorized (spec/01:340-346). Live RPCs already cover
  the path: `spaces.create`, `invitations.accept`, `admin.invitations.list/revoke` plus `bots.create`,
  `models.connect`, `models.preferences.get/set` (`crates/api/src/rpc.rs` §§namespaces/procedures).
- CONFIRMED — `/api/auth/capabilities` exists but advertises auth/billing/`localFirst` only; there is
  **no** support field, no support/ticket RPC and no generic user-preferences procedure (spec/openapi.yaml:81-106).
  A secondary `Support` icon item with no destination is already in the sidebar
  (`apps/web/components/application/dashboard/dashboard-sidebar.tsx:255,533-534`) — a hook, not a channel.
- CONFIRMED — REQ-057 requires 14 locales and real RTL including e-mail; every string must go through
  `t()` with English fallback (spec/01:465-471; docs/UI.md:101-126).
- **Gap:** onboarding and invite acceptance have no help affordance, and the placeholder label is a
  hardcoded literal. A real contact transport, config field or cross-device dismissal changes wire
  behaviour and needs a new REQ id, ADR and PARITY/STATE entry before any code (owner rule).

## 3. Onboarding change (presentation only)

1. Add a compact help strip below the primary action on step 1 (space naming) and on the invite-accept
   screen — **not** a fourth step. Keep the §8.2 route map; never cover the space/invite form. With no
   operator-configured contact target, render nothing (no dead affordance).
2. Content: one plain sentence, a contact action and a dismissal action, all from `onboarding.help.*`
   ICU keys; target is deployment-supplied config (GUESS: `supportUrl`), never an upstream address.
   Skipping or closing never changes space creation, membership or invitation acceptance; REQ-042 stays
   authoritative. No agent side effect, approval or run is triggered from this strip.
3. Render only once capability + dismissal state are resolved, to avoid a flash (upstream rule). Until a
   state source exists, dismissal is session-local (GUESS); a failed/unknown help lookup hides the strip
   rather than blocking the flow. Never gate REQ-042 steps on help state.
4. Privacy: the invite token and space id stay out of help links, logs and analytics; keep the token in
   the route query as §8.2 requires. Any correlation context is opt-in in operator config.
5. RPCs unchanged in this slice: `spaces.create`, `invitations.accept`, `admin.invitations.*`,
   `bots.create`, `models.connect`; no new procedure, field or brand enters the contract.

## 4. Non-goals

No upstream wording/brand/asset, Pro, second design system or outbound ticket/mail worker; no persisted
cross-device dismissal, no upsell/plan logic, no change to REQ-042 evaluation. Those stay proposals in
`docs/STATE.md` until the owner grants a REQ id + ADR; any wire change is a DEVIATION in `docs/PARITY.md`.

## 5. A11y / RTL / motion

- **A11y:** strip is a named `complementary` region; dismissal is a named icon button ≥44 px
  (`.boma-target`), contact action is a real button/link with a descriptive name; Tab/Enter operable,
  Escape dismisses, focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring` / `.boma-focus`;
  status/notice in the polite live region (spec/08 §8.9); zero critical/serious axe findings in light
  and dark; failure shown as text + icon in `text-text-error-primary`, never colour alone.
- **RTL:** keep the existing `DirectionProvider`; use `ms`/`me`, `ps`/`pe`, `text-start`/`text-end`,
  `border-s`/`border-e`; mirror the dismissal glyph and any horizontal strip motion; keep URLs, ids and
  e-mail in `<bdi>`/`dir="auto"`; never mirror charts/canvas (spec/08 §8.9; docs/UI.md:116-126).
- **Motion:** a 120–180 ms fade only, `prefers-reduced-motion` drops movement via BoardUI globals; no
  animation on streaming content or while the notice is being read.
- **Tokens (CONFIRMED AGENTS.md rules / docs/UI.md):** installed free `button`, `direction`;
  `bg-background-secondary-default`, `bg-background-full`, `text-text-primary`, `text-text-secondary`,
  `border-separator-border`, `border-border-focus-ring`, `text-foreground-icon-primary`,
  `text-text-error-primary`; composites `text-body-medium`, `text-caption-1-semibold`; `cx()` from
  `@/utils/cx`; icons by reference from `@remixicon/react`. No raw palette/hex, no `dark:` literals.

## 6. One REQ-named Playwright test

`test("REQ-042: onboarding help strip never blocks space creation or invite acceptance, is dismissible and mirrors in RTL", …)`
in `apps/web/tests/onboarding.spec.ts`:
- GIVEN a signed-in newcomer on `/onboarding/1` with capabilities resolved, WHEN the strip renders,
  THEN it has an accessible name, clean axe, and an operator-configured target; keyboard Tab/Enter
  dismissal hides it, `spaces.create` fires once with the entered name (intercept `**/rpc/spaces/create`)
  and the step advances.
- GIVEN `/invite?token=…` signed in as the invited address, WHEN accepted, THEN `invitations.accept`
  fires once, membership is granted, and the token appears in no help link or final URL; with no
  configured target the strip is absent. A wrong address is still refused (REQ-042).
- Set `localStorage["boma-locale"]="he"` before load: assert `dir="rtl"`, logical classes, clean axe;
  with `reducedMotion:"reduce"` assert no entrance animation.

## 7. First slice (test-first, small)

1. Write the failing REQ-042 test above (onboarding step 1 + invite accept, seeded capability state).
2. Add `onboarding.help.*` keys to all 14 catalogues and render the strip/skip; no transport.
3. Apply logical utilities and focus/axe fixes; keep session-local dismissal only; replay shell goldens.
4. Record config field, persisted dismissal and any contact backend in `docs/STATE.md` as proposals.
5. Gates: web typecheck/lint/build, Playwright onboarding+invite+a11y clusters, 360 px, dark mode;
   update `docs/UI.md` for any component edit; no new dependency (`docs/DEPENDENCIES.md` untouched).

## 8. Evidence

CONFIRMED — spec/08 §§8.1, 8.2, 8.9; spec/01 REQ-042, REQ-057; spec/openapi.yaml:81-106;
`crates/api/src/rpc.rs` namespaces/procedures; `apps/web/components/application/dashboard/dashboard-sidebar.tsx`;
docs/UI.md:101-126; upstream CHANGELOG:4191-4212,4772 and `app/src/components/layout/self-host-banner.tsx`,
`app/src/routes/_authed/onboarding.tsx`, `app/src/lib/settings/message-list.ts` (data only).
GUESS — `supportUrl` shape, session-local dismissal, notice wording.
