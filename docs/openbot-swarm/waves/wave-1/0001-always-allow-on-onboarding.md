# 0001 — Always-allow-on through the onboarding lens

- Brief: id 1, area `always-allow-on`, lens `onboarding`. Inspiration only; product code untouched.
- Status: **proposal**. The upstream tree was read as data only — no code, asset, copy or brand carried over.
- **CONFIRMED** = read from §8; **GUESS** = recommendation needing the owner's decision.

## 1. Upstream capability (own words)
Each agent has a standing autonomy level. By default an action that changes files, runs a command,
publishes a page or calls a tool raises a pending card and waits. At the permissive level those effects
run without a card. Between them sits a one-shot consent on the card: while it waits, a person can turn
that single answer into a standing grant. The grant moves the agent to the permissive level, resolves
every card already parked for it, and keeps the conversation instead of opening a new session.

The grant is bounded. The provider retains it per tool and per server account, so it survives a restart;
the product's own export leaves it out, and an import downgrades it to a warning so a new agent never
starts elevated. A remote admin changing the level clears waiting cards the same way. Some deployments
force the level and block narrowing; computer-use consent is accepted once and never stored. Forms,
sign-in and takeovers are never absorbed. In short: an explicitly confirmed, reversible transfer of a
repeated decision into a standing default, with safe import and no silent escalation.

## 2. Where Boma Bot stands
- CONFIRMED — `/onboarding/:step?` is three skippable steps (space, first bot + one-sentence purpose,
  model), resumes on reload, finishes on `/app/:botId` with the intro streaming (spec/08 §8.2, REQ-042).
- CONFIRMED — `bots.create` makes the bot, thread and configured intro run (spec/03 §3.1); its spec input
  is `{name, description?, instructions?, avatarSet?, color?, sectionId?}`. `spaces.create`,
  `models.connect`, `threads.answer`, `models.preferences.set` are live; `onboarding`, `approvalRules`
  and `preferences` are declared namespaces with no live procedures (`crates/api/src/rpc.rs procedures()`).
- CONFIRMED — per-space/per-user policy decides which calls need approval; the card parks the effect,
  never freezes the chat (REQ-017, REQ-007). No grant field, autonomy level or card affordance exists.
- CONFIRMED — `docs/STATE.md` schedules **M16 scoped auto-approvals**; a new REQ id, ADR and PARITY
  DEVIATION need owner approval before transport changes ship. Export/import exists (REQ-044).

**Gap:** the newcomer's first real decision is an unrequested security decision — the intro run can park
its first write on a card before the approval model is explained, and no standing grant exists.

## 3. Onboarding change
1. **Explain once, safely.** Step 2 gets a two-sentence approval-model explainer (what happens, how to
   reverse) beside the bot form, via `onboarding.*` i18n keys (docs/UI.md "Copy conventions"). Keep the
   three steps and route map: spec/08 §8.1 — anything unlisted is not a screen.
2. **Explicit opt-in.** A labelled two-option choice — "Ask before this bot acts" (preselected) and
   "Let this bot act without asking" — from installed free BoardUI `select`/`switch`. The permissive
   option opens a confirmation `alertdialog` naming the covered effect class and the way back. Skipping
   keeps the safe level.
3. **One applied value.** Stored on the first bot, read by its approval decision only; REQ-017 policy,
   exactly-once effects/answers (REQ-007/017), spending (REQ-018) and tenancy (REQ-043) stay
   authoritative. Never covers sign-in, takeover or foreign spaces.
4. **Card closes the loop (follow-on).** The same grant is offered on the first REQ-007 card only at the
   safe level, with the same confirmation, clearing cards already waiting for that bot.
5. **Conservative import (GUESS).** Imported bots resume at the safe level; an imported grant becomes a
   warning needing fresh confirmation. No locked/forced permissive mode — narrowing stays possible.

## 4. Non-goals
No Pro component, second design system, vendored asset, upstream string/brand, deployment-wide switch
or silent escalation; no change to REQ-017 evaluation itself. A new REQ id, ADR, PARITY and STATE entry
are required before the transport half ships (GUESS, per the owner's standing rule).

## 5. A11y / RTL / motion
- **A11y:** confirmation is a named/described `alertdialog`, focus moved in and returned, Escape cancels,
  Tab/Enter operable, result in the polite live region (spec/08 §8.9). Two-sentence error copy; warning
  in `text-text-error-primary` plus text + icon, never colour alone. Targets ≥44 px (`.boma-target`),
  focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring` / `.boma-focus`; zero critical/
  serious axe findings in light and dark.
- **RTL:** control and dialog mirror under `DirectionProvider` (`dir="rtl"`, `lang="he"`) using logical
  `ms`/`me`, `ps`/`pe`, `text-start`/`text-end` and `cx()`; model names, URLs and ids stay LTR in `<bdi>`;
  horizontal dialog motion reverses (docs/UI.md i18n/RTL).
- **Motion:** 120–180 ms state changes, `prefers-reduced-motion` via BoardUI globals, no animation on
  streaming content or on the trust label being read (spec/08 §8.9).
- **Tokens (CONFIRMED docs/UI.md + AGENTS.md rules):** free `select`, `switch`, `settings-modal`,
  `tooltip`, `direction`; `bg-background-secondary-default`, `bg-background-full`, `text-text-primary`,
  `text-text-secondary`, `border-separator-border`, `border-border-focus-ring`,
  `text-foreground-icon-primary`, `text-text-error-primary`; `text-title-3-semibold`, `text-body-medium`,
  `text-caption-1-semibold`; icons from `@remixicon/react` as references.

## 6. One REQ-named Playwright test
`test("REQ-042: onboarding first-bot approval default is safe by default, explicit to widen, and mirrored in RTL", …)`
in `apps/web/tests/onboarding.spec.ts`:
- Seed signed-in user, create a space via `spaces.create`, open `/onboarding/2`; assert the safe option
  is preselected and there is no critical/serious axe finding.
- Keyboard only: Tab to the control, pick permissive, assert a named/described `alertdialog`; Escape
  restores safe; pick again, confirm; assert one `bots.create` call carrying the chosen `approvalDefault`
  (intercept `**/rpc/bots/create`).
- Set `localStorage["boma-locale"]="he"` before load; assert `dir="rtl"`, the translated label, clean
  axe; with `reducedMotion:"reduce"` assert no dialog entrance animation.

## 7. First slice
Onboarding step 2 only: explainer, choice, confirmation, i18n keys, REQ-042 test. Store via one optional
`bots.create` field (GUESS: `approvalDefault` = `ask`|`allow`) and read it in that bot's approval decision.
No card affordance, import change, other settings surface or deployment switch.
**Gate:** field + policy hook are a DEVIATION and a new REQ id — record in `docs/PARITY.md` /
`docs/STATE.md`, open an ADR, get approval, then implement test-first. If transport is refused, ship
presentation-only pointing to M16, never a toggle that promises an unapplied grant.

## 8. Evidence
CONFIRMED — spec/08 §§8.1, 8.2, 8.3, 8.9; spec/01 REQ-007/017/018/042/043/044; spec/03 §3.1;
docs/UI.md; `crates/api/src/rpc.rs` `procedures()`/namespaces; docs/STATE.md (M16, card slice, owner rule).
Upstream data only — CHANGELOG standing-consent entries; `docs/architecture/structure.md`,
`computer-use.md`, `agents.md` (welcome, import downgrade); `apps/mobile/.../agent-access-fields.tsx`.
GUESS — field name/values, policy hook, import-warning wording.
