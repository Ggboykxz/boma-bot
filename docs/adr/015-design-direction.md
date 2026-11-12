# ADR-015 — Design direction: fresh design with BoardUI, references only

Status: proposed (awaiting owner go) — this is DEC-12

## Context

`spec/08-ui-ux.md` fixes behaviour, layout regions and states, not pixels. The reference's brand,
fonts, icons and screenshots are not covered by its source licence, and no captures exist yet
(`/ui-probe` never ran). AGENTS.md defaults to a fresh design and forbids shipping an uncleared asset.

## Alternatives

1. **Fresh design built from BoardUI components and semantic tokens; the reference's interaction model
   is the specification, its appearance is not.**
2. Faithful visual parity: reproduce palette, type and spacing from captures.
3. A third-party template (a paid BoardUI template or similar) as the base.

## Scoring

| Criterion | 1 fresh | 2 parity | 3 template |
|---|---|---|---|
| 1 fits the spec | 1 | 1 | 2 (template structure fights the spec's regions) |
| 3 security | 1 | 2 (uncleared assets) | 2 |
| 5 simplicity | 1 | 3 (pixel archaeology without captures) | 2 |
| 7 licence | 1 | 4 | 2 |
| 8 a11y/i18n | 1 (built to WCAG 2.2 AA from the start) | 3 (inherits contrast and target sizes) | 1 |

## Decision

Option 1. BoardUI provides the component base, the semantic token set, the type scale and dark mode;
the design work is layout, density, states and motion. Nothing from the reference's visual identity
is imported: no logo, font, icon set, colour value or copy. `docs/UI.md` maps every screen to its
components and states; `docs/DEPENDENCIES.md` records every icon, font and package with its licence.

## Consequences

- `/ui-parity` compares behaviour and structure, not pixels; `docs/UI-PARITY.md` lists intentional
  visual differences.
- BoardUI Pro templates are optional and not required by the spec; if used, the licence terms are
  checked before the repository becomes public (Pro source may not be redistributable).
- Accessibility is designed in: tokens are checked for 4.5:1 text contrast in both themes, targets are
  at least 44 px where the spec has touch, and focus rings come from tokens rather than ad-hoc values.
