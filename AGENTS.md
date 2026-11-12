# Rebuild Workspace (Rust backend, parity-first)
Mission

Rebuild, from the specification in ./spec, a new software that does everything the spec requires and does it better. The backend is written in Rust, and the user interface, when the spec has one, is built with BoardUI. You may consult the original workspace and the running original whenever it helps; the spec remains the contract. Reply to me in informal French. Everything you write to disk (code, docs, commit messages) is in English, except user-facing texts when the spec says otherwise.

The product you are building is called Boma Bot. Use that name for the product, the binaries, the package scope (@boma/*), the environment prefix (BOMA_*), the app ids (com.boma.app) and any user-visible string. Never ship the original's brand, domain, package scope or identifiers; the analysed tree in ../re-workspace keeps its own names so its citations stay valid.

Source of truth

Work from ./spec. If ./spec has no 01-requirements.md, stop and tell me to run /spec in the analysis workspace and then tools/export-spec.sh.
The spec wins on behavior; the original is evidence. When they disagree, write the conflict in docs/STATE.md and ask me.
You may read ../re-workspace (target, notes, notes/ui, captures) and run the original next to the new app. Record what you consulted and why in docs/STATE.md.
Prefer reimplementation over copy-paste. If you reuse code, an asset, a font or wording, record its origin and license in docs/DEPENDENCIES.md and docs/UI.md.
If the spec is ambiguous or silent, do not guess silently: write the question in docs/STATE.md under "Decisions needed", pick the simplest safe default, mark it GUESS and continue.
Public interface names listed in the spec (endpoints, fields, flags, file formats) are kept for compatibility.
Never bypass licensing, DRM, authentication, anti-tamper or any protection. Scope: only software I own or am authorized to reproduce or replace.
Dependencies: well-maintained open-source libraries with a license compatible with the license I will choose (enforced by cargo-deny, checked by hand for npm). Record each in docs/DEPENDENCIES.md (name, version, license, why). Add Rust dependencies with cargo add and read docs.rs for APIs; never type versions or APIs from memory.
./spec is editable if it is wrong. Note every edit in docs/STATE.md and refresh spec/EXPORT.md's provenance table when you do.
Gates are yours, honesty is not

You may edit ./spec, .opencode/, .claude/, tools/, AGENTS.md and opencode.json. Nothing is denied by configuration.
You may change a gate, but never silently: write an ADR (or a STATE.md entry for small ones) explaining why, and keep the gate at least as strict for everything else. A test that passes because it was weakened is a lie.
BoardUI's install_rules may refresh its own fenced block inside AGENTS.md. Review the diff; never hand-edit that block.
Stack policy

Backend: Rust, mandatory.
Everything else (database, web UI, mobile, desktop, CLI, background jobs, messaging, auth provider, deployment, CI) is YOUR decision. Do not ask me to pick a stack: decide, justify in an ADR (docs/adr/NNN-title.md: context, at least 2 alternatives, scoring against the criteria below, decision, consequences), and I will veto if I disagree.
Criteria, in this order: (1) fits what the spec actually requires (no UI in the spec means no UI; an API or library only means Rust everywhere); (2) maturity and active maintenance, checked on the real registry or repo; (3) security track record; (4) performance on low-end devices and poor networks; (5) simplicity for a single maintainer: boring technology, few moving parts, as few languages and services as the job allows; (6) a type-safe contract with the Rust backend (generate clients from the OpenAPI document, never hand-write duplicates); (7) license compatibility; (8) accessibility and i18n support.
Default Rust picks (deviate only with an ADR): Tokio, Axum with tower and tower-http, serde, SQLx (compile-time-checked queries, offline metadata committed) or another justified data layer, thiserror in libraries and anyhow at binary edges, tracing with tracing-subscriber, utoipa (or equivalent) to generate the OpenAPI document, secrecy for secrets, argon2 for passwords, rustls (avoid OpenSSL), proptest, wiremock, testcontainers (or SQLite in-memory if the ADR says so), criterion or divan for benchmarks.
Default layout (deviate only with an ADR): a Cargo workspace with crates/ domain (pure business rules, no IO, no async), app (use cases and ports), infra (database and external adapters), api (HTTP layer, DTOs, OpenAPI), and a binary crate; migrations/; tests/ for integration, contract and golden tests.
If the spec has a UI: BoardUI through its MCP server for components, theme tokens and design rules, and Playwright Test with @axe-core/playwright for end-to-end and accessibility tests. BoardUI targets the Tailwind v4 / shadcn ecosystem: confirm the framework requirement with boardui_get_started before you commit to it, and if the spec forces another framework, that is an ADR (you may still reuse the semantic token roles and the design rules). The rendering mode, the state/data layer and the router are decisions with an ADR each.
UI/UX policy (only if the spec has a UI)

The UI is part of parity: every UI requirement (spec/08-ui-ux.md and the ## UI block of spec/01-requirements.md) is implemented and covered by a Playwright test whose name carries the REQ id, exactly like backend requirements.
Two valid directions, decide with an ADR and let me veto: (a) faithful visual parity — reuse the original's palette, type, spacing, wording and assets when their license allows it; (b) a fresh design that answers the same behavior. Default: (b), with the captures in ../re-workspace/notes/ui/ as references. Never ship an asset you have not cleared.
Design system: BoardUI components, theme tokens, always-on rules and agent skill. The fenced BoardUI block in AGENTS.md is binding for tokens, spacing, type scale, dark mode and cx(); it never overrides a spec requirement — a conflict is a decision for me.
BoardUI Pro: I paste the license key in the conversation and boardui_activate_license stores it on the machine. Keep it out of files, commits, logs, notes, docs/ and spec/. One seat per machine: check boardui_license_status before activating, and never deactivate_license without my say-so. A throwaway container may need re-activation.
Vendored components: the copied source is ours to adapt. Keep local edits minimal, list them in docs/UI.md, diff before re-running an install (it overwrites), record every dependency's license in docs/DEPENDENCIES.md. Before making the repository public, check BoardUI's terms: if Pro source may not be redistributed, keep it out of the public repo and install it at build time.
Playwright may drive the NEW app and the ORIGINAL app, both on localhost or a host I control. Use /ui-parity for side-by-side screenshots, DOM/accessibility diffs and keyboard paths. Never point it at production, a search engine or a third-party site.
Accessibility: WCAG 2.2 AA is a gate. Zero critical or serious axe finding, a keyboard-only path for every primary flow, visible focus, 360 px width, light and dark, prefers-reduced-motion, and no information conveyed by color alone.
Performance: LCP, INP, CLS and a JS bundle budget fixed in docs/PLAN.md, measured by /ui-verify and /audit with the same command on the same machine.
UI goldens: replay every spec/golden/ui/*.json. A golden asserts a seeded state, not a click path. A mismatch is a defect or a DEVIATION.
i18n/RTL: if the spec requires it, every string goes through the i18n layer from day one, no hardcoded user-visible text, and tests cover dir=rtl.
One design system, one theme. Never install a component library found by chance, never add a second design system, never hand-edit the BoardUI skill.
Rust standards (mandatory)

Pin the toolchain in rust-toolchain.toml (stable, with clippy, rustfmt and llvm-tools-preview). Commit Cargo.lock. Use --locked in CI. Set rust-version.
Define lints in [workspace.lints]: unsafe_code = "forbid" (an exception needs an ADR); clippy all and pedantic as warnings (allow individual pedantic lints only with a comment); deny clippy unwrap_used, expect_used, panic, todo, unimplemented, dbg_macro and print_stdout in non-test code, with clippy.toml allowing unwrap, expect and panic in tests. CI runs clippy with -D warnings.
Errors: typed errors with thiserror in libraries, anyhow only at binary edges, one stable API error format, never leak internals to clients.
Async: no blocking calls on the async runtime (use spawn_blocking), no lock held across .await, bounded channels, a timeout on every outbound call, graceful shutdown that drains in-flight requests, cancellation safety considered.
Types: newtypes for ids, money and units; make invalid states unrepresentable; money never as floating point (integer minor units or a decimal type).
HTTP: request body limits, timeouts, compression, CORS allow-list, request ids, security headers, strict deserialization (deny_unknown_fields where the spec requires it), validation at the boundary.
Persistence: versioned migrations, parameterized queries only, transactions for multi-step writes, idempotency keys where the spec has retries or payments, no N+1 queries.
Config: typed config struct from environment variables, validated at startup (fail fast); secrets wrapped in secrecy and never logged.
Observability: structured JSON logs with tracing, request ids, /healthz (liveness) and /readyz (readiness), a metrics endpoint; no secrets or personal data in logs.
Security: Argon2id if the spec has passwords; session or token expiry and rotation per spec; constant-time comparison for secrets; rate limiting; rustls.
Tests: unit tests in domain; property tests (proptest) for critical rules; integration tests in tests/ against a real database; golden tests replaying spec/golden/*.json; UI golden replay and Playwright end-to-end tests for UI requirements; a contract test comparing the generated OpenAPI document with spec/openapi.yaml (any difference is a DEVIATION); property or fuzz tests for every parser of an external format.
Traceability: the REQ id must appear in the test NAME (fn req_012_refund_is_idempotent(), it("REQ-012 ..."), test("REQ-012 ...")). tools/trace_check.py only counts declaration lines, never comments.
Performance: benchmark hot paths; [profile.release] tuned (lto, codegen-units=1, strip), keep unwinding unless an ADR says otherwise.
Docs: rustdoc on public items of library crates; ADRs for decisions.
What "better" means

See docs/QUALITY_BAR.md. In short: full parity with every MUST requirement, then fix every defect in spec/05-known-defects.md, then improve security, performance, tests, developer experience, observability and UX inside the spec's scope. Any externally visible behavior change (API, data formats, CLI) is a DEVIATION: record it in docs/PARITY.md with a justification and wait for my approval before shipping it. Do not add features outside the spec without asking; list them in docs/STATE.md as proposals.

Working method

At the start of every session read docs/STATE.md; before ending update it (current milestone, done, next, decisions needed, what you consulted in the original).
Test-first: for each REQ-xxx write the tests from its acceptance criteria BEFORE the implementation. A UI REQ gets a Playwright test, not a unit test.
Small steps. After each one run cargo fmt --check, clippy, and the tests. Commit after each requirement or coherent change, with a Conventional Commit message that mentions the REQ ids. Push when a remote is configured, and tell me when you do.
When the original runs, use it as an oracle for differential tests (same input, compare output), never as a source to copy. Record the comparison in docs/PARITY.md.
Mark every claim in docs/ as CONFIRMED (verified by a command, test, capture or tool output you ran in this session) or GUESS (inferred). Never claim tests pass without having run them.
After /ui-kit, restart opencode so the BoardUI skill and the always-on design rules are loaded. MCP tools and skills are bound at session start.
/parity and /audit run as the auditor agent: by convention it writes only under docs/ and never fixes code. Switch back to the build agent afterwards.
Never put secrets in the repo: environment variables, ship .env.example. That includes the BoardUI license key.
Slash commands

/plan, /adr , /scaffold, /implement , /parity, /improve, /audit, /release , /resume, /ui-kit, /ui-verify, /ui-parity

<!-- boardui:rules:start -->
# BoardUI design rules

This project uses BoardUI (React + Tailwind CSS v4, source-owned components under `components/`). These rules always apply when writing UI code. MCP tools are on demand; these rules are not optional context.

## Components first

- Before hand-building any UI element, check for an installed BoardUI component under `components/base/` and `components/application/`, and prefer it.
- Missing a component? Install it (BoardUI MCP `install_components`, or `npx boardui@latest add <name>`) instead of writing a lookalike.
- Import through the `@/` alias, e.g. `import { Button } from "@/components/base/buttons/button"`.

## Color: semantic tokens only

- Never use raw palette classes (`text-gray-500`, `bg-white`, `border-neutral-200`) or hex/oklch literals. Every color rides a BoardUI semantic token, which also makes dark mode automatic.
- Text: `text-text-primary`, `text-text-secondary`, `text-text-tertiary`, `text-text-placeholder`, errors `text-text-error-primary`.
- Surfaces: `bg-background-primary-default`, `bg-background-secondary-default` / `-hover`, `bg-background-tertiary-default`, page ground `bg-background-full`.
- Borders: `border-border-button-default` / `-hover`, hairlines `border-separator-border`, tables `border-border-table`, errors `border-border-error-default`.
- Icons: `text-foreground-icon-primary` through `text-foreground-icon-quaternary`.
- Charts: the `chart-1` … `chart-5` tokens (plus `-active` variants). CTAs and selection states: the `accent-50` … `accent-950` ramp.
- Dark mode flips tokens via the `.dark` class on `<html>`. Do not write `dark:` overrides with raw colors; if a token pair looks wrong in dark mode, pick a different token, not a literal.

## Typography: composite utilities only

- Use BoardUI's composite type utilities: `text-title-1-medium`, `text-title-2-medium`, `text-title-3-semibold`, `text-headline-medium`, `text-body-medium`, `text-body-regular`, `text-body-2-*`, `text-caption-1-semibold`, and friends. Each sets size, weight, line-height, and letter-spacing together.
- Never rebuild type by stacking `text-sm font-medium leading-5`; if a style seems missing, look in `styles/typography.css` before inventing one.

## Spacing and shape

- Stay on Tailwind's spacing scale (`gap-2`, `p-4`, `mt-6`); prefer flex/grid `gap` over per-child margins. Arbitrary values (`p-[13px]`) only when matching an existing BoardUI component exactly.
- Cards and panels: `rounded-3xl` with `border-border-button-default`. Inputs and menu rows: `rounded-md` to `rounded-xl`. Pills: `rounded-full`.

## Mechanics

- Direction: wrap the app (or a mixed-language region) in `DirectionProvider` from `@/components/foundations/direction/direction`, with an explicit BCP 47 `locale` (`ar`, `he`, `en-US`, `zh-CN`). Set matching `lang` and `dir` on `<html>` for full-app RTL. CSS `dir` alone does not configure React Aria keyboard navigation or portal placement. Use `useDirection()` for custom interactions and put its value on custom portal roots.
- Use logical styles: `ms`/`me`, `ps`/`pe`, `start`/`end`, `text-start`/`text-end`, `border-s`/`border-e`, `rounded-s`/`rounded-e`, and inline logical CSS properties. Mirror navigation/disclosure glyphs and horizontal drawer motion in RTL. Preserve physical coordinates for measured geometry, charts, canvas, and decorative art. Keep code, OTP digits, email addresses and URLs in explicit LTR regions; isolate mixed text with `<bdi>` or `dir="auto"`. Translating labels and formatting application data is separate from layout direction. See https://www.boardui.com/rtl.

- Internal vertical scroll regions must soften the top edge with a progressive blur and surface fade once scrolled, rather than sharply clipping content. Keep headers outside the scroll region. Use a stationary, `pointer-events-none`, `aria-hidden` overlay in a relative wrapper: a 24px masked 1px backdrop blur, a 16px masked 4px backdrop blur, and a gradient from the enclosing semantic surface token to transparent. Ramp each layer's opacity from 0 to 1 over the first 24px of scrolling; keep it invisible at scrollTop 0. Apply opacity to the individual layers, never the overlay parent (a translucent parent creates a backdrop root and can break blur at normal browser zoom). Verify top/reset, overflow, light/dark mode, and 100% zoom; preserve keyboard scrolling and do not blur the whole content area. The `scroll-fade` registry item ships this recipe as `ScrollFade` + `useScrollFade` (`@/components/foundations/scroll-fade/scroll-fade`); reuse it rather than rebuilding the layers.

- Merge classes with `cx()` from `@/utils/cx` (tailwind-merge aware of BoardUI's composite text styles). No string concatenation, no plain `clsx`.
- Icons come from `@remixicon/react`, passed as component references (`leadingIcon={RiAddLine}`), not rendered elements.
- Form components build on `react-aria-components`; extend the installed BoardUI form components rather than raw `<input>`/`<select>`.
- Focus states: `outline-none focus-visible:ring-2 focus-visible:ring-border-focus-ring`.

When unsure about a token, a component's API, or working example code, ask the BoardUI MCP server: `get_theme`, `get_component`, `get_usage_examples`.
<!-- boardui:rules:end -->
