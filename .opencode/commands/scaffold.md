---
description: Create the project skeleton (milestone M0) as decided in docs/PLAN.md
---
Create the project skeleton as decided in docs/PLAN.md and my answers (milestone M0). Include:
- Cargo workspace and crates per the plan; rust-toolchain.toml pinned (stable, clippy, rustfmt, llvm-tools-preview); [workspace.lints] and clippy.toml as required by AGENTS.md "Rust standards"; rustfmt.toml; deny.toml (licenses allow-list compatible with an open-source release, advisories deny, bans on duplicate crate versions as warnings, sources restricted to crates.io); .cargo/config.toml only if needed; [profile.release] tuned.
- Config module with typed environment config; tracing setup; /healthz and /readyz; graceful shutdown; a first error type and the common API error format.
- Test harness: one passing unit test, one integration test in tests/, the golden-test runner that replays spec/golden/*.json (initially skipping unimplemented REQs explicitly, never silently), the UI golden runner for spec/golden/ui/*.json (skipped with a visible reason when the spec has no UI), and the contract-test skeleton comparing the generated OpenAPI document with spec/openapi.yaml. Every test name carries its REQ id.
- The non-backend parts decided in the ADRs (UI, mobile, CLI...), minimal but buildable, with strict lint and type-check configs.
- If the spec has a UI: the app shell, the theme wired to BoardUI tokens, playwright.config.ts with a webServer that binds 127.0.0.1 by default, an axe fixture (@axe-core/playwright) that fails on critical and serious, a route guard that aborts requests to production hosts, one passing end-to-end test with a REQ id, and make targets ui-run, ui-test and ui-a11y.
- Makefile (or justfile) with install, run, fmt, lint, typecheck, test, coverage, deny, bench and build targets.
- .env.example; a multi-stage Dockerfile for the backend only if the plan needs one; CI workflow (fmt --check, clippy -D warnings, nextest, cargo deny, llvm-cov threshold, machete, secret scan, trace_check, and the UI suite with axe when there is a UI); README quickstart; docs/DEPENDENCIES.md; a LICENSE file containing "TO BE CHOSEN BY THE OWNER".
Install what the plan needs, run the whole toolchain once and fix until green. Commit ("chore: scaffold"). Update docs/STATE.md.
