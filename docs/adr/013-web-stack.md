# ADR-013 — Web UI: React 19 + Tailwind v4 on Next.js with static export, served by the Rust binary

Status: proposed (awaiting owner go)

## Context

`spec/08-ui-ux.md` describes a SPA-like product (chat shell, panels, drawers) that talks only to the
RPC and HTTP interfaces. AGENTS.md mandates BoardUI for components and tokens; BoardUI's own
`get_started` states it is "a React + Tailwind CSS v4 design system (Next.js first)". The deployment
target is a self-hosted box where the reference served a static build behind the API. Criterion (5)
rewards "as few languages and services as the job allows", and criterion (4) rewards performance on
low-end devices and poor networks.

## Alternatives

1. **Next.js (App Router) with `output: "export"`**: a static artifact built in CI, served by the Rust
   binary; BoardUI's first-class target.
2. Vite + React 19 SPA: static artifact, no framework runtime, but BoardUI is only informally tested
   outside Next.js.
3. Remix/React Router framework mode with a Node server at runtime.
4. SvelteKit or Solid: smaller bundles, but no BoardUI support (it is React-only), which contradicts
   the mandate.

## Scoring

| Criterion | 1 Next export | 2 Vite SPA | 3 Remix server | 4 SvelteKit |
|---|---|---|---|---|
| 1 fits the spec | 1 | 1 | 2 (second runtime) | 3 (no BoardUI) |
| 2 maturity | 1 | 1 | 1 | 1 |
| 3 security | 1 (static, no server code) | 1 | 3 (server surface) | 2 |
| 4 performance | 2 (framework payload) | 1 | 2 | 1 |
| 5 simplicity | 2 (Node only in CI) | 1 | 4 (Node in prod) | 4 (new ecosystem) |
| 6 typed contract | 1 | 1 | 1 | 2 |
| 7 licence | 1 | 1 | 1 | 1 |
| 8 a11y/i18n | 1 | 1 | 1 | 2 |

## Decision

Option 1. `apps/web` is a Next.js App Router project with `output: "export"` and no server features:
no route handlers, no server actions, no ISR. The build emits `out/`, which the Rust binary serves
from disk with immutable hashed assets, `index.html` fallback per route, and long-lived caching for
`/_next/static/*`. CI runs Node; production does not.

## Consequences

- BoardUI components install and refresh with the supported toolchain (init_boardui, install_components),
  and the always-on design rules apply unchanged.
- Any BoardUI component that imports a Next-only module is adapted locally (recorded in `docs/UI.md`)
  or replaced by an equivalent composed from the same tokens. This is the known cost of following
  BoardUI rather than fighting it.
- The app has no runtime configuration: it discovers capabilities from `/api/auth/capabilities` and
  `/ready`, so one build serves every deployment.
- Deep links must work without a Node server; the Rust static handler maps unknown paths under the
  app to `index.html` and never to a directory listing.
