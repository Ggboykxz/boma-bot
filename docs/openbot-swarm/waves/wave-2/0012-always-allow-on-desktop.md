# 0012 — always-allow-on × desktop (parity proposal, Boma Bot)

- Brief: id 12, area `always-allow-on`, lens `desktop` (`docs/openbot-swarm/briefs-1000.jsonl`).
- Status: **PROPOSAL only.** Upstream read as data (HEAD `386085ff`, 2026-11-18); no product code, spec, golden, asset, copy, brand or catalogue changed. No BoardUI Pro, one design system.
- Markers: **CONFIRMED** = read from a file this session; **GUESS** = needs owner's decision (AGENTS.md:63 — externally visible behaviour is a DEVIATION).

## 1. Upstream capability, in my own words (inspiration only)

Some desktop agent hosts keep a **computer-bound permission file** on the machine running the agent: which agents may act without asking. Restated generically, with no upstream name or wording reused:

- The open tier raises no card for a classified write; the middle tier parks one, and that card offers a one-click step that **promotes the agent to the open tier** — only while in the middle tier. The agent can narrow its own reach, never widen it.
- Promotion is deliberate, a host-side forced setting can lock it, and the change also **answers the cards already waiting** for that agent instead of leaving each open.
- The file belongs to the computer, never crosses the team/remote API, is scoped per agent; import downgrades a remembered grant to a warning so a fresh agent never starts elevated.
- Tool-level "always allow" lives in a file-backed layer per tool and provider account; privileged computer-use completion is accepted once and **never stored**; forms, sign-in and browser takeovers stay interactive.

The pattern worth carrying: a reversible, explicit, computer-scoped transfer of a repeated decision into a standing default, with safe import, honest audit trail and hard exceptions — never a hidden or default-wide grant.

## 2. Where Boma Bot stands (CONFIRMED)

- REQ-055 (`spec/01-requirements.md`): Electron shell loads the web app, keeps a session in a partition it controls, supports native passkeys and signed auto-update, manages a local stack; "desktop and mobile are the same product in shells" (`spec/08-ui-ux.md`).
- `apps/desktop/src/main.ts`: thin shell — window on `persist:boma` (contextIsolation/sandbox on, nodeIntegration off); `guardNavigation` pins one origin; `BOMA_DESKTOP_URL` selects a remote window; native File/Stack/View `Menu` (sign-out clears storage; stack restart); signed auto-update wiring; **no preload/IPC surface today**.
- `apps/desktop/src/stack.ts`: `startStack` boots embedded Postgres + migrate + serve + worker as supervised children, env `BOMA_LOCAL_FIRST=true`, `SANDBOX_PROVIDER=none`, `BOMA_JOBS=postgres`, `BOMA_REALTIME=postgres`, `BOMA_WEB_ROOT`, `BOMA_BIND_ADDR`, master key, artifact dir; state in `userData/boma/`. **No approval/auto-approve variable exists at boot.**
- M35 design (`docs/superpowers/specs/2026-11-16-m35-self-contained-desktop-design.md`): the double-click, self-contained desktop; "M35 is distribution" — explicitly out of scope: any change to wire, web, server logic or spec. `docs/STATE.md` schedules M16 scoped auto-approvals; a new REQ id, ADR and `docs/PARITY.md` DEVIATION need owner approval before transport changes ship.
- REQ-017: a per-space/per-user policy decides which calls need approval, effect runs once; REQ-007: a pending card never freezes chat. `CardBlock` (`apps/web/components/thread/message-blocks.tsx`) is its only client surface; "the card is the state" (spec/08 §8.3). No always-allow field, autonomy level or card affordance exists today.

**Gap:** the desktop is the natural computer-bound host for a standing grant, yet M35 forbids the wire/web/server change that would create one — and neither shell nor boot config states the safe invariant, so nothing proves the self-contained app cannot come up pre-elevated or absorb a remote grant.

## 3. Desktop parity contract (the lens)

1. **Safe boot is the parity.** Stack starts with the REQ-017 gate on: `stack.ts` gains no env var or config write granting blanket approval, `main.ts` writes none outside its partition. Always-allow is one per-space/per-user policy state (REQ-017, REQ-043), never a desktop-wide switch or launcher flag (GUESS: policy held by the local server).
2. **Computer-scoped, not remote.** The embedded stack is single-user, loopback-only (M35, `BOMA_LOCAL_FIRST=true`); `BOMA_DESKTOP_URL` windows stay on their origin under `guardNavigation`: a local setting must never grant approval on a remote deployment, a remote admin change is authoritative, the shell never answers a card on either host's behalf.
3. **No inherited grant.** Export/import (REQ-044) and any agent file must not carry an elevated grant into a fresh local agent; a remembered grant becomes a warning needing confirmation.
4. **Exceptions stay interactive.** Sign-in, takeover, forms and privileged computer-use consent are never absorbed; exactly-once effects (REQ-017/018) and tenancy (REQ-043) stay authoritative; a card closed by a policy flip is "superseded by policy", never minted as a member's "Approved".
5. **Thin shell.** Disclosure/control belongs on the web surface inside Electron, not the native `Menu` — the shell has no preload/IPC and M35 is distribution-only.

## 4. BoardUI surface inside the shell (CONFIRMED docs/UI.md)

When the M16 policy exists, card disclosure and confirmation stay in the web export: free `alertdialog`/`tooltip`; `bg-background-secondary-default` with `border-separator-border`; type `text-body-2-regular`/`text-caption-1-semibold` in `text-text-secondary`/`text-text-tertiary`; focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring`/`.boma-focus`; targets ≥44 px (`.boma-target`); one `@remixicon/react` icon as a component reference, `aria-hidden`; copy via the catalogue under `DirectionProvider` with logical `ms`/`me`, `ps`/`pe`, `text-start`/`text-end` and `cx()`, so RTL mirrors inside the window (REQ-057). Native menu strings are plain Electron labels, not a second design system. No palette/`dark:` literals, no Pro, no upstream asset or brand.

## 5. One REQ-named Playwright test

In `apps/web/tests/desktop-approval.spec.ts`, reusing the `smoke-modes` harness (`BOMA_SMOKE_BASE`/`BOMA_SMOKE_PROD`, the M35 static export served by the embedded stack):

```ts
test("REQ-055 the self-contained desktop boots with the approval gate on and never auto-answers a waiting card", async ({ browser }) => {
  const context = await browser.newContext({ viewport: { width: 1280, height: 800 }, locale: "en" });
  const page = await context.newPage();
  const answers: unknown[] = [];
  await page.route("**/rpc/threads/answer", async (route) => {
    answers.push(route.request().postDataJSON());
    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ ok: true }) });
  });
  expect((await page.request.get(`${BASE}/health`)).status()).toBe(200);
  const caps = await (await page.request.get(`${BASE}/api/auth/capabilities`)).json();
  expect(caps.localFirst).toBe(true);
  await page.goto(`${BASE}/app/${BOT}/`);        // seeded classified write (real API / seedRpc)
  const card = page.getByRole("region", { name: "Pending decision" });
  await expect(card).toBeVisible();              // no boot-time blanket grant suppresses it
  await expect(page.getByRole("textbox", { name: "Message" })).toBeDisabled();
  expect(answers).toEqual([]);                   // never auto-resolved without a decision
  expect(await new AxeBuilder({ page }).analyze()).toEqual(expect.objectContaining({ violations: [] }));
  await context.close();
});
```

It pins the desktop-lens invariant — a self-contained boot is safe by default and the card is never silently answered — distinct from `approval-card.spec.ts` (REQ-007/017 naming, Deny) and `smoke-modes.spec.ts` (render smoke). Dev mode runs the same assertions via `seedRpc`.

## 6. First slice (smallest safe, test-first)

1. Write the REQ-055 test above, red, against the local stack; prove the gate holds at boot.
2. No product-code change: add **only** a "no boot-time always-allow" contract comment to the `stack.ts` env block and `main.ts` header — a grant belongs to REQ-017/M16, never a launcher flag or remote-origin window (the runtime already sets no such variable).
3. Record in `docs/STATE.md` (M16 dependency) and `docs/UI.md`; no `docs/PARITY.md` DEVIATION, because externally visible behaviour does not change.
4. **Do not** add a wire field, policy write, card affordance, native-menu toggle or import change — those need a new REQ id, ADR and owner approval; ship after M16, never a toggle promising an unapplied grant (AGENTS.md:27/63).
5. Gate: test green; web typecheck/lint/build; desktop `npm run build`/`typecheck`/`lint`; axe zero critical/serious; keyboard path; 360 px; light/dark and RTL. No dependency change.

## 7. Non-goals / evidence

No upstream code, asset, wording, icon, colour, font or brand; no Pro; no second design system; no deployment-wide or launcher switch; no silent/default escalation; no change to REQ-017 evaluation. Upstream is an oracle, never a source to copy (AGENTS.md:70).

CONFIRMED — `spec/01-requirements.md` REQ-007/017/043/044/055/057; `spec/08-ui-ux.md` §§8.3, 8.9; `apps/desktop/src/main.ts`, `apps/desktop/src/stack.ts`, `apps/desktop/package.json`; M35 design doc; `docs/STATE.md`, `docs/UI.md`; `apps/web/tests/approval-card.spec.ts`, `smoke-modes.spec.ts`, `fixtures.ts`; `AGENTS.md` rules 27/34/63/70. Upstream data only — `docs/architecture/structure.md`, `computer-use.md`, `browser.md`, `agents.md`, `CHANGELOG.md` (standing consent, import downgrade, already-waiting cards, forced setting). GUESS — policy field name, disclosure copy, shell comment.
