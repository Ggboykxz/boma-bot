---
description: Read the spec, decide the stack and produce the rebuild plan (stops for my go)
---
If spec/01-requirements.md is missing, STOP and tell me to run /spec in the analysis workspace, then tools/export-spec.sh. Otherwise read everything in ./spec (start with spec/README.md and spec/EXPORT.md), then write docs/PLAN.md containing:
1. Summary of the software to rebuild in 5 lines, plus the count of MUST/SHOULD/COULD requirements and the number of golden examples (cross-check with spec/EXPORT.md).
2. Stack decisions. Backend: Rust, as mandated. For everything else (data store, UI, mobile, desktop, CLI, jobs, messaging, auth, deployment, CI) YOU decide using the criteria in AGENTS.md, one ADR each in docs/adr/NNN-title.md with at least 2 alternatives. Do not ask me to choose a stack. Present the decisions as a table (decision, choice, main reason) so that I only veto.
3. Target architecture: mermaid diagram of crates and modules and the data flow, boundaries, folder layout.
4. Performance budget (p95 latency, memory, startup time, binary and bundle size as relevant) and security approach, from spec/04-nonfunctional.md and docs/QUALITY_BAR.md.
5. Milestones M0..Mn, each with the REQ ids covered, exit criteria and size (S/M/L). M0 = scaffold and CI; then a walking skeleton; then vertical slices in dependency order; last = hardening, migration and release.
6. Improvement map: each defect of spec/05-known-defects.md -> how it will be fixed -> in which milestone.
7. Test strategy: unit, property, integration, contract tests against spec/openapi.yaml, golden replay of spec/golden/, e2e if there is a UI, and how REQ ids map to test names.
8. Risks, and open questions. Copy the unresolved items of spec/06-decisions-needed.md into docs/STATE.md under "Decisions needed". Mark claims CONFIRMED/GUESS.
9. UI/UX (only if spec/08-ui-ux.md exists): framework and rendering mode (ADR, at least 2 alternatives), design direction (faithful visual parity or fresh design; references in ../re-workspace/notes/ui/), design system (BoardUI with its theme tokens and rules; alternatives: hand-rolled Tailwind, shadcn/Radix, a full component framework), router, state and data-fetching layer, i18n/RTL, WCAG level (2.2 AA by default), budgets (LCP, INP, CLS, JS KB), Playwright and axe strategy, and the asset/license plan for anything reused from the original. Do not ask me to pick: decide and let me veto.
Then STOP: print (a) the stack decision table and (b) the numbered decisions I must answer, and wait for "go" or my vetoes and answers before anything else. Write nothing in the code folders during this command.
