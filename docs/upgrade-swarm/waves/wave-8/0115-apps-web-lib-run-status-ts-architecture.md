# 0115 — run-status helper architecture proposal (v0.4.0)

Module: `apps/web/lib/run-status.ts` (51 lines, read this session).
Lens: architecture / structure.
Spec anchor: REQ-012 (explicit run state machine, MUST), `spec/01-requirements.md` §REQ-012;
`spec/golden/ui/REQ-012-run-statuses.json`; local id W8.115.
Status: "Current structure" and the inconsistencies below are CONFIRMED by reading the file,
its two call sites (`components/activity/activity-screen.tsx`, `components/thread/thread-screen.tsx`),
`lib/i18n.ts`, `tests/fixtures.ts` and the spec this session; numbered upgrades and risks are
PROPOSAL/GUESS (design inference, not implemented).

## Current structure (CONFIRMED)

A tiny, pure, client-side presentation helper: no IO, no React, no state. It exports one vocabulary
(`RUN_STATUSES` + derived `RunStatus`), a guard (`isRunStatus`), two i18n-key builders
(`runStatusKey`, `runTriggerKey`) and one predicate (`isRetryable`, true only for `"failed"`).
Consumers turn a returned key into `t(key)` and fall back to the raw wire string when the key is
`null`. This module is the *view* vocabulary for REQ-012; the authoritative transition table lives
in the Rust backend and must not be duplicated here.

Observed structural smells (CONFIRMED):

- **"Eight" vs nine.** The header and the `isRunStatus` doc comment both say "eight statuses", but
  `RUN_STATUSES` has **nine** members — `"paused"` is present. `lib/i18n.ts` carries a key for all
  nine in en/fr/he; `tests/fixtures.ts` seeds only eight (no `"paused"`) and the REQ-012 golden
  expects eight chips. `paused` is a UI-only state toggled by the pause/resume buttons in
  `activity-screen.tsx`, not a domain transition. The vocabulary silently conflates domain statuses
  with a UI-only state, and the comments contradict the data.
- **Asymmetric vocabularies.** Statuses have an exported `as const` array, a derived type and a
  guard; triggers have an inline `known[]` rebuilt on every `runTriggerKey` call, no exported
  constant and no `RunTrigger` type. The two `...Key` functions are the same "validate, then prefix"
  algorithm written twice.
- **Loose signatures / stringly-typed rule.** `isRunStatus`, `runStatusKey` and `isRetryable` take
  `string` even where a `RunStatus` is expected; `isRetryable` hardcodes the `"failed"` literal, so
  a typo there would change retry affordances with no type error. `.includes` on an array cast to
  `readonly string[]` is O(n) and discards literal types.
- **No tests.** No unit test references this module (search of `apps/web/tests`, CONFIRMED). REQ-012
  coverage is only the Playwright golden, which never exercises unknown-value fallback or `paused`.
- **Untyped key returns.** Both builders return `string | null`, so a misspelled catalogue key is
  only caught at runtime via the `t()` fallback.

Invariants to preserve (REQ-012): every status renders as a distinct *named* chip (never colour
alone); retry is offered only for `failed`; unknown wire values degrade to the raw string, never a
crash or an empty chip. This helper is presentation metadata only — it must mirror, not reimplement,
the backend state machine.

## Proposed upgrades (v0.4.0)

### 1. Reconcile the vocabulary and make one source of truth (PROPOSAL)
Decide and document whether `paused` is a domain status or a UI-only state. Given the i18n keys and
the pause/resume control, keep it in the vocabulary but label it "UI-only" in `RUN_STATUSES`, and
correct both "eight status" comments to nine. Derive `RunStatus` from the array (already done) and
have `fixtures.ts` import `RUN_STATUSES` instead of re-declaring a second eight-value list.
*Risk:* if `paused` were dropped instead, `runStatusKey("paused")` would return `null` and the chip
would show the raw wire token despite an existing translation — keep it. If the golden's eight-chip
assertion is intentional, document why the seed omits `paused` rather than silently diverging.

### 2. Give triggers a first-class constant, type and guard (PROPOSAL)
Extract `RUN_TRIGGERS` (`as const`), `export type RunTrigger`, `isRunTrigger` and a module-level
`Set<string>` for each vocabulary, mirroring the status side; remove the per-call `known[]`.
*Risk:* the trigger list must exactly match the wire enum in the generated API types and the eight
`run.trigger.*` i18n keys (all present in en/fr/he, CONFIRMED). A value added on the wire but not
here falls back to raw text — acceptable, but keep the list and i18n keys in one commit.

### 3. Unify the two key builders behind one typed helper (PROPOSAL)
Add a private `catalogueKey(prefix, value, vocabulary)` used by thin, typed `runStatusKey(status)`
and `runTriggerKey(trigger)` wrappers so there is a single "validate then prefix" implementation.
Keep the public signatures (`string -> string | null`) unchanged for both call sites.
*Risk:* a generic helper can erase literal types; keep the wrappers typed and avoid `any`, and
never let the generic return a key for a value outside the set. Do not change the `null` fallback
contract without updating `activity-screen.tsx` and `thread-screen.tsx` together.

### 4. Make retryability data, not a hardcoded literal (PROPOSAL)
Have `isRetryable` accept `RunStatus` and read from a single `RETRYABLE_STATUSES` set (or a
`RUN_STATUS_META: Record<RunStatus, { retryable: boolean; terminal: boolean; i18nKey: string }>`
table that also feeds `runStatusKey`). A `satisfies Record<RunStatus, ...>` annotation makes a
missing status a compile error.
*Risk:* this must stay *presentation* metadata. A client-side "terminal" or transition table must
not claim authority over REQ-012/REQ-021 semantics owned by Rust; keep semantics in sync by contract,
and record any externally visible change as a DEVIATION.

### 5. Add fast, pure unit tests with REQ-012 in the name (PROPOSAL)
Add `apps/web/lib/run-status.test.ts`: `it("REQ-012 every status maps to a catalogue key")`,
`it("REQ-012 unknown status/trigger returns null")`, `it("REQ-012 retry only on failed")`, plus a
guard that every `RUN_STATUSES`/`RUN_TRIGGERS` member has an i18n key. Keep it pure (no catalogue
import if that couples lib to i18n — assert against a key list or `Record<RunStatus,string>` shape).
*Risk:* duplicating the key list in a test can drift; prefer a compile-time `satisfies` check plus a
runtime check over a shared exported key map. Gate on the REQ-012 golden replay as well.

## Non-goals / constraints

- No new externally visible API or data-format change without a DEVIATION entry and owner approval.
- Do not reimplement Rust transition/lease rules here; this is a view vocabulary only.
- No raw palette/hex classes or second design system (BoardUI rules); not UI-rendering code today.
- Keep the module dependency-free and synchronous so it stays trivially testable and tree-shakeable.
