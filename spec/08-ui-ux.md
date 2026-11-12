# 08 — UI and UX specification

The web app is the primary surface. Desktop and mobile are the same product in shells (REQ-055,
REQ-056); where they differ it is stated. Route table is taken from `apps/web/src/App.tsx:183-299`.

Design constraints that apply everywhere:

- **One screen, one job.** The chat is the product; every other screen is a drawer, a panel or a
  settings route reached from it.
- **State is never ambiguous.** Every run, approval, computer and artifact shows a status the user can
  name out loud. No decorative spinners that outlive their work.
- **The interface must survive latency.** Skeletons and optimistic states everywhere; a blank panel is
  a bug.
- **Keyboard first.** Every action in a menu is reachable without a pointer.
- **No hardcoded strings.** All copy comes from the catalogue; the layout mirrors for RTL.

---

## 8.1 Route map

| Route | Screen | Access |
|---|---|---|
| `/` | Root: redirects to `/app` when signed in, otherwise the sign-in screen | public |
| `/sign-in`, `/sign-up` | Authentication | public |
| `/forgot-password`, `/reset-password` | Password recovery | public |
| `/invite/:token` | Accept an invitation (sign-in or sign-up inline) | token |
| `/onboarding/:step?` | First-run setup: create space, first bot, connect a model | signed in, no space yet |
| `/app` | Shell with the selected bot's thread | signed in |
| `/app/:botId` | Shell, thread of one bot | signed in |
| `/app/g/:groupId` | Shell, group thread | member of the group |
| `/app/x/:linkId` | A saved navigation link | signed in |
| `/app/artifacts`, `/app/artifacts/:artifactId` | Artifact library and viewer | signed in |
| `/app/board` | Ticket board (feature-flagged) | signed in, board enabled |
| `/integrations/setup` | Connector setup return path | signed in |
| `/admin/plugins` | Plugin submissions review | space admin |
| `/admin/access` | Members, invitations, access grants | space admin |
| `/p/:slugToken` | Public shared page (no session) | token |
| OAuth callback paths | MCP and messaging install returns | state token |
| `*` | Not found | public |

Anything not listed is not a screen. In particular there is no settings *page*: space, model,
connector and member settings are panels inside `/app`.

---

## 8.2 Authentication and onboarding

**`/sign-in`** — one column, product mark, form. Methods shown are the ones the deployment reports from
`/api/auth/capabilities`: password (when enabled), passkey, SSO (when configured), e-mail code. Hidden
methods are not rendered as disabled buttons — they are absent.

States: idle → submitting (button busy, form disabled) → error (inline, above the field, no toast) →
success (redirect to `/app` or to the `next` route). An invalid credential must not reveal whether the
address exists.

**`/sign-up`** — same layout. The first account on a fresh deployment may become the operator; the
screen must say so plainly rather than silently granting power.

**`/forgot-password` / `/reset-password`** — identical response whether or not the address exists. The
reset screen validates token and password locally before submitting, then signs the user in.

**`/invite/:token`** — shows the space name and the inviter, then either signs the person in or lets
them create an account, then accepts the invitation. Expired or revoked tokens show a specific message
and a way to request a new invitation.

**`/onboarding/:step?`** — three steps, each independently skippable except leaving a space without a
model: (1) name the space, (2) create the first bot with a name and one sentence of purpose,
(3) connect a model provider or paste a key. Progress is visible; reloading resumes on the same step;
finishing lands on `/app/:botId` with the bot's intro message already streaming.

---

## 8.3 The shell (`/app`, `/app/:botId`, `/app/g/:groupId`)

Three regions on desktop, two on mobile.

**Left rail — navigation**
- Space switcher at the top (name, role, switching is instant and keeps the current route shape).
- Bot list grouped by user-defined sections; each row shows name and a status dot when it has an
  active run.
- "New bot" and "New group" actions.
- Activity entry that replaces the list with the run-centric view (REQ-011).
- Footer: member avatar, settings panel trigger, admin entries when the role allows.

**Centre — the thread**
- Header: name, member/participant count, model badge, computer indicator, overflow menu
  (rename, archive, delete, secrets, memory, routines).
- Transcript: virtualised, newest at the bottom, stable scroll anchoring when older pages load.
- Typed blocks: text (markdown with code), images, files, tool calls (collapsible, one line when
  collapsed), cards (question, approval, purchase, takeover), run status rows.
- Unread divider and "jump to latest" affordance (REQ-009).
- Composer at the bottom: text, attachment button, mention `@bot`, reply-to chip, stop button while a
  run is active, send disabled only when there is nothing to send.

**Right panel — context**
- Computer screen stream when a computer is running, with takeover and release controls
  (REQ-028, REQ-029).
- Artifacts produced by this thread, newest first.
- Run activity for this bot.

**Mobile** — rail becomes a drawer; the context panel becomes a bottom sheet with three tabs; the
composer sticks to the bottom above the keyboard and never covers the last message.

### Thread states

| State | What the user sees |
|---|---|
| Loading | Skeleton rows shaped like messages, composer active |
| Empty (new bot) | The bot's intro text and two suggested prompts; composer focused |
| Streaming | Assistant text grows token by token; a status row above the composer names the phase |
| Tool running | One line per tool call, expandable; failures inline and red |
| Waiting on user | The card is the state: it scrolls into view, takes focus, and the composer is disabled while the card is unanswered only if the card is modal (approvals are; questions are not) |
| Waiting on takeover | Banner over the transcript, computer panel raised |
| Failed | Error row with a plain-language cause and a retry action (REQ-021) |
| Stopped | Same row, "stopped by <name>", no retry unless the failure was a failure |
| Offline | Thin bar: "not live"; messages still send, and the UI reconciles on reconnect without duplicating rows |

---

## 8.4 Activity view

Reached from the rail, `REQ-011`. A grouped list by day: prompt snippet, bot name, status chip, duration,
and a link back to the message that started it. Filter by bot and status. Working runs sort to the top
and update in place. Background (ticket) runs appear here but not in the transcript.

---

## 8.5 Artifacts

`/app/artifacts` — a grid of cards: name, type icon, size, owner bot, updated time; filter by type and
by bot; search by name. `/app/artifacts/:artifactId` — viewer with version selector (versions are a
chain), download, "open in thread", and a copy action. Images render in a bounded frame (never wider
than the panel), PDFs and text render natively, anything else offers a download. Content is untrusted:
never render HTML from an artifact in the app document.

---

## 8.6 Board (`/app/board`, feature-flagged)

Columns by state, ticket cards with key, title, assignee bot, and last comment. A ticket detail panel
shows the description, comments, and the background runs attached to it with their status. Creating a
ticket and assigning it to a bot queues work; the board shows progress, the chat does not.

When the flag is off, the route is absent — not a locked screen.

---

## 8.7 Administration

**`/admin/access`** — members table (name, email, role, last seen), pending invitations, access grants
with reasons. Role changes are optimistic with a rollback on failure. Removing a member states what
they lose.

**`/admin/plugins`** — submissions queue: source reference, submitter contact, review state, and the
parts the installer refused with the reason. Approving a plugin never executes its code.

**`/integrations/setup`** — the post-OAuth landing: confirm the connection, choose which bots may use
it, and surface a partial-install report rather than a green check when parts were skipped.

---

## 8.8 Public shared page (`/p/:slugToken`)

No navigation chrome, no session, no product chrome beyond a small footer. Renders the compiled page
with its own CSP. Unknown or revoked tokens render an identical "not available" screen — the same one
for both, with no timing or wording difference. `noindex`, `no-referrer`.

---

## 8.9 Cross-cutting behaviour

**Run status vocabulary in the UI** must match 02 exactly: working (`queued`, `leased`, `running`),
paused (`waiting_input`, `waiting_takeover`), terminal (`completed`, `failed`, `cancelled`). Chips use
text plus icon, never colour alone.

**Approvals** are modal and blocking for the effect only: the transcript stays readable, other threads
stay reachable, and the decision is one click with an explicit "what will happen" sentence.

**Announcements** — run status changes, card arrivals and errors are announced through a polite live
region; the transcript itself is not a live region (otherwise every token is announced).

**Keyboard** — send, new line, stop run, focus composer, next/previous bot, close panel, search, and
open the shortcut list. Shortcuts are discoverable from a `?` overlay and never fire while a text field
has focus except send/new-line.

**Theming** — light and dark from the same tokens; system preference by default; no flash of the wrong
theme on load (the theme class is set before first paint).

**RTL** — mirrored layout for `he` (and any future RTL locale): rail on the right, arrows flipped,
drawer motion reversed, numbers, code and URLs kept LTR.

**Density** — comfortable and compact; both preserve hit targets of at least 32 px.

**Motion** — 120–180 ms for state changes, no animation on content that is streaming, respect
`prefers-reduced-motion`, and never animate a value the user is reading.

**Error copy** — one sentence of what happened, one of what to do. Stack traces, ids and provider
messages never reach the screen; the correlation id is offered behind "details" for support.

---

## 8.10 UI golden examples

Each file in `golden/ui/` pins one seeded screen state, never a click path: `state` is
`{route, seed?, role, viewport, media}`, `expect` is a list of accessibility assertions
(`{role, name?, state?, count?}`), and `notes` says why the state matters. Filenames are
`REQ-<id>-<slug>.json`; a `name` assertion is used only where this specification defines the wording,
never the reference's.

| File | Requirement | Covers |
|---|---|---|
| `REQ-001-send-message.json` | REQ-001 | Sending creates one optimistic message that is replaced, not duplicated, by the server row |
| `REQ-007-pending-card.json` | REQ-007 | A pending card is visible, takes focus, and does not block reading or other threads |
| `REQ-009-unread-divider.json` | REQ-009 | Unread divider position and removal after reading |
| `REQ-012-run-statuses.json` | REQ-012 | Every run status renders a distinct, named chip; terminal states offer the right action |
| `REQ-026-computer-panel.json` | REQ-026 | Computer panel states from provisioning to stopped, including takeover |
| `REQ-037-shared-page.json` | REQ-037 | Shared page renders without chrome |
| `REQ-037-shared-page-invalid.json` | REQ-037 | Unknown or revoked token shows the uniform unavailable screen |

Adding a screen state means adding a file here; a golden without a requirement is a bug in this spec.
