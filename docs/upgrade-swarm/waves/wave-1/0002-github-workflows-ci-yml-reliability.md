# 0002 — .github/workflows/ci.yml — reliability (v0.4.0)

Worker #002 · module `.github/workflows/ci.yml` · lens: reliability · READ-ONLY review
of `rebuild-workspace/.worktrees/before-m5` at v0.3.0-era source. Proposals only; no code
in this change. Rerun counts are this month's CI numbers from the swarm brief: **Maven
404/429, apt 100, compose 56** — hundreds of human reruns, almost all transient infra, not
code defects.

## What the file already does right (keep, don't re-propose)

- `concurrency: cancel-in-progress: true` on `ci-${{ github.workflow }}-${{ github.ref }}`;
  red intermediate runs carry no information.
- Postgres service with `--health-*` gating; compose job probes with `curl --retry 30 --retry-connrefused`.
- `tools/fetch-embedded-postgres.sh` already retries curl `--retry 8 --retry-all-errors
  --retry-delay 6` against two canonical Maven hosts, with a SHA-256 pin as the integrity
  gate (a different host can never supply different bytes).
- `desktop` matrix has `fail-fast: false`; `desktop-boot-windows` is separate, `timeout-minutes: 25`; artifacts upload `if: failure()` / `if: always()`.
- `cargo ... --locked` everywhere; `docker compose down -v` under `if: always()`.

## Failure modes seen this month → concrete hardening proposals

### F1. Maven Central 404/429 on the embedded-Postgres fetch (desktop, both OS legs)

`stage-desktop-vendor.sh` → `fetch-embedded-postgres.sh` fetches the zonkyio jar from
`repo1.maven.org` / `repo.maven.apache.org`. Despite the existing retry loop, shared
runner IPs are rate-limited (429); `repo1` intermittently 404s cold artifacts.

P1. Add a third mirror to `urls` in `fetch-embedded-postgres.sh` (e.g. a SHA-256-verified proxy or internal artifact store); pin unchanged — integrity never widens, only availability.
P2. Upgrade the fixed `--retry-delay 6` to exponential backoff with jitter
    (`--retry-delay 2 --retry-max-time 120` plus explicit loop with `sleep $((2**n + RANDOM%3))`),
    and fail the *step* (not the job) so `desktop` can distinguish fetch vs. build.
P3. Cache the extracted vendor (`actions/cache` keyed on `PG_VERSION` + OS + SHA-256) so a
    Maven outage does not gate every push; fall back to network fetch on cache miss.
P4. Emit a `::warning::` annotation when a mirror fails but a fallback succeeds, so
    degraded redundancy is visible on the job summary.

### F2. apt-get failure 100 (rust job: `pkg-config libssl-dev`; also `docker info` and the Dockerfile's apt steps)

`sudo apt-get update && sudo apt-get install --yes ...` has zero retry; exit 100 is the
classic transient mirror/hash-sum mismatch on ubuntu-24.04 runners.

P5. Wrap apt in a bounded retry: `for i in 1 2 3; do sudo apt-get update && sudo apt-get install ... && break || sleep $((5*i)); done`,
    or `Acquire::Retries=5` via `-o`. Apply identically in `Dockerfile` and `tools/Dockerfile`
    (same failure class, same fix).
P6. Pin/verify nothing extra for apt beyond `--no-install-recommends`; do NOT swap mirrors
    blindly — runner mirror selection is usually the culprit, and retries absorb it.
P7. Keep the declared-dependency comment (image may drop libssl-dev) — this proposal
    hardens the fetch, not the requirement.

### F3. compose 56 (compose job: `docker compose up -d --build`)

Base images `node:22-bookworm-slim`, `rust:1.99.1-bookworm`, `debian:bookworm-slim` are
pulled from Docker Hub on every run; Hub rate-limits anonymous runner IPs (429), and
transient pull/registry and DNS failures read as infra flakes. 56 reruns this month.

P8. `docker compose pull` with explicit retry/backoff (e.g. 4 attempts, 15s→120s backoff)
    before `up --build`, so pulls fail as *pulls*, early and diagnosable, not as a build timeout.
P9. Pre-pull the three base images in a dedicated step with `docker pull` retries, then
    `docker compose up -d --build --pull never`... or keep `--build` but rely on warm local
    images; layer caching via `docker/build-push-action` or `docker buildx` with GHA cache
    (`type=gha`) removes most Hub hits entirely.
P10. Add `timeout-minutes: 30` parity to every job that lacks it (`rust`, `web`, `desktop`,
    `chart`, `compose` has one; `desktop-boot-windows` has one) so a hung pull/build fails
    fast instead of consuming the 6h default.
P11. On probe failure the job prints `docker compose logs app` — good; extend to
    `docker compose ps` and `docker inspect` health status; still no retry of the *probe*
    semantics change.

### F4. Cross-cutting transients (npm, actions, tool installs, helm)

P12. `npm ci` (web, desktop, desktop-boot) and `npx playwright install --with-deps chromium`:
    wrap in bounded retry (npm: `npm config set fetch-retries 5 fetch-retry-maxtimeout 120000`
    or a retry loop; playwright: 3 attempts).
P13. `taiki-e/install-action@v2` (nextest, cargo-deny, ...): add fallback — install from
    crates.io/GitHub releases via `cargo binstall` or pin prebuilt tarballs with SHA check —
    mirrors the Maven fix pattern.
P14. `chart` job helm download (`curl -fsSL https://get.helm.sh/helm-v3.22.1-...`): no retry;
    add `--retry 5 --retry-delay 3 --retry-all-errors`, or install helm via
    `azure/setup-helm` / `helm/kind-action`-style maintained action with built-in retry.
    vcpkg `openssl:x64-windows-static-md` on the Windows leg shares the same shape — wrap or
    cache `VCPKG_ROOT`.
P15. `actions/upload-artifact` / `download-artifact` v4 already handle most transients
    internally; leave alone except ensuring `retention-days` stays bounded (done).

## Explicit non-goals / rejected options

- Do not widen SHA-256 pinning or `--locked` discipline to reduce "failures" — those gates
  exist to keep bytes honest; reliability comes from availability paths, not weaker checks.
- Do not auto-retry *test* steps to green CI; reruns mask real flakes. Only network/fetch
  and infra steps get retries, and every retry must be bounded and logged.
- Do not switch registries (GHCR mirror of everything) in v0.4.0 scope — evaluate as a
  follow-up ADR; it touches provenance and docs/DEPENDENCIES.md.

## Risks and trade-offs

- R1. Retry storms on a genuinely down upstream (Maven/Hub): capped by bounded attempts +
  job timeout (P10); worst case fails slower, never silently greens.
- R2. Cache poisoning (P3): keyed on immutable inputs (version + hash), and extraction is
  re-verified; cache miss degrades to today's behavior.
- R3. `--pull never` (P9) can pin stale base images; mitigate by scheduling a weekly
  `pull-and-rebuild` or refreshing the GHA cache TTL, and document in the workflow comment.
- R4. Extra steps lengthen the YAML and add maintenance surface; each proposal is
  independently revertable and none changes gate semantics.
- R5. `apt` retries hide persistent mirror rot; add a weekly scheduled `apt-get update`
  canary job (or rely on Dependabot/maintenance window) to notice chronic failures.
- R6. New third-party action (P14 alternative) widens the supply-chain surface; prefer
  shell curl+retry with pinned version and SHA over new actions where feasible.

## Suggested rollout for v0.4.0

Wave 1 (this doc, no code): land P1–P4 (Maven), P5 (apt + Dockerfiles), P8/P9/P10 (compose).
Wave 2: P7/P12/P14, P15 verification, P11 log enrichment. Track rerun rate for 30 days;
target ≥70% reduction in infra-class reruns before closing the milestone. Gate semantics,
lockfile, and pin verification unchanged.

_Scope note: line references are against `.worktrees/before-m5/.github/workflows/ci.yml`.
Counts (Maven 404/429, apt 100, compose 56) are as reported this month; no CI logs were
re-derived in this read-only pass._
