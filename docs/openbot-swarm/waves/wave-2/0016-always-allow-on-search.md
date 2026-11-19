# 0016 — always-allow-on × search (parity proposal, Boma Bot)

- Brief: id 16, area `always-allow-on`, lens `search` (`docs/openbot-swarm/briefs-1000.jsonl`).
- Status: **PROPOSAL only.** No product code, spec, golden, catalogue or dependency changed.
- Provenance: upstream read as **data only** (HEAD `386085ff`, 2026-11-18) in `CHANGELOG.md`,
  `docs/architecture/computer-use.md`, `docs/architecture/structure.md`, `docs/architecture/agents.md`.
  No upstream code, asset, wording, icon, colour, font or brand enters Boma Bot; no Pro component,
  no second design system. Markers: **CONFIRMED** = read from a repo/upstream file this session;
  **GUESS** = recommendation needing the owner's decision (AGENTS.md: externally visible behaviour
  change is a DEVIATION).

## 1. Upstream capability, in my own words (inspiration only)

Some agent hosts let a person turn a repeated one-off approval into a **standing rule for a class of
action**: once set, later calls in that class raise no decision card and proceed. Restated generically,
with no upstream name or wording reused:

- The offer to make a decision standing appears only in the middle permission tier. The most-open
  tier never shows a card (the host answers its own prompts); the most-closed tier keeps prompting.
- Where the rule persists, it is scoped per tool and provider account, held in a file-backed layer the
  person owns; the host stores no consent of its own.
- Turning the rule on (or an admin widening access) also closes decision cards **already waiting** for
  that agent, instead of leaving each open.
- A less-trusted import downgrades an existing rule to a warning so a new agent never starts elevated;
  changing a rule discards a cached provider session that still carries the old one.
- A privileged completion is accepted once and **never stored**; forms, sign-in and browser takeovers
  stay interactive. The standing grant is a deliberate, named, reversible default — never silent.

**Permissive defaults stay out.** We do not adopt: the host silently answering its own prompts,
automatic promotion to a fully-open tier from one click, "never ask" as any default, an import silently
keeping an elevated grant, or the host storing consent on a person's behalf.

## 2. Search parity, grounded in Boma Bot's search RPC and screen (CONFIRMED)

The `search` lens asks how a standing-allow policy would show up on, and be found through, search.

- **RPC:** `search` is one of the 52 declared namespaces in `spec/03-api-contract.md` §3.1 (line 29),
  but it is **not** in `crates/api/src/rpc.rs` `namespaces`/`procedures()` and there is no `search.*`
  wrapper in `apps/web/lib/api/rpc.ts`, no `spec/golden/search*.json`, and no REQ acceptance criteria.
  So there is no live search RPC to call; any `search.*` behaviour is a **GUESS**. We invent none.
- **Screen — workspace search:** `/app/artifacts` `ArtifactsGrid` (`apps/web/components/artifacts/
  artifacts-grid.tsx`) filters `artifactsListSpace` client-side via a labelled `type="search"` input
  (`artifacts.search`, REQ-036, spec/08 §8.5); `apps/web/tests/artifacts.spec.ts` already covers it.
- **Screen — navigation search:** the dashboard sidebar (`apps/web/components/application/dashboard/
  dashboard-sidebar.tsx`) has an inline `⌘/Ctrl-L` filter over local labels — presentation only, no RPC.
- **Gap:** neither search surface has an approval gate or a standing policy today. An
  "always-allow-on × search" has **no exact counterpart** here; the closest honest mapping is a
  presentation-only disclosure of the scope/policy that governs a search, plus a discoverable route to
  reverse a future standing allowance — not a permission write.

## 3. Search-lens change (read-only first, reversible)

1. **Disclose scope, not a grant.** On `/app/artifacts`, show a read-only `role="region"` line under the
   filter toolbar naming what the search covers (this space) and that no standing permission is used
   to widen it. It answers "is this search gated by an always-allow rule?" without claiming a grant.
2. **Discoverable, not activatable.** If a future standing rule ever governs a result, the same line is
   where its scope and reversal path are named. The first slice renders **no toggle/switch** and makes
   no policy write; a "Review" control is a later slice (new REQ id, ADR, PARITY DEVIATION, owner
   approval per AGENTS.md). Record it in `docs/STATE.md`, not in code.
3. **Keep the contract.** REQ-017 (per-space/per-user policy), REQ-018 (exactly-once effects),
   REQ-036 (artifacts + search), REQ-043 (tenancy) and REQ-057 (i18n/RTL) stay authoritative. No new
   procedure, field, golden or vocabulary; no upstream tier names.

## 4. BoardUI surface (one design system, semantic tokens only)

In `artifacts-grid.tsx`, insert a read-only disclosure region below the toolbar, inside the existing
`bg-background-secondary-default` / `border-separator-border` / `rounded-3xl` shell, `gap-2`, merged
with `cx()` from `@/utils/cx`. Type via `text-body-2-regular` / `text-caption-1-semibold` in
`text-text-secondary` / `text-text-tertiary`. One `@remixicon/react` glyph as a component reference
(e.g. `RiShieldCheckLine`), `aria-hidden`, `text-foreground-icon-secondary`. Focus ring
`focus-visible:ring-2 focus-visible:ring-border-focus-ring`; targets `.boma-target` (44 px). Copy from
`t()` — add `artifacts.search.scope` and `artifacts.search.standing.none` to the `en`/`fr`/`he`
catalogues in `apps/web/lib/i18n.ts` (English fallback for the long tail, REQ-057). No raw palette,
no hex/oklch, no `dark:` literal (dark mode rides tokens), no Pro component. Note (GUESS, separate
cleanup): the file's raw `<input type="search">` and `<select>` are candidates for installed free
BoardUI `SearchField`/`Select`; not part of this slice.

## 5. One REQ-named Playwright test

In `apps/web/tests/artifacts.spec.ts`, add **exactly one** test, distinct from the existing REQ-036
search/filter/viewer cases, using the existing `seedGrid` helper:

```ts
test("REQ-036 the artifacts search states its scope and uses no standing permission", async ({ page }) => {
  await seedGrid(page);
  await page.goto("/app/artifacts/");
  const scope = page.getByRole("region", { name: t("en", "artifacts.search.scope") });
  await expect(scope).toContainText(t("en", "artifacts.search.standing.none")); // no widening grant
  await expect(scope.getByRole("switch")).toHaveCount(0); // disclosure, not an always-allow control
  await page.getByRole("searchbox", { name: t("en", "artifacts.search") }).fill("chart");
  await expect(page.getByText("chart.png")).toBeVisible();
  await expect(page.getByText("notes.txt")).toHaveCount(0); // scope unchanged by the disclosure
});
```

Pins the search-lens contract: a named, read-only scope disclosure, no standing-grant wording, no
toggle, and search still narrows. No new golden is required (state, never a click path).

## 6. First slice (reversible, test-first, small)

1. Write the failing REQ-036 test above.
2. Add the two `artifacts.search.scope`/`artifacts.search.standing.*` keys to `en`/`fr`/`he`; no other strings.
3. In `ArtifactsGrid` only, render the read-only disclosure region; leave the search input, filters,
   grid and viewer untouched.
4. **Do not** add a policy RPC, an always-allow toggle, or "supersede waiting cards" here — those change
   permission semantics and need a new REQ id, ADR, PARITY DEVIATION and owner approval. Log in
   `docs/STATE.md` as a later slice.
5. Gates (`AGENTS.md`): web typecheck, lint, build, Playwright artifacts/axe clusters (light, dark,
   RTL), 360 px width; update `docs/STATE.md` (CONFIRMED/GUESS) and `docs/UI.md`; no dependency change.

## Guardrails

- No upstream code, asset, wording or brand; analysis is inspiration only. No Pro, one design system.
- **GUESS:** the spec declares a `search` namespace but nothing is wired; confirm before any future
  `search.*` read/write slice. No live search RPC was exercised — this document ran no build or test.
- Every claim is CONFIRMED only after a command actually ran; no test-pass claim is made here.
