# 0032 — contact-us-in × desktop (parity proposal, Boma Bot)

- Brief: id 32, area `contact-us-in`, lens `desktop` (`docs/openbot-swarm/briefs-1000.jsonl`).
- Status: **PROPOSAL only.** Upstream read as data at HEAD `559d5acd` (2026-11-19); no product code, spec, golden, asset, copy, brand or catalogue changed. No BoardUI Pro; one design system.
- Markers: **CONFIRMED** = read/run this session; **GUESS** = needs the owner's decision.

## 1. Upstream capability, in my own words (inspiration only)

The desktop keeps a small, persistent help/contact entry in the window rail; its entries are **named destinations** that the main process resolves into URLs and opens in the system browser — never inside the app session. Restated generically, with no upstream name, wording or asset reused:

- "Report a problem" opens a prefilled issue form carrying facts only the host knows: platform, app version, OS, built-from-source or installed-release. A feedback entry opens a social post intent; community, changelog and guides entries round out the menu.
- The renderer records which destination was opened (a counter with a label, never content); a separate in-app path lets an agent file feedback through the service, with similar-issue de-duplication, daily rate limits and a sender-side withdraw. A hosted mailbox contact is a plain mail link.
- The pattern worth carrying: the machine is the only side that knows the machine, so diagnostics are composed on-device; contact is one deliberate tap; nothing leaves the machine unless the user chooses to send; the channel answers nothing and approves nothing by itself.

## 2. Where Boma Bot stands (CONFIRMED)

- REQ-055 (`spec/01-requirements.md`): an Electron shell loads the web app, keeps a session in a partition it controls, supports native passkeys and signed auto-update, manages a local stack; "desktop and mobile are the same product in shells" (`spec/08-ui-ux.md`).
- `apps/desktop/src/main.ts:` thin shell; window on `persist:boma` (`contextIsolation`/`sandbox` on, `nodeIntegration` off, no preload/IPC today); `guardNavigation` denies `window.open` and routes http(s) URLs to `shell.openExternal`; native menu is File/Stack/View only; fatal dialogs compose `logHint()`/`resetHint()` (paths to `boma.log`, `postgres.log`, "delete `userData/boma/` to reset"); `boot.log` checkpoints; `StackError{ step, log }` names the failing stage.
- `apps/desktop/src/stack.ts`: embedded Postgres + migrate + serve + worker, `BOMA_LOCAL_FIRST=true`, `BOMA_WEB_ROOT`; HTTP port persisted in `userData/boma/port`. No support, diagnostics or contact affordance anywhere; no telemetry/analytics in the shell (grep clean).
- Web app: the "Support" secondary nav entry (`apps/web/components/application/dashboard/dashboard-sidebar.tsx`, `RiCustomerServiceLine`) is a dead `<a href="#">` with no `onClick` — wired to no page, dialog or external link. `spec/08-ui-ux.md` offers the correlation id "behind details" for support; no contact/support transport exists in the spec or code.
- M35 design (`docs/superpowers/specs/2026-11-16-m35-self-contained-desktop-design.md`): the self-contained double-click desktop; "M35 is distribution" — no wire, web, server or spec change.

**Gap:** the self-contained desktop has no reachable contact path at all — not in the window chrome, not on the web surface. The fatal path is the only place support is mentioned, and it only names log paths; the user is left to find the operator alone.

## 3. Desktop parity contract (the lens)

1. **Self-contained first (M35).** Works with no account, no setup, no network: diagnostics are composed on-device; nothing is transmitted without an explicit user tap; no in-app mail client, no phone-home.
2. **System browser, never the partition.** Support links go through `guardNavigation` → `shell.openExternal`; the window never navigates off its deployment origin and popups stay denied (REQ-055's session guarantee intact).
3. **The shell composes only facts it already holds:** packaged app version, `process.platform`/`arch` and system version, packaged vs built-from-source, embedded loopback vs `BOMA_DESKTOP_URL` origin, last fatal `StackError.step` / `boot.log` checkpoint. Paths only as pointers (like `logHint()`) — never the master key, env vars or log bodies. The correlation id behind "details" (spec/08) is the only sanctioned reference; reuse, never invent.
4. **Thin shell.** A native "Help" menu (File/Stack/View precedent; plain Electron labels — not a second design system, no preload/IPC): `Contact support…` (external) and `Copy diagnostics` (Electron `clipboard`, paste-anywhere). When approved, the web "Support" entry opens a BoardUI dialog with the same content; that wiring is web scope, not this slice.
5. **No new transport.** A ticket/mail backend, stored contact records, or telemetry on support clicks need a new REQ id, an ADR and a `docs/PARITY.md` DEVIATION, plus owner approval. None is proposed here.
6. **BoardUI, for the later web dialog:** free `alertdialog`/`dialog` only; `bg-background-secondary-default` with `border-separator-border`; `text-headline-medium`/`text-body-2-regular`/`text-caption-1-*` in `text-text-primary/-secondary/-tertiary`; one `@remixicon/react` icon as a component reference, `aria-hidden`; `cx()` from `@/utils/cx`; `focus-visible:ring-2 focus-visible:ring-border-focus-ring`; hit targets ≥32 px (spec/08); strings from the catalogue under `DirectionProvider`; logical `ms/me`, `ps/pe`, `text-start/end`; email/ids/URLs in LTR islands (`<bdi>`); dark mode rides `.dark` tokens; no raw palette, no `dark:` literals, no Pro.

## 4. One REQ-named Playwright test

In `apps/web/tests/desktop-contact.spec.ts`, reusing the `smoke-modes` harness (`BOMA_SMOKE_BASE`/`BOMA_SMOKE_PROD`; the M35 static export served by the embedded stack; `seedRpc` in dev):

```ts
test("REQ-055 the desktop contact path keeps the session on its origin and sends support to the system browser", async ({ browser }) => {
  // GIVEN the M35 export on the embedded stack (seeded local-first, 1280×800)
  // WHEN the Support entry in the secondary nav is opened
  // THEN the dialog carries the support correlation id behind "details";
  //      the app page URL never leaves the deployment origin;
  //      an external support link opens a NEW context to an allowlisted https
  //      host (in Electron: popup denied, shell.openExternal — §3.2);
  //      no master-key path, env name or log body appears in the DOM;
  //      axe reports no serious finding; the context closes cleanly.
});
```

Distinct from `smoke-modes.spec.ts` (render) and `shell.spec.ts`. Dev mode runs the same assertions via `seedRpc`. RED today: `Support` is `href="#"` — no dialog, no route anywhere.

## 5. First slice (smallest safe, test-first)

1. Write the REQ-055 test above, red; record the failure.
2. Add only a "no outbound support transport; the shell stays distribution-only" contract comment to the `main.ts` header and `stack.ts` env block — no behaviour change.
3. **Not** wired: the Help menu, clipboard diagnostics, the web dialog, any transport, analytics or support storage. Each is an owner-approval item (§3.4/§3.5); list them in `docs/STATE.md` as proposals with their REQ/ADR/DEVIATION needs.
4. No spec, wire, web, server, catalogue or golden edit; no dependency change (`docs/DEPENDENCIES.md` untouched).
5. Gates: web typecheck/lint/build; Playwright `desktop-contact` (red, documented) + `smoke-modes`; desktop `npm run typecheck`/`lint`/`build`; axe zero critical/serious; light/dark and `he` RTL; loopback-only proof. Record in `docs/STATE.md` and `docs/UI.md`.

## 6. Non-goals / evidence

No upstream code, asset, wording, icon, colour, font or brand; no Pro; no second design system; no phone-home or silent send; no approval/effect behaviour touched (REQ-017 untouched); no wire field or migration. Upstream is an oracle, never a source to copy.

CONFIRMED — REQ-055/spec/08; `apps/desktop/src/main.ts`, `src/stack.ts`; `apps/desktop/README.md`; M35 design doc; sidebar `Support` (`href="#"`, no `onClick`); `spec/08-ui-ux.md` (correlation id, hit targets); `smoke-modes.spec.ts`; `docs/openbot-swarm/waves/wave-2/0012-always-allow-on-desktop.md`. GUESS — final menu wording, the diagnostics allowlist shown browser-side, the web dialog shape, and any Help menu at all (owner decision).
