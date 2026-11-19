# Brief 0043 — "continue in a channel" through an accessibility lens

Proposal only; no product code, spec, golden or catalogue was changed. **CONFIRMED** = verified by a read here; **GUESS** = a recommendation for the owner. Upstream was read as data at `/tmp/opencode/openbot-parity` (HEAD `559d5ac`, 2026-11-19). No upstream code, asset, wording, identifier or brand is reproduced; no Pro component is used. Product strings stay Boma Bot. Language: English.

One caveat up front: the area slug `continue-in-a` is a fragment. The reading **"continue in a channel"** — typed "continue" resuming a stopped subtask, and the stopped-task card's Continue — is inferred from that fragment and its siblings (`delete-history-in/of`, `keep-memories-in`, `in-a-telegram`) (GUESS on the exact mapping). The surface itself is CONFIRMED at `559d5ac`; nothing below depends on the guess beyond the framing.

## 1. The upstream capability in my own words (inspiration only)

CONFIRMED (upstream `ChannelStoppedTasks.tsx`, `docs/architecture/channels-and-messaging.md`, `CHANGELOG.md:4082`, `channel.ts` strings, `error.backend.channelTaskStopped`): in a group channel where an agent delegates to members, work can stop mid-flight — a person pressed stop, the host restarted, or the loop guard tripped. The host then does three things:

- A **stopped-task card** renders above the composer: a labelled region per stopped task naming the owner, the instruction preview and a plain-sentence reason (a named-members assignment-loop reason, or the task's error), with **Continue**, **Reassign** (menu of other members) and **Cancel** actions. Continue resumes the task with its instruction; the new message rides along as a follow-up line.
- A **typed keyword** in the composer ("continue", or close synonyms) resumes *each* stopped root task of the newest request; a failed root task continues only when none is stopped, and a recipient member's own task goes first. Older open work never blocks a resume.
- A **waiting list** above the composer shows in-flight subtasks (asked/working/replied/failed) with a Stop action on unsettled rows and a dismiss once settled; it is a collapsible panel that goes `inert` when closed.
- Resume semantics are deterministic, no model in the loop: the routing keeps the existing task rather than starting a new request.

Accessibility interest: this is a **recovery affordance for work that already failed the person once** — exactly where a screen-reader or keyboard user is most likely to be lost. It must be named, announced, reachable, and never signalled by colour alone. Upstream's card shows the pattern to *improve on*, not to copy: a bare "Continue" button with no per-task accessible name, a reason line that can carry a raw provider error, and no live-region announcement when a task stops or resumes.

## 2. Why accessibility is the deciding lens

Boma Bot has the adjacent surfaces but no room-level "continue the stopped delegation" affordance, and the group room's existing controls were shipped before an a11y pass on that screen (CONFIRMED, below). If the upstream-shaped affordance is ever added without the a11y contract first, we rebuild upstream's weak spots with our tokens. This lens fixes the contract and the screens that exist today; the new control itself stays owner-approved and out of the first slice.

## 3. Grounding in our own contract

- **§8.3 group thread** (`/app/g/:groupId`) — shared log, member attribution, composer routes by mention; CONFIRMED, `spec/08-ui-ux.md` and `apps/web/components/group/group-thread-screen.tsx`.
- **§8.9 cross-cutting** — run status vocabulary matches 02; chips are text+icon never colour alone; status changes announced through a **polite live region**; shortcuts listed on a `?` overlay and never firing inside text fields; RTL mirrors `he`; 120–180 ms motion, `prefers-reduced-motion` respected; comfortable/compact density keeps ≥32 px targets; error copy = one what-happened sentence + one what-to-do sentence with the correlation id behind "details". All CONFIRMED.
- **REQ-068 (SHOULD)** — a delegation in a group room may be reassigned to another member; the room shows the move. CONFIRMED, `spec/01-requirements.md`; exercised by `apps/web/tests/group.spec.ts` ("REQ-068 a delegation is reassigned…"). **REQ-067** (pause/resume queued runs) and **REQ-021** (failures surface as a run status plus a reason) are the neighbours. REQ-068 is the anchor.
- **Live regions already in the tree**: `role="log" aria-live="polite"` on the transcript, `aria-live` on the "composer waiting" and "offline" lines, `role="status"` on card/activity status lines, `role="alert"` on card and reassign failures. CONFIRMED. Note: §8.9 says the transcript should *not* be a live region — the `role="log"` on the transcript is our own drift, worth flagging to the owner, not silently changing.
- **Docs/UI.md** — WCAG 2.2 AA target, zero critical/serious axe findings in light/dark/RTL; `.boma-target` 44 px; `text-text-secondary` re-mapped to clear 4.5:1; RTL proof lives in `apps/web/tests/a11y.spec.ts` (Hebrew, `dir="rtl"`, clean axe); logical properties; `DirectionProvider` from the locale; `<bdi>`/`dir="auto"` for LTR islands. CONFIRMED.

## 4. Proposed parity changes (grounded, no new feature)

1. **Name the stopped/failed delegation region.** When a room's delegation stops (cancelled/failed) the banner must be a `role="region"` with an accessible name stating owner and state ("Stopped task for {name}" pattern, catalogue keys under `groups.*`), with the reason as plain text — never a tint alone (WCAG 1.4.1, 1.4.11, 4.1.2, 2.4.6). Icon + text always.
2. **Per-control accessible names.** "Continue" and "Reassign to {name}" must name the task owner (compound name via `aria-labelledby` row id + action id, the pattern upstream's waiting rows use), so a screen-reader user tabbing the banner hears *which* task each control acts on, not three ambiguous verbs (WCAG 2.4.6, 4.1.2).
3. **Announce transitions, don't steal focus.** Card arrival and resume/reassign/cancel outcomes go through the existing polite status line (`role="status"`), never the transcript, never `role="alert"` unless a side effect failed; focus stays with the composer while the person is typing; if focus was inside the card when it disappears after a resume, it moves to the next sensible element (next card, else composer), not to `<body>` (WCAG 2.4.3, 4.1.3; §8.9 "Approvals/Announcements").
4. **Honest states.** Reason text reuses REQ-021's legible-reason contract; raw provider strings never render bare; correlation id behind "details" only. Chips follow the run-status vocabulary (`waiting_input`/`cancelled`…) with text+icon (§8.9; WCAG 1.4.1).
5. **Keyboard path end-to-end.** Every card action operable by Tab/Enter/Space and pointer; the `?` overlay lists the room shortcuts; no shortcut fires while the composer has focus; the reassign menu (BoardUI `dropdown`, free tier) keeps roving tabindex and `Escape` close returning focus to the trigger. Targets ≥44 px via `.boma-target`, ≥32 px in compact density (WCAG 2.1.1, 2.5.8).
6. **RTL and bidi.** `he` renders `dir="rtl"` with the banner, back affordance and menu placement mirrored — the back link currently uses a physical left arrow (CONFIRMED, `RiArrowLeftLine`) and should switch to BoardUI's `directional-icon`; member names, ids and the reason's inline code stay `<bdi>`/`dir="auto"` LTR; menu opens toward the correct inline start (WCAG 1.3.2; §8.9 "RTL", docs/UI.md).
7. **Motion and contrast.** Card entrance uses the existing `boma-card-in` with the reduced-motion fade fallback; nothing animates on streaming content; no value the user is reading is animated (§8.9 Motion). Reason text uses `text-text-secondary` (already re-mapped ≥4.5:1); focus ring stays `border-border-focus-ring` via `.boma-focus`/BoardUI (WCAG 1.4.3, 2.4.13).
8. **Awaiting/waiting analogue.** If the room grows a waiting-list panel, it must reuse the proven collapsible pattern: `aria-expanded`/`aria-controls`, `inert` when closed, per-row stop with compound names, dismiss only when settled — same rules as (1)–(7). Design only; not in this slice.

## 5. One REQ-named Playwright test

Add to `apps/web/tests/group.spec.ts`, name carrying the REQ id:

```
test("REQ-068 a stopped delegation is named, announced and reassignable by keyboard", ...)
```

Steps: `applySeed(page, "group-golden-room")`; route `groups/get` with an active run in `cancelled`/stopped state carrying a reason; load `/app/g/group_golden`; assert the banner is a named `region` whose reason is visible text; Tab from the composer to the banner, then to "Reassign to analyst"; activate with Enter (no pointer); assert the `groups/reassign` payload, that the `role="status"` line announces the hand-off, and that focus never lands on `<body>`; re-run with `boma-locale=he` init script asserting `dir="rtl"` and a clean axe run; set `reducedMotion: "reduce"` and assert the entrance animation degrades to a fade; zero critical/serious axe findings throughout. REQ-068 keeps `tools/trace_check.py` honest; `docs/TRACEABILITY.md` is extended only after the test exists.

## 6. First slice

1. Catalogue keys for the named region, announcement and reassign-outcome status line (`groups.*`), en + fr + he, English fallback.
2. `role="region"` + `aria-labelledby` on the room's active-run/stopped banner; `RiArrowLeftLine` → directional icon; `role="status"` outcome line wired to `groupsReassign` resolution. No new RPC, no new control, no spec change; the "Continue" affordance itself is proposed separately for owner approval (a new run-resume surface in a room is a DEVIATION until then).
3. The single test above; watch it fail first, then green. Run `a11y`, `group`, `run-controls`, `golden`, `motion` specs.
4. Flag the §8.9 transcript-live-region drift to the owner in `docs/STATE.md`; do not fix it silently.

**Out of scope:** new resume/continue RPC or control (owner approval), new REQ ids, upstream copy/brand/assets, the waiting-list panel, any Pro component, spec rewording.

## 7. Evidence

CONFIRMED — upstream `559d5ac`: `packages/ui/src/features/channels/ChannelStoppedTasks.tsx` (region `aria-label`, bare `common.continue`, Reassign `DropdownMenu` with `aria-label`, ghost Cancel, assignment-loop vs plain-error reason), `packages/ui/src/features/conversation/AwaitingReplies.tsx` and `TaskList.tsx` (`aria-expanded`/`aria-controls`, `inert` panel, `aria-labelledby` row/action compound names, `role="status"` states, dismiss when settled), `docs/architecture/channels-and-messaging.md` (keyword resume semantics, failed-vs-stopped rule, recipient-first, restart reason `channelTaskStopped`), `CHANGELOG.md:4082`, `packages/i18n/src/messages/en/channel.ts`, `channel-paused-task*` classes. Ours: `spec/08-ui-ux.md` §8.3/§8.9, `spec/01-requirements.md` REQ-021/067/068 (+REQ-057), `docs/UI.md` (AA target, `.boma-target`, RTL proof, token re-mappings), `group-thread-screen.tsx`, `thread-screen.tsx`, `message-blocks.tsx`, `activity-screen.tsx`, `run-controls.spec.ts`, `group.spec.ts`, `a11y.spec.ts`, `docs/TRACEABILITY.md`. GUESS — the `continue-in-a` framing, the Catalogue key names, the proposal to expose a room-level Continue control at all.
