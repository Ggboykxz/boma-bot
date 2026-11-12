# Dependencies

Every third-party artefact, why it is here, and its licence. Versions were resolved from the live
registry on **2026-11-12** (`crates.io` API `max_stable_version`, `registry.npmjs.org` `/latest`),
never typed from memory. Refresh this table and `docs/STATE.md` together when you bump something.

**CONFIRMED**: resolved from the registries above, and the workspace compiles, lints, tests and
passes `cargo deny check` with this exact set (commands and results in `docs/STATE.md`).

## Project licence

| Subject | Value |
| --- | --- |
| Boma Bot | **MIT** (`LICENSE`) — chosen by the agent on 2026-11-12 to unblock `cargo deny`; the owner may change it in one edit (`Cargo.toml` + `LICENSE`) and re-run `cargo deny check` |
| Constraint on dependencies | every licence below is compatible with an open-source release |
| Enforcement | `cargo deny check` with the allow-list in `deny.toml` |

`cargo deny` reads an SPDX expression, not a prose file: a workspace without `license = "…"` is
reported as `unlicensed` and the gate fails. That is why the licence is an expression in
`[workspace.package]` rather than only a file.

## Rust crates

`cargo-deny` checks these; the "why" column is the reason it earns a place (AGENTS.md: maturity,
maintenance, licence, simplicity).

| Crate | Version | Licence | Why |
| --- | --- | --- | --- |
| `axum` | 0.8.9 | MIT | HTTP layer, same ecosystem as `tower` (ADR-003) |
| `tokio` | 1.54.1 | MIT | the async runtime everything else assumes |
| `tower` | 0.5.3 | MIT | `ServiceExt::oneshot` in tests, middleware contracts |
| `tower-http` | 0.7.1 | MIT | trace, request id, body limit, security headers |
| `sqlx` | 0.9.0 | MIT OR Apache-2.0 | checked queries, migrations, pool (ADR-001). MSRV 1.94 sets ours |
| `serde` | 1.0.229 | MIT OR Apache-2.0 | DTOs, fixtures, manifest |
| `serde_json` | 1.0.154 | MIT OR Apache-2.0 | JSON bodies and golden comparison |
| `serde-saphyr` | 2.1.1 | MIT OR Apache-2.0 | parse `spec/openapi.yaml` in the contract test. Replaces `serde_yaml_ng`, whose `unsafe-libyaml` base is archived (RUSTSEC-2024-0449) |
| `thiserror` | 2.0.21 | MIT OR Apache-2.0 | typed errors in libraries |
| `anyhow` | 1.0.104 | MIT OR Apache-2.0 | binary edge only, per AGENTS.md |
| `tracing` | 0.1.44 | MIT | structured logs with spans |
| `tracing-subscriber` | 0.3.23 | MIT | JSON and pretty renderers, `EnvFilter` |
| `utoipa` | 6.0.0 | MIT OR Apache-2.0 | generate the OpenAPI document from handlers (ADR-003) |
| `clap` | 4.6.7 | MIT OR Apache-2.0 | one binary, five subcommands (ADR-017) |
| `uuid` | 1.29.2 | Apache-2.0 OR MIT | request ids now, entity ids from M1 |
| `async-trait` | 0.1.92 | MIT OR Apache-2.0 | object-safe async ports (`ReadinessCheck`, providers) |

Deliberately **not** added yet, so no unused dependency rots in the tree: `argon2`, `secrecy`,
`aes-gcm`, `proptest`, `wiremock`, `testcontainers`, `criterion`, `reqwest`. Each arrives with the
milestone that needs it (M1 for argon2/`secrecy`/proptest, M2 for `wiremock`, M4 for HTTP providers).

**YAML parser (deliberate).** The contract test parses the spec's OpenAPI document, so one YAML
parser is needed. `serde_yaml_ng` was replaced by **`serde-saphyr` 2.1.1** (published 2026-11-05,
MSRV 1.89): the old one depends on `unsafe-libyaml`, whose repository was archived in 2024 and
carries RUSTSEC-2024-0449, and `cargo deny` refuses it. `serde-saphyr` is a typed, panic-free
parser with no unsafe code of its own; the crate is used with `default-features = false` and only
the `deserialize` feature, so the optional validation and diagnostic stacks stay out of the graph.

**One extra allowed licence.** `webpki-roots` (pulled in by `sqlx`'s `tls-rustls-ring` feature)
ships the Mozilla root store under `CDLA-Permissive-2.0`. It is permissive and compatible with the
project's own licence, so `deny.toml` allows it explicitly rather than disabling the check.

## Node packages (`apps/web`)

| Package | Version | Licence | Why |
| --- | --- | --- | --- |
| `next` | 16.4.6 | MIT | App Router with `output: "export"` (ADR-013) |
| `react` / `react-dom` | 19.3.1 | MIT | required by Next 16 |
| `typescript` | **6.0.3** | Apache-2.0 | see the pin note below |
| `tailwindcss` / `@tailwindcss/postcss` | 4.3.5 | MIT | BoardUI targets Tailwind v4 |
| `eslint` | **9.39.5** | MIT | flat config; see the ESLint pin note below |
| `eslint-config-next` | 16.4.6 | MIT | Next's rules, exported as a flat config array |
| `@playwright/test` | 1.65.0 | Apache-2.0 | end-to-end and UI golden replay |
| `@axe-core/playwright` | 4.14.0 | MPL-2.0 | accessibility gate, zero critical/serious |
| `@remixicon/react` | 4.9.0 — **not installed at M0** | Remix Icon License 1.0 | BoardUI's icon set; added by `/ui-kit` with the first icon, not before |
| `@types/node` | **22.20.5** | MIT | matches the Node 22 runtime CI uses |
| `@types/react` / `@types/react-dom` | 19.3.0 | MIT | React types |

**TypeScript pin (deliberate).** `typescript@latest` is **7.0.2**, the native compiler port
(~2.5 MB unpacked, versus ~24 MB for the JavaScript implementation). Next's type plugin drives the
compiler through the JavaScript API, which the native port does not expose yet, so this project pins
the latest JavaScript release instead: **6.0.3** (`registry.npmjs.org/typescript`, major-line
`6.x`). Revisit when Next declares support for 7.x; record the change here.

**ESLint pin (deliberate).** `eslint@latest` is **10.14.0**, but `eslint-config-next@16.4.6` pulls
`eslint-plugin-react@7.37.5`, whose peer range stops at ESLint 9.7. npm 10 resolves peers strictly, so
ESLint 10 makes `npm install` fail with `ERESOLVE`. This project pins the latest 9.x, **9.39.5**;
`eslint-config-next` itself declares `eslint >=9.0.0`. Revisit when `eslint-plugin-react` declares
ESLint 10 support.

**Node types.** `@types/node` is pinned to **22.20.5**, the newest 22.x line, because CI runs Node 22
and `@types/node@latest` (26.6.6) describes a runtime that is not deployed.

## Container images

| Image | Tag | Why |
| --- | --- | --- |
| `rust` | `1.99.1-bookworm` | matches `rust-toolchain.toml` (`channel = "1.99.1"`) |
| `debian` | `bookworm-slim` | runtime for the static-ish binary; CA roots only |
| `postgres` | `17.11-alpine` | current major, small image, `statement_timeout` set in `compose.yaml` |

## Design system

BoardUI (free tier) supplies components, semantic tokens and the design rules. The Pro licence is
**not** activated on this machine and no Pro source is in the tree. Before publishing, re-read
BoardUI's terms: if Pro source may not be redistributed, it must be installed at build time and kept
out of the public repository. `apps/web/app/globals.css` currently holds a placeholder token layer
that `init_boardui` replaces during `/ui-kit`; that replacement is recorded in `docs/UI.md`.

## Assets

Nothing visual was taken from the analysed product. No font, icon or image is vendored at M0.
