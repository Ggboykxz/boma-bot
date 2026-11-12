---
description: Systematic improvement pass beyond parity, measured before/after
---
Run a systematic improvement pass beyond parity, using docs/QUALITY_BAR.md as the checklist. For each category (security, reliability, performance, code quality, developer experience, observability, UX and accessibility if there is a UI):
(a) measure the current state with real tools (clippy pedantic review, cargo deny, cargo machete, cargo bench or criterion, a profiler such as cargo flamegraph or samply, a load test of the hot endpoints, bundle and accessibility tools for the UI, cargo mutants on the domain crate time-boxed to what is reasonable),
(b) list the findings by severity,
(c) fix those inside the spec's scope, one commit each,
(d) re-measure with the same command and the same machine.
For the UI, measure before and after: axe critical/serious count, bundle KB, LCP/INP/CLS on a throttled profile, keyboard coverage, 360 px and RTL screenshots of OUR UI.
Any externally visible behavior change is a DEVIATION: do not apply it; add it to docs/PARITY.md as PROPOSED.
Write docs/IMPROVEMENTS.md with before/after numbers per category and the exact commands used. Proposals outside the spec go to docs/STATE.md. Do not improve a number by weakening its gate.
