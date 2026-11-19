# 0041 — continue-in-a × onboarding parity proposal (Boma Bot)

- Brief: id 41, area `continue-in-a`, lens `onboarding` (`docs/openbot-swarm/briefs-1000.jsonl`).
- Status: **PROPOSAL only**. Product code, spec, goldens and the RPC surface are untouched.
- Provenance: the upstream tree was read as **data** at `/tmp/opencode/openbot-parity` (HEAD `559d5ac`).
  The brief's area slug is a truncated fragment of a "Continue in a…" phrase; the pinned tree has no literal
  string by that name, so this is anchored on the HEAD-verified onboarding capability of the same family:
  first run lets an account that already belongs somewhere **continue there** instead of redoing setup.
  No upstream code, wording, identifier or brand enters Boma Bot; no Pro component.
- **CONFIRMED** = read this session; **GUESS** = owner's call.

## 1. The upstream capability, in my own words

CONFIRMED (`FirstRunFlow.tsx`, `FirstRunExistingServers.tsx`, `server-selection.tsx`, `analytics.ts`,
`AppView.tsx`, commit `559d5ac`; `docs/web-client.md`):

- A new or wiped-clean desktop signs in; the account already owns remote servers (refreshed by the host
  directory sync at sign-in; this computer's local servers excluded). The first question is replaced by a
  "welcome back" decision — not an extra step: pick an existing server to continue in it, or "Start new".
  Shown only before the analytics choice is touched, only when an existing remote server is known.
- Each row is a ghost button: server mark, name, a state-dependent second line (offline servers read "it
  opens when it is on"), chevron; while opening, `aria-busy` + spinner, siblings disabled. On open: previous
  selection remembered, target selected, setup saved with path `existing`; on save failure the previous
  server is re-selected and an inline error shows (the old silent-stick-to-remote behaviour was the bug behind
  this commit). `onboarding_step`/`onboarding_completed` carry a closed `path` enum extended with `existing`;
  no free text is sent.
- Same family at the same HEAD: the phone-browser interstitial pins a "continue in browser" escape,
  remembered per-browser, skipped by link/invitation addresses; invitations read "paste an invite link to continue".

The essence: onboarding is not a one-way new-tenancy funnel — arrival offers continuation into where the
account already lives, with an explicit start-new alternative, failure that never strands nor double-creates.

## 2. Why onboarding is the deciding lens

CONFIRMED — `/onboarding/:step?`'s access column reads "signed in, no space yet" (spec/08:27); §8.2 (spec/08:66-69)
defines three skippable steps ending on `/app/:botId`. Nothing contemplates a **returning** account: it always
funnels to `spaces.create`, minting a *second* space for someone who already belongs to one. `/invite?token=` is
the only surface today continuing a newcomer into an existing space (spec/08:61-64). REQ-042 (spec/01:340-346)
owns membership; §8.1 (spec/08:40) forbids unlisted screens — so the parity move lives inside `/onboarding/:step?`.

## 3. Where Boma Bot stands

- CONFIRMED — the wizard (`apps/web/app/onboarding/[step]/page.tsx` → `components/onboarding/onboarding-screen.tsx`)
  ships steps 1 `spaces.create`, 2 `bots.create`, 3 `models.*`; bookmark `boma-onboarding` is versioned and
  self-healing; it never writes the client's space pointer.
- CONFIRMED — `apps/web/lib/api/rpc.ts:194-210`: `getSpaceId()` reads `localStorage["boma-space-id"]`, but
  **no writer exists anywhere**; reload falls back to the caller's default space (`resolve_actor`,
  `crates/app/src/usecases.rs`); a foreign/removed space answers *not found*, never *forbidden* (REQ-043).
- CONFIRMED — no "list my spaces" read exists: `spaces` serves only `create`/`invite` (`rpc.rs`
  `procedures()`); `admin.members.list` is own-space; infra has `spaces_of` (`crates/infra/src/stores/spaces.rs`,
  AccountExport path) with no client-facing procedure.
- CONFIRMED — web onboarding tests are REQ-042-named; REQ-042's row is in `docs/TRACEABILITY.md:62`;
  invites anchor REQ-042 end-to-end (`components/invite/accept-screen.tsx`, `GET /api/v1/invites/{token}`).

**Gap:** a returning user with existing memberships is never offered continuation; the wizard always offers a
new space, and the chosen space does not survive reload.

## 4. Proposed shape — continue in a space you already belong to

1. Step 1 gains a conditional panel **"Spaces you're in"** above the name form — rendered only when the (new)
   read answers ≥1 row; otherwise absent, step 1 exactly as today. No new route (§8.1), no fourth step (§8.2).
2. Each row is a full-width ghost button: name (+ role), label "Continue in {name}". Activation clears the
   `boma-onboarding` bookmark, **writes `localStorage["boma-space-id"]`**, routes to `/app/` — no intro re-run.
3. Failure honesty: a pick answered absent (removed member mid-session, REQ-042) keeps the panel, shows a
   `role="alert"` line, stays on step 1. Never auto-creates a space, never clears sibling rows, never reveals
   existence beyond what the caller already knew.
4. The missing writer lands in two more places: after `invitations.accept`, and on finish (step 3) — today
   both strand the choice in memory. No `is_default` write, no change to REQ-042 evaluation.
5. Wire half (GUESS, owner-gated): one caller-scoped read `spaces.list` → `{ id, name, role, isDefault }[]`,
   ordered, reusing `spaces_of`. Wire change: PARITY/STATE entry + goldens + approval first. If refused, the
   slice ships presentation-only (steps 2–4), panel hidden behind the stub; the debt is recorded.
6. Catalogue keys `onboarding.existing.*` in all 14 locales (REQ-057); own words, English fallback.
7. Non-goals: no shell space switcher, no presence badge (we have no presence), no app-store interstitial
   (no mobile app; REQ-056 deferred), no analytics (the web client has none), no change to REQ-017/042/043,
   no invite-semantics change.

## 5. A11y / RTL / motion

- **A11y:** panel is a named `role="group"` with a visible legend; rows are real `<button>`s ≥44 px
  (`.boma-target`), name "Continue in {name}" (+ role hint), `aria-busy` + polite status while opening,
  siblings disabled; failure in `role="alert"` (text + icon); focus ring (`.boma-focus`); Tab/Enter/Space;
  zero critical/serious axe findings in light, dark and 360 px.
- **RTL:** logical `ms/me`, `ps/pe`, `text-start/end`, `border-s/e`; chevron flipped under `dir="rtl"`; space
  names (user content) in `<bdi dir="auto">`; URLs/e-mail stay LTR; `DirectionProvider` already wraps `/onboarding`.
- **Motion:** none on rows; panel appears with the page; any first-paint skeleton ≤120 ms fade;
  `prefers-reduced-motion` drops it (BoardUI globals); never animate a value being read (spec/08:191-200).
- **Tokens (free BoardUI only; CONFIRMED docs/UI.md + AGENTS.md rules):** `button`, `direction`; surfaces
  `bg-background-secondary-default` (+ `-hover`), `bg-background-full`; `text-text-primary|secondary|tertiary`,
  `border-separator-border`, `border-border-focus-ring`, `text-foreground-icon-primary`,
  `text-text-error-primary`; composites `text-headline-medium`, `text-body-medium`, `text-body-2-medium`,
  `text-caption-1-semibold`; merge with `cx()`; icons from `@remixicon/react` as references. No palette/hex,
  no `dark:` literals, no second design system, no Pro.

## 6. One REQ-named Playwright test

`test("REQ-042: a returning newcomer continues in an existing space, is alerted on mid-session removal, and mirrors in RTL", …)`
in `apps/web/tests/onboarding.spec.ts`:

- Stub the read to two memberships; `goto("/onboarding/1/")`; assert panel + rows; axe clean.
  Tab → Enter the second row: assert `localStorage["boma-space-id"]` equals the picked id, `page` lands on
  `/app/`, and **zero** `**/rpc/spaces/create` calls fire.
- Re-stub the pick to fail (removed membership): assert a `role="alert"` line, still on `/onboarding/1/`,
  and no `spaces.create` was attempted.
- `/invite?token=…` signed in: accept fires once, `boma-space-id` is set, `next` respected.
- `localStorage["boma-locale"]="he"` + `reducedMotion:"reduce"`: `dir="rtl"`, translated legend, mirrored
  chevron, clean axe, no entrance animation.

`tools/trace_check.py` counts the declaration; the REQ-042 row in `docs/TRACEABILITY.md:62` is extended only
after the test exists.

## 7. First slice (test-first, smallest complete loop)

1. Write the failing REQ-042 test (stubbed read).
2. Client-only: the two `boma-space-id` writers, the step-1 panel behind the stub, `onboarding.existing.*`
   keys ×14, `<bdi>`, logical classes, axe pass. No server change.
3. Wire half (GUESS, owner gate): `spaces.list` → `usecases::list_my_spaces` over a ported `spaces_of` →
   rpc/wire + golden; STATE entry **first**; PARITY note. If refused, ship slice 2 with the panel hidden and
   record the debt in `docs/STATE.md`.
4. Gates unchanged: web typecheck/lint/build, Playwright onboarding+invite+a11y, 360 px, dark;
   cargo/`trace_check.py` only when the wire lands.

## 8. Evidence

CONFIRMED — upstream HEAD `559d5ac`: `FirstRunExistingServers.tsx`, `FirstRunFlow.tsx` (`existingShown`,
`openServer`, `startNew`, `existing` path tag), `server-selection.tsx` (`openServerDuringSetup` restores the
previous server), `analytics.ts` (closed enum), commit `559d5ac`. Ours: spec/08 §8.1 (:19-43), §8.2 (:45-69),
§8.9 (:170-200); spec/01 REQ-042 (:340-346); `crates/app/src/usecases.rs` (`resolve_actor`),
`crates/infra/src/stores/spaces.rs` (`spaces_of`), `crates/api/src/rpc.rs` (`spaces.create|invite` only),
`wire.rs:1879-1900`; web: `app/onboarding/[step]/page.tsx`, `components/onboarding/onboarding-screen.tsx`,
`components/invite/accept-screen.tsx`, `lib/api/rpc.ts:132-210`, `tests/onboarding.spec.ts`;
`docs/TRACEABILITY.md:62`.
GUESS — `spaces.list` name/shape, `onboarding.existing.*` wording, role hint line, step-1 panel vs a step 0,
the 404-on-pick stub shape.
