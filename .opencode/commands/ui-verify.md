---
description: Verify the rebuilt UI with Playwright MCP plus a Playwright test suite
---
Verify the rebuilt UI.
Start the app bound to 127.0.0.1. State the port.
Explore with the Playwright MCP tools: navigate to http://127.0.0.1:<port>, take accessibility snapshots, click through each primary flow, read console messages and network requests. Say in one line what you are about to do when a step is destructive or writes data. Never point the browser at production.
Turn what you learned into tests in tests/ui/, one file per screen or flow, every name carrying its REQ id: test("REQ-xxx ..."). Replay every spec/golden/ui/*.json: the fixture maps state.seed to a seeded database, opens state.route at state.viewport and state.media, and asserts the expect entries (role, optional name, state, count). Cover loading, empty, partial, error and permission-denied states. Add a keyboard-only path per primary flow, axe via @axe-core/playwright failing on critical and serious, 360 px width, light and dark, prefers-reduced-motion, and visible focus.
Guard: the fixture aborts requests to production hosts. A test that needs the internet is a bug: report it instead of adding an exception.
Run the suite. Fix what fails, one commit per fix, then re-run until green. Do not lower the axe threshold or skip a golden to make it pass.
Write docs/UI-VERIFY.md: commands run, per-REQ pass/fail, axe summary, bundle size, LCP/INP/CLS on a throttled profile, screenshots of OUR UI (tests/ui/screenshots/), and every remaining finding. Update docs/STATE.md and docs/TRACEABILITY.md.
