---
description: Implement a milestone or REQ ids, test-first (usage /implement M1 or /implement REQ-001 REQ-002)
---
Implement $ARGUMENTS (a milestone such as M1, or a list of REQ ids) following docs/PLAN.md. For each REQ in scope:
1. Write failing tests from its acceptance criteria, with the REQ id in each test NAME (for example fn req_012_refund_is_idempotent(), test("REQ-120 empty order list")), including the golden examples of spec/golden/ (or spec/golden/ui/) for that REQ and a property test when the rule is a critical invariant. A UI requirement is tested with Playwright in tests/ui/, keyboard included.
2. Implement the minimum clean code to pass, respecting AGENTS.md "Rust standards" and "UI/UX policy".
3. Refactor, then run cargo fmt --check, clippy with -D warnings, and the whole test suite.
4. Commit with a Conventional Commit message mentioning the REQ id.
5. Add the REQ to docs/TRACEABILITY.md (REQ -> test names -> code locations -> status).
After the last REQ run `python3 tools/trace_check.py` and close every gap it reports (a comment is not evidence; rename the test instead). If a gate is genuinely wrong, fix the gate with a note in docs/STATE.md, not by deleting the test. If the spec is ambiguous, apply the rule of AGENTS.md (note it, simplest safe default, GUESS) and continue. Finish by updating docs/STATE.md.
