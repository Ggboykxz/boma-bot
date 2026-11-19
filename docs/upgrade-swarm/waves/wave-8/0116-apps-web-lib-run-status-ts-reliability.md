# Wave 8 / worker 0116 — `apps/web/lib/run-status.ts` — reliability (v0.4.0)

Status: CONFIRMED (read from the tree at `before-m5`, 2026-11-19). Read-only audit; no code changed.
Scope: a 51-line, dependency-free adapter between the Rust wire vocabulary and the web UI —
`RUN_STATUSES` (9 snake_case values), the `RunStatus` type, `isRunStatus`, `runStatusKey`,
`runTriggerKey`, `isRetryable`. Consumers: `components/activity/activity-screen.tsx`
(status/trigger labels, Retry, pause/resume) and `components/thread/thread-screen.tsx` (run label
and the "active" predicate).

## Failure modes

### FM-1 — Unknown status variants degrade into raw wire tokens (CONFIRMED)
`isRunStatus` returns false for any value Rust adds later; `runStatusKey` then returns `null`, and both
consumers fall back to the raw string (`activity-screen.tsx:131`, `thread-screen.tsx:310`). The user
sees `awaiting_approval` instead of a localized label. `isRetryable` returns false: it fails *closed*
(good — no illegal retry) but silently hides a legal action if Rust ever permits retry from another
status. The helpers accept `string`, not `RunStatus`, so every call repeats a runtime check that
cannot be discharged statically.

### FM-2 — Drift from the Rust transition table (CONFIRMED)
Rust owns `RunStatus::ALL: [Self; 9]` and `RunTrigger::ALL: [Self; 8]`
(`crates/domain/src/run.rs:43,186`). The web keeps five more hand-copied vocabularies: this module's
`RUN_STATUSES`; the local `known` array in `runTriggerKey` (`:35-44`, reallocated on every call);
`runIsActive` inline in `thread-screen.tsx:312`; `tests/fixtures.ts:376` and
`scripts/screenshots.cjs:139` (both list 8 statuses — `paused` is missing). Adding a Rust variant is
invisible to all of them at build and test time. `isRetryable` hardcodes `status === "failed"`,
mirroring `Failed => Queued` and REQ-012, with no link to `can_transition_to` (`:100-119`).

### FM-3 — Stale labels (CONFIRMED)
The header comment (`:4-6`) and the guard doc (`:23`) already say "eight statuses" while the array
has 9 — the label is wrong today. `t()` falls back to English and then to the key itself
(`lib/i18n.ts:1218-1227`): all 9 statuses currently have entries in en/fr/he (`:162-178`, `:515-531`,
`:867-883`), but any status added without catalogue entries renders literally `run.status.<new>`.
No test asserts catalogue parity with `RUN_STATUSES`.

### FM-4 — Policy predicates are unpinned (CONFIRMED)
No unit tests target this module (grep found imports only). `isRunStatus` and `isRetryable` encode
policy and have no regression guard.

## Concrete hardening for v0.4.0

**H-1 Generate the vocabulary, do not copy it.** Prefer deriving `RUN_STATUSES` and the trigger list
from the OpenAPI enum emitted by `utoipa`; interim, add a CI contract test reading the wire values
from the generated document and asserting set equality with both lists. Risk: the generated enum may
be named/flattened differently — pin the comparison to the schema `$ref`, never a guessed path. If
the OpenAPI pipeline is not stable yet, use a checked-in golden exported by a Rust test/script, never
a hand-edited list (that only relocates FM-2).

**H-2 Make the trusted path total.** Keep `isRunStatus` for untrusted wire data, add
`runStatusKeyOf(status: RunStatus): string` that cannot return `null`, and have typed consumers use
it. Narrow `isRetryable` to `RunStatus` (or keep a `string` wrapper that still fails closed). Risk:
RPC DTOs expose `string`, so parsing must be centralised where the response is decoded, not scattered
across components.

**H-3 Render unknowns explicitly.** Replace the `statusKey === null ? run.status : t(statusKey)`
fallback with a dedicated `run.status.unknown` catalogue key, plus a deduplicated counter/warning
carrying the raw value (no PII). Users get a localized "Status unavailable"; operators keep the raw
value in logs. Risk: a new key across 14 locales — ship English first and rely on `t()` fallback;
sample the warning so a list of unknown rows cannot flood logs.

**H-4 Catalogue parity test.** For every status and trigger key, assert `t("en", key) !== key`, and
repeat for every locale that already defines any `run.*` key. Risk: en/fr/he pass CONFIRMED; other
locales are not yet populated for these keys, so scoping the assertion avoids inventing translations
in this change.

**H-5 De-duplicate the display predicate.** Move `thread-screen.tsx:312` into an exported
`isActiveForDisplay` here, documented as intentionally broader than Rust `is_working()` (which
excludes the two waiting states). Risk: naming it `isWorking` would merge two distinct concepts.

**H-6 Correct the stale data and docs now.** "Nine statuses" in the header/guard docs; add `paused`
to `tests/fixtures.ts` and `scripts/screenshots.cjs` so fixtures match production. Risk: golden
screenshots change — regenerate them and record any visible diff in `docs/PARITY.md`.

## Tests to add (REQ id in the name where relevant)

- `it("REQ-012 isRetryable accepts only failed and fails closed for unknown statuses")`
- `it("the run-status vocabulary matches the OpenAPI RunStatus and RunTrigger enums")`
- `it("every run.status and run.trigger key resolves in the English catalogue")`
- `it("an unknown status renders a localized fallback, not a snake_case token")` (Playwright)

## Risks and open decisions

High value/high coupling is H-1; do not couple v0.4.0 to an unstable generator — take the golden
fallback if needed. H-3 changes user-visible text for unknown values only; record it in
`docs/PARITY.md` if it counts as a DEVIATION. Decisions for the operator: (1) accept the new
`run.status.unknown` key; (2) confirm `isActiveForDisplay` is intentionally broader than Rust
`is_working()` before freezing it as a contract; (3) generated enum vs checked-in golden for v0.4.0.
