# Worker 108 — `apps/web/lib/bot-directory.ts` — reliability (v0.4.0 proposal)

## Scope read (CONFIRMED)

`bot-directory.ts` (67 lines) is an in-memory external store: `bots` / `listeners` (19-20),
`registerBot` add-or-rename by id (23-31), `listBots` (34-36), `subscribeBots` (39-44),
`serverBots` returning the constant `EMPTY` (17, 47-49), and `botIdFromPath` parsing
`/app/<id>/` with `decodeURIComponent` (60-67). Consumers: `bot-rail.tsx` subscribes and
falls back to this directory only when the roster is `null` (lines 86, 161-193);
`thread-screen.tsx` is the sole writer, registering after a successful `botsGet` (line 111);
`app-entry.tsx` uses `botIdFromPath` with a no-op subscribe and `null` server snapshot (line 26).
`roster.ts` never resets to `null` after the first answer (36-45). No tests reference this
module (grep over `*.test.*` / `*.spec.*`: none).

## Failure modes

1. Stale data (CONFIRMED by code):
   - The store is append/rename-only: no eviction, no size bound. A deleted or archived bot
     stays in the directory for the whole session and reappears in the rail fallback whenever
     the roster is `null` or a refresh fails (`roster.ts:43` keeps the last answer but does not
     clear it). Long sessions accumulate unbounded entries (GUESS: minor memory growth).
   - Last-write-wins on rename with no ordering token: a late `botsGet` for an older bot name
     overwrites a fresher one (e.g. quick successive navigations, aborted-but-resolved fetch).
   - `registerBot` with an empty/duplicate-shaped id (`id === ""`) creates a `/app//` link;
     the rail uses `bot.id` directly as a React key and href (`bot-rail.tsx:173-183`).

2. Missing entries (CONFIRMED by code):
   - Registration is coupled to a successful fetch. If `botsGet` fails, the thread screen is on
     a bot but never registers it; with the roster also pending/failed the rail renders the
     "empty" message — a visible gap exactly when the user needs the directory most.
   - The store is module memory only: a reload of a deep link loses the directory. Until
     `botsGet` resolves there is no fallback entry (GUESS: brief empty rail on slow networks).
   - Only the single-bot thread path registers. Group paths (`/app/g/<id>/`, `/app/artifacts/…`)
     and the ACP-detected row never write here, so the fallback cannot represent them.
   - `decodeURIComponent(match[1])` (line 66) is unguarded: a malformed escape in the URL
     (e.g. `/app/%/`) throws `URIError` inside a `useSyncExternalStore` snapshot
     (`app-entry.tsx:26`) and can break rendering of the whole shell. This is the highest-severity
     reliability defect found; the path is untrusted input.

3. SSR/CSR divergence (CONFIRMED by code):
   - Invariant is currently correct: `serverBots()` is a stable `EMPTY` and writes happen only in
     a client effect (`thread-screen.tsx:111`), so first client render matches the server.
   - Divergence risk is by convention, not enforced: any future synchronous `registerBot`
     during module init/render would make `listBots()` non-empty before hydration and produce a
     mismatch. Nothing in the module documents or guards this.
   - `registerBot` allocates a new array even when id and name are unchanged (line 26), notifying
     all subscribers and forcing re-renders with identical data (GUESS: churn under repeated
     navigation). Listener iteration (28-30) has no per-listener isolation: one throwing
     subscriber blocks later ones, leaving them stale.

## Concrete hardening (ranked; v0.4.0)

- H1 (P0) Guard `botIdFromPath`: `try { decodeURIComponent(...) } catch { return null }`; also
  reject empty/decoded-slash ids and cap length. Add a property/fuzz test over arbitrary path
  strings asserting "never throws". Risk: an id that currently decodes with an encoded slash
  would be rejected — check spec id format first; keep the accepted set at least as broad as
  today's normal ids.
- H2 (P0) Make `registerBot` idempotent: early-return when the id exists with the same name;
  validate `id`/`name` are non-empty strings. Risk: consumers that relied on a "ping" notification
  would miss it — none do today (CONFIRMED), so low risk; document the no-op contract.
- H3 (P1) Order-aware rename: add an optional monotonic sequence to `BotSummary` (e.g.
  `seq?: number`) supplied by the writer; ignore updates older than the stored one. Risk: changes
  a shared interface (optional field keeps compatibility); a wrong clock source can freeze names
  — prefer a caller-side counter, not `Date.now()`.
- H4 (P1) Bound and prune the directory: cap at ~200 entries (LRU) and pin the id currently in
  the path; optionally clear entries absent from a freshly loaded roster so deleted bots stop
  showing in the fallback. Risk: eviction can drop a bot mid-view (mitigate with pinning); pruning
  on roster load is a visible-behavior change — treat as DEVIATION and record in `docs/PARITY.md`.
- H5 (P1) Isolate listeners: `for (const l of listeners) { try { l() } catch { /* log once */ } }`.
  Risk: hides subscriber bugs; mitigate with a dev-only `console.error`, no user-visible text.
- H6 (P2) Deep-link coverage: register a placeholder `{ id, name: id }` from `botIdFromPath` when
  entering a thread, replacing it with the server name on success, so the fallback is never empty
  on a bot route. Risk: placeholder flashes and can persist after a failed `botsGet` (misleading);
  use a `pending` flag or expiry, and never seed names from untrusted URL text.

## SSR/CSR guardrails (no behavior change)

- Keep `serverBots()` returning the same `EMPTY` reference (CONFIRMED stable). Add a code comment
  and a dev assertion that `registerBot` is not called during render/SSR; add a hydration test
  asserting server snapshot `[]` and first client snapshot `[]`.
- Tests to add (none exist today): unit tests for register/rename/no-op/dedup, H1 fuzz case, a
  listener-throws case, and a Playwright deep-link test asserting no hydration warning and a
  non-empty rail after a delayed `botsGet`.

## Risks / deviations summary

H1/H2/H5 are internal and low risk. H3/H4/H6 touch visible behavior (ordering, eviction, placeholder
names); they need a DEVIATION entry in `docs/PARITY.md` and the maintainer's approval before
shipping. No BoardUI token or copy changes are needed in this module.
