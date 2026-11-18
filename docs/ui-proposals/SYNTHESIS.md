# UI proposals — 100 sector specialists (2026-11-18)

## What ran

One hundred read-only subagents, each assigned one industry sector, produced one English proposal
in this directory (`001-…` to `100-…`). They read `docs/UI.md`, `spec/08-ui-ux.md` and real
components/RPCs; **no product code was touched**, no Pro component was used, and no upstream
brand, code, wording or asset entered the repository. Total: ~1.2 MB of grounded proposals.

## Cross-sector signal

Counting the "highest-impact first slice" of each proposal:

- **Approval card / decision card — 72 of 100 sectors.** Regardless of industry, the first thing
  people want is an approval surface that explains *what* will happen, *why*, and *what happens if
  they say no*, without needing the transcript. The most referenced REQ ids confirm it:
  REQ-007 (338 mentions), REQ-017 (187), REQ-012 (182), REQ-011 (151).
- **Board / triage surface — 71 sectors.** A stable place to see queued, working, waiting and
  failed work (REQ-038 174, REQ-036 149), with filters that do not rely on colour alone.
- **Artifact / shared-page library — REQ-037 (118)** and **activity/runs triage — REQ-001 (132)**.
- **i18n/RTL — REQ-057 (108)** appears as a cross-cutting requirement in nearly every proposal,
  not a sector-specific ask.

Installed BoardUI components dominate the proposals (`Button` 96, `Table` 94, `Tooltip` 93,
`Badge` 93, `Select` 78, `Input` 77, `Drawer` 76) — the free tier already covers what the sectors
need. The newer `agent-thinking`/`composer-loader`/`file-upload` were not known to the agents.

## Recommendation (priority order)

1. **One generic contextual approval card, reused by every sector.** Make REQ-007/017 cards carry
   a structured "effect · reason · reversal" summary, a keyboard path, explicit status text,
   RTL-safe layout and reduced-motion behaviour. This single change answers 72 sectors instead of
   building 72 variants. Needs a short design + ADR (wire shape and new fields are a deviation).
2. **Board/triage upgrade with saved filters** (REQ-036/038): queued/working/waiting/failed as
   text + token, status counts, and a focus ring that survives filtering.
3. **Activity list with sector-agnostic run context** (REQ-001/012): the same run vocabulary the
   100 proposals name, no extra jargon.
4. **Locale/RTL assertion** (REQ-057): a test that asserts shipped catalogue count, so the
   already-open issue #16 cannot drift again.

## Discipline

Each item above must be spec-backed, carry REQ ids in its test names, keep the semantic-token-only
rule, and leave `trace --max-missing 7` and the two release boot gates green. Nothing here is a
commitment yet: the owner picks the slice to implement next.
