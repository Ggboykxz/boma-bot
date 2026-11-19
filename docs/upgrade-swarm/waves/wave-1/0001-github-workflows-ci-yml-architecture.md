# Wave 1 / 0001 — `.github/workflows/ci.yml`, architecture lens

Provenance: `docs/upgrade-swarm/briefs-upgrade.jsonl` id 1 (module `.github/workflows/ci.yml`,
lens `architecture`). Read `before-m5` worktree @ `53fe72b` (v0.3.0-rc5). READ-ONLY for product
and CI files; this document is the only artefact written. English per repo disk convention.
Spec and ADR authority is kept: nothing below weakens a gate; where reality and an ADR diverge,
the proposal is to bring them back into agreement, not to route around either.

## 1. What works today (keep, don't touch)

- Concurrency cancel-in-progress on `ci-${{ github.workflow }}-${{ github.ref }}`; PR and dev refs
  don't cancel each other. Postgres service with a real `pg_isready` healthcheck; `--locked`
  everywhere; fmt → clippy → nextest → llvm-cov (floor 80) → deny → machete → trace ratchet
  (`--max-missing 7`) → secret scan → D-02 no-dev-mail-route grep. The `web` job builds before
  `tsc --noEmit` for the `next-env.d.ts` reason, uploads the Playwright report only on failure.
  `compose` polls health/ready/aliases then probes the web surface (200/404 + content asserts)
  and always tears down with `down -v`. `desktop-boot-windows` boots the *exact* packed portable
  on a clean runner — evidence, not a claim (ADR-016, REQ-055). `RUSTFLAGS=-D warnings` is
  workflow-wide. Adversarial grep gates (D-02) are greps, not tests — correct call.

## 2. Fragile or redundant in the CI *design*

1. **Two toolchain sources of truth.** The `rust` job pins via `rust-toolchain.toml` (1.99.1,
   comment in-file: "CI and a developer machine use the same compiler"); `desktop` uses
   `dtolnay/rust-toolchain@stable`, floating, so the pack can be built by a different compiler
   than the code was tested with. (Adjacent, out of scope: `release.yml` installs no toolchain
   at all and would use the runner default.)
2. **The web export is built twice per push.** `web` runs `npm ci` + `build`; `desktop` then
   re-runs `npm ci` + `typecheck` + `build` in `apps/web` to stage the vendor. With a committed
   lockfile the outputs are identical, so the second build is redundant compute on the critical
   path of the desktop artifact.
3. **`desktop-boot-windows` waits on both matrix legs.** `needs: desktop` with
   `matrix: os [ubuntu-24.04, windows-latest]` means a Linux pack failure suppresses the only
   Windows first-launch evidence, though the Windows artifact is independent. Also: no
   `timeout-minutes` on `rust`, `web`, `desktop`, `chart` (only `compose` 30 and the boot job 25);
   a wedged job burns up to the 360-minute default.
4. **Supply-chain posture doesn't match the release story.** Release attests artifacts
   (Sigstore, keyless, ADR-016), yet CI runs `actions/*@vN` tag-pins rather than commit SHAs,
   installs `nextest, cargo-deny, cargo-machete, cargo-llvm-cov` unversioned via
   `taiki-e/install-action@v2`, curls the helm v3.22.1 tarball with no checksum/signature check,
   and has **no top-level `permissions:` block** (`release.yml` sets `permissions: {}`).
5. **Drift between CI constants and their homes.** `Makefile` parameterises
   `COVERAGE_MIN ?= 80` and `TRACE_MAX_MISSING ?= 7`, but ci.yml hardcodes both. The
   `chart` job templates `image.tag=0.3.0-rc` while the workspace is at `0.3.0-rc5`. And ADR-018's
   decision text lists a "contract diff against `spec/openapi.yaml`" CI job that does not exist
   in ci.yml (no `tools/` or workflow references it) — either the gate is missing or the ADR
   silently diverged; both need the owner. Note ADR-018 itself is still "proposed (awaiting
   owner go)". Minor: `SQLX_OFFLINE=true` with no `.sqlx/` committed and no `query!` macros —
   harmless, but noise that implies an offline-data workflow that isn't used.

## 3. Proposals for v0.4.0

**P1 — One toolchain source for the desktop legs.** Drop `dtolnay/rust-toolchain@stable`; use
the same `setup-rust-toolchain` action (reading `rust-toolchain.toml`) as the `rust` job, keeping
the existing `components` line. Risk: low — pinned 1.99.1 is already what clippy/tests enforce;
if desktop starts failing it was previously passing only by floating to a newer stable. No spec
or ADR impact. Order: **first** (cheapest real correctness fix).

**P2 — Build the web export once.** `web` uploads `apps/web/out` as a SHA-keyed, 1-day artifact;
`desktop` downloads it instead of rebuilding (keep the rebuild one edit away behind a fallback).
Risk: inter-job coupling and artifact staleness — mitigated by SHA-keying and short retention;
`npm ci` determinism makes outputs identical today, so this removes ~2–4 min of the desktop
critical path without changing bits. Risk of masking a genuinely broken `out/` is unchanged
(the boot leg would catch it). Order: **last** — restructures the job graph; do after P1/P4 settle.

**P3 — Least-privilege + pinned toolchain for CI itself.** Add top-level `permissions:
contents: read` (no ci.yml step writes; `upload-artifact` needs none); pin all actions to commit
SHAs; pin the four Rust tools in the `taiki-e/install-action` step to explicit versions;
checksum-verify the helm tarball (or use a pinned setup action). Risk: bump friction — needs a
scheduled dependency-bump loop, and SHA pinning without one just freezes rot; bundle the first
verification in the same PR. ADR-016/018 and the attestation story argue for this; AGENTS.md
secret rules are unaffected. Order: **second** after P1, before the job-graph split.

**P4 — Decouple the Windows boot leg and bound every job.** Split `desktop` into
`desktop-linux` / `desktop-windows` (shared YAML anchors or a composite action) so
`desktop-boot-windows` needs only the Windows leg; add `timeout-minutes` (suggest: rust 45,
web 30, desktop-linux 45, desktop-windows 90, chart 10). Ride-along: chart job templates
`image.tag` from `Cargo.toml`'s version (or a neutral `0.0.0-ci`) so the render can't rot.
Risk: medium — splitting a matrix duplicates ~30 lines; an error in `needs` wiring could mask
the boot leg entirely, so verify the boot job still triggers on a Windows-only break before
merging. Order: **third**.

**P5 — Single-source the budgets; close the ADR-018 contract-gate gap explicitly.** Source
`--fail-under-lines` and `--max-missing` from the same variables the Makefile uses (e.g. a
tiny `tools/ci.env` sourced by both, or call the Make targets). Separately, for the missing
`spec/openapi.yaml` contract diff: either add the job as warn-first (fail after one clean
green cycle), or submit an ADR-018 amendment recording the omission and its reason. Never
leave CI and the ADR silently disagreeing. Risk: the diff may surface real spec/export drift —
that is the point, but it will look like a CI break on day one; warn-first absorbs this.
Order: **fourth** (decision-heavy; needs owner input, least mechanical).

## 4. Suggested order and non-goals

Order: **P1 → P3 → P4 → P5 → P2.** Non-goals, explicitly: no sharding/rewrite of the `rust`
job, no self-hosted runners, no removal/weakening of the trace ratchet, D-02 grep, coverage
floor or secret scan, no Playwright cache that hides flakes, no Wine anywhere, no ADR edits
without the owner. Charter/spec/ADR text stays authoritative over this file. If the owner
amends ADR-018 (e.g. drops the contract diff), P5 collapses into the single-sourcing half.

Success measure for v0.4.0: the same commit compiles desktop with the pinned toolchain, CI runs
with read-only token and pinned actions, Windows-boot evidence no longer depends on the Linux
pack, and every quality constant in ci.yml has exactly one home in the repo.
