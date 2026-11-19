# 0101 — `apps/web/components/thread/thread-composer.tsx` — architecture (v0.4.0)

- Worker: 101 · Lens: architecture · Target: v0.4.0 · Read-only review; no code edited.
- Module: 106-line client component. Spec anchors: `spec/08-ui-ux.md` §8.3, REQ-001 (attachments),
  REQ-012 (chat/loader), REQ-017 (approval gate), `AGENTS.md` BoardUI token/RTL rules.
- Read here: `components/thread/thread-screen.tsx`, `components/application/composer-loader/composer-loader.tsx`,
  `components/base/file-upload/file-upload.tsx`, `components/base/textarea/textarea.tsx`,
  `components/base/input/input.tsx`, `lib/api/rpc.ts`, `lib/i18n.ts`, `tests/{reply,thread-ui,approval-card}.spec.ts`,
  `CHANGELOG.md`, `docs/STATE.md`, `docs/superpowers/specs/2026-11-18-ui-bot-create-and-acp-detection-design.md`.
- CONFIRMED = read in this session; GUESS = inferred.

## Structure today (CONFIRMED)

`ThreadComposer` is a single controlled client component with a clean public surface
(`value`/`onChange`/`onSend`/`sending`/`reply`/`awaitingApproval`/`transparent`). It owns: a reply
banner (`ReplyContext` + `onClear`), one raw React Aria `TextField`/`TextArea`/`Label` (lines 4, 78–99),
a `Button` submit, and the `canSend = trim>0 && !sending && !awaitingApproval` gate (line 46).
Enter sends / Shift+Enter newlines via `onKeyDown` (88–96); `<form onSubmit>` is a second path (54–59).
`thread-screen.tsx` wraps it in `ComposerLoader` and passes `transparent` so the loader paints the
pill surface (no double background). A11y/i18n are sound: `textbox` role, sr-only label, all copy via
`useTranslation()`, token-only classes, `cx()`. The shape is mostly correct and should survive v0.4.0.

The architecture debt is that the *field* is hand-built from raw React Aria instead of the installed
BoardUI `Textarea`, and that the composer has no draft model beyond a bare `string` — so attachments
have nowhere to attach. That is exactly the file-upload gap below.

## The file-upload gap — issue #20 (CONFIRMED)

`components/base/file-upload/file-upload.tsx` is installed but unwired: `onUploadComplete(file)`,
`allowedExtensions`, `maxBytes`, `renderFileIcon`; it *simulates* upload and is backend-agnostic
(8 MiB / pdf,jpg,jpeg,png,xlsx defaults). `threadsSend()` in `lib/api/rpc.ts` (274–286) accepts only
`botId`, `text`, `clientNonce`, `replyToMessageId`, `replyQuote` — **no attachment ids**. The design
doc (74–75) and `CHANGELOG.md:63` therefore keep the dropzone out because it would be a dead end
(forbidden), tracked as issue #20 / REQ-001 attachments; `docs/STATE.md:128–129` records it as
installed-but-unwired. Note the server already has a sniff-on-upload artifact path (D-03) and a
16 MiB cap (D-07), so the composer must carry an *artifact id*, never raw bytes.

## BoardUI Composer / Input / Textarea

BoardUI **has no `Composer` component installed** (CONFIRMED: no `components/**/composer/`), only
`components/base/textarea/textarea.tsx` (`Textarea` with `autoResize`/`maxRows`/`showCount`/
`aria-label` fallback) and `components/base/input/input.tsx`. The `ComposerLoader` is the chat-surface
component already in use. v0.4.0 should build on installed components (install a BoardUI Composer via
MCP only if the catalogue actually offers one — no hand-built lookalike), keep semantic tokens and
`cx()`, and never add a second design system.

## Concrete upgrades for v0.4.0 (each with a risk)

1. **Adopt BoardUI `Textarea` for the field.** Replace the raw React Aria trio (78–99) with
   `@/components/base/textarea/textarea` (`autoResize`, `maxRows`, explicit `label`/sr-only), keeping
   the `textbox` role and the `getByLabel(/Message/)` selector that `tests/reply.spec.ts` depends on.
   *Risk:* the BoardUI wrapper composes its own `TextField`; the Enter/`onKeyDown` handler and sr-only
   label must be forwarded correctly, or keyboard send and the a11y selector regress (GUESS — verify
   the prop surface before editing and re-run REQ-001/REQ-012 + axe).

2. **Give the draft a typed model so attachments have a home.** Change `value: string` to a small
   draft `{ text: string; attachmentIds: string[] }` (keep a `value`-compatible shim or update both
   call sites, `thread-screen.tsx` and `group-thread-screen.tsx`), and derive `canSend` from it via a
   pure helper `computeCanSend(draft, sending, awaitingApproval)` with unit tests.
   *Risk:* widening the prop is a public-surface change touching two screens; keep the gate identical
   and preserve Enter-to-send + the approval gate or REQ-017/REQ-012 goldens break.

3. **Wire issue #20 only behind a real transport capability.** Once `threads.send` accepts
   `attachmentIds` (contract change + ADR + generated-client update, not hand-written), add a compact
   `IconButton` trigger + named `Chip` row mirroring `ReplyContext`, using installed `file-upload`
   and `*artifacts.create`/the sniffed id — bytes never enter `text`. Until that RPC exists, render
   no attach control (dead end stays forbidden).
   *Risk:* a wire change is an externally visible DEVIATION needing owner sign-off (`docs/PARITY.md`);
   reconcile the component's 8 MiB default with the server's 16 MiB cap; keep filenames/ids LTR in
   `<bdi>` and the chip row RTL-aligned.

4. **Collapse the two send paths into one idempotent submit.** `onSubmit` and `onKeyDown` both call
   `onSend` (54–59, 88–96); they are guarded only by the parent-flipped `sending` flag. Funnel both
   through a single `submit()` local latch and reuse the per-send `clientNonce` already generated in
   `thread-screen.tsx:278` as the idempotency key.
   *Risk:* a local latch can swallow a legitimate retry if the parent never resets `sending` on
   failure — the "failed send never takes the keyboard away" invariant (line 42) must be preserved;
   assert the nonce is per-send, not per-mount (CONFIRMED per-send today).

5. **Finish RTL/a11y polish.** Use logical utilities (`text-start`, `border-s`, `ps/pe`) on the reply
   banner and chip row, keep focus `focus-visible:ring-2 focus-visible:ring-border-focus-ring` and
   icons as `@remixicon/react` component references, and verify the `awaitingApproval` live region
   (currently in `thread-screen.tsx`) stays announced.
   *Risk:* mirroring glyphs/spacing changes pixels captured by the §8.3 golden and `/ui-parity` —
   treat as parity evidence, not a silent visual DEVIATION; re-run axe (zero critical/serious).

## Explicit non-goals

No second design system, no BoardUI Pro dependency, no hardcoded copy (all via `i18n.ts`), no bytes
in `threads.send`, no attach control before issue #20's wire exists, and no change to `ComposerLoader`
or the server-is-the-only-boundary approval model. Keep `transparent`, Enter/Shift+Enter and the
`canSend` rule exactly as specified; ship no new feature outside the spec without a STATE/ADR entry.
