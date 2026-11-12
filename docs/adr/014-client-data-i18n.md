# ADR-014 — Client data layer, routing and i18n

Status: proposed (awaiting owner go)

## Context

The UI needs: a typed client for 283 procedures plus the HTTP endpoints, cache invalidation driven by
server events (an SSE frame must update a thread without a refetch storm), optimistic updates with
rollback (REQ-002 send, REQ-009 read state), and 15-locale-ready i18n with RTL. Spec 8.9 requires
keyboard paths, live-region announcements and no hardcoded strings.

## Alternatives

- **Data**: (a) TanStack Query with a generated client; (b) SWR; (c) hand-rolled store with reducers.
- **Routing**: (a) Next.js App Router file routes; (b) TanStack Router inside a single App Router page;
  (c) hand-rolled history routing.
- **i18n**: (a) ICU MessageFormat JSON shared by `next-intl` (UI) and a Rust ICU crate (API, e-mail);
  (b) Fluent in Rust and a separate JS loader; (c) a flat key-value map with string concatenation.

## Scoring

| Criterion | Query | SWR | hand-rolled |
|---|---|---|---|
| 1 fits the spec (optimistic, invalidation, retries) | 1 | 2 | 3 |
| 2 maturity | 1 | 1 | 1 |
| 5 simplicity | 1 | 1 | 3 |
| 6 typed contract | 1 | 1 | 2 |

| Criterion | App Router | TanStack Router | hand-rolled |
|---|---|---|---|
| 1 fits the spec (deep links, params) | 1 | 1 | 3 |
| 5 simplicity | 1 | 2 | 3 |
| 4 performance (bundle) | 2 | 1 | 1 |

| Criterion | ICU shared | Fluent + JS loader | flat map |
|---|---|---|---|
| 1 fits the spec (plurals, RTL, 15 locales) | 1 | 2 | 4 |
| 5 simplicity | 1 | 3 | 1 |
| 8 a11y/i18n | 1 | 1 | 4 |

## Decision

- **Data**: TanStack Query over a generated client. One query key per procedure + input hash; SSE
  frames write into the cache by `threadId`/`seq` (append, dedupe by seq) and invalidate the derived
  lists, so a stream never triggers a refetch loop. Mutations that the spec makes idempotent use the
  same `clientNonce` on retry.
- **Routing**: Next.js App Router file routes mirroring spec 8.1 exactly, with `generateStaticParams`
  only for the public page route; everything else is client-rendered because it needs a session.
- **i18n**: ICU MessageFormat JSON under `packages/i18n/messages/<locale>.json`, one catalogue, keys
  namespaced by screen. `next-intl` loads it in the UI; a Rust ICU crate formats the same catalogue
  for API errors and transactional e-mail. A unit test asserts every key used in the UI exists in
  `en` and that no non-test source contains a user-visible literal (lint rule).

## Consequences

- One catalogue keeps e-mail and UI wording from diverging, at the cost of an ICU crate on the Rust
  side (a small, vetted dependency).
- Server-driven strings (run errors, validation messages) arrive as keys plus parameters, not
  pre-formatted text, so the client can localise them — the reference returns formatted English
  strings in some paths, which we deliberately do not copy (recorded in `docs/PARITY.md`).
- RTL uses BoardUI's `DirectionProvider` with an explicit locale and logical properties (`ps/pe`,
  `start/end`); the screen stream keeps physical coordinates.
