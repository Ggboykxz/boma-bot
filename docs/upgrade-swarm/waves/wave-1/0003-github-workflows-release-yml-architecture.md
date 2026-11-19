# 0003 — `.github/workflows/release.yml` — architecture lens (v0.4.0 proposal)

Status: **proposal only**; no code, spec, goldens or ADR edited. Target v0.4.0. Lens: architecture
(structure, boundaries, coupling, debt); worker 0004 owns reliability/ops (timeouts, caches,
draft→upload→verify→undraft mechanics) — read it as the complement, not a duplicate. Everything
below was read from `release.yml` @ `before-m5` (v0.3.0-rc5) this session. CONFIRMED = read in the
file/docs; GUESS = needs the owner's call.

## 1. Strengths (worth keeping)

- Least-privilege by construction: workflow-level `permissions: {}`, per-job grants (id-token +
  attestations only where signing happens), no secrets beyond `GITHUB_TOKEN` (CONFIRMED).
- Keyless Sigstore attestation (OIDC, no stored key) over the Linux tarballs + `SHA256SUMS.txt`
  and the Windows exe/zip + `SHA256SUMS-windows.txt`; the Windows job then runs
  `gh attestation verify` per file as a pre-upload gate — the pipeline partially dogfoods the
  stranger's verify path (CONFIRMED, lines 77–82, 209–225).
- Boot gates are real and ordered before publish: the packaged portable is booted on
  `windows-latest` (Playwright round-trip, evidence always uploaded), and the Linux released
  binary is rehearsed against the embedded stack (`desktop-boot-linux`); `publish` `needs:` all
  three legs (CONFIRMED).
- `cargo build --release --locked`, `fetch-depth: 0` (correct provenance), per-ref concurrency,
  `cancel-in-progress: false`, 14-day artifact retention (CONFIRMED).
- Honest separation of duties already sketched: `publish` holds no `id-token`, it only downloads
  and creates the release (CONFIRMED).

## 2. Weaknesses (architecture)

1. **No shared release manifest — coupling by filename convention.** `VERSION` is re-derived from
   `GITHUB_REF_NAME` in two jobs, artifact names ride on `github.sha`, and each job globs its own
   outputs (`dist/*.tar.gz`, `apps/desktop/dist/*.exe`). Nothing publishes a single
   `manifest.json` (version, commit, per-artifact sha256, attestation subjects) that downstream
   jobs must satisfy — so `publish` cannot tell "the exact bytes I verified are the bytes I
   ship" (CONFIRMED).
2. **`publish` is an unverified consumer.** It runs `ls -la dist` and `gh release create` with no
   hash re-check, no attestation verify, no asset-count assertion, no readback; boot evidence is
   uploaded `if-no-files-found: warn` and never asserted to exist (CONFIRMED, lines 118, 121–125,
   192–199, 290–299). The v0.2.1 failure (attest-then-fail-verify, assets never shipped) is the
   class this leaves open on the Linux/publish side (CONFIRMED: docs/STATE.md:358–381).
3. **Governance of the trigger is absent.** `on: push tags: v*` matches any tag (`vtest`,
   `v0`); `workflow_dispatch: {}` can publish from any ref; no check that the tag is a release
   tag, that `Cargo.toml` workspace version == tag−`v`, that `CHANGELOG.md` has a `[VERSION]`
   section, or that `ci` was green on the exact SHA. **RC tags publish as "Latest"** —
   `gh release create` never passes `--prerelease` (CONFIRMED).
4. **Release notes are code, and stale.** The heredoc still says "Release candidate 5 … owner
   smokes the Windows installer before v0.3.0 final", hardcodes "66 of 73 REQ ids", and would ship
   verbatim under a final v0.4.0 tag (CONFIRMED, lines 90–115; CHANGELOG shows `[0.3.0-rc5]`).
   The owner's manual smoke gate exists in prose only — nothing in the pipeline enforces it.
5. **Structure drift between plan, ADR and reality.** ADR-018 chose `cargo-dist` + signed
   archives + multi-arch image; `docs/PLAN.md:55` still lists it; reality is hand-rolled
   x86_64-only tarballs, unsigned by owner decision (ADR-016), no image. PLAN's own release-job
   budgets (cold start ≤ 2 s, RSS ≤ 150 MB, binary ≤ 40 MB stripped, `docs/PLAN.md:125–127`) are
   not enforced anywhere in this file. The web client and Rust binary are also built twice per
   run (build-linux and desktop-windows) — same commit, two independent rebuilds, no equivalence
   check (CONFIRMED).

## 3. v0.4.0 proposals (in order)

1. **Preflight job (architecture gate, additive).** New job ahead of all builds: tag matches
   `^v[0-9]+\.[0-9]+\.[0-9]+(-rc[0-9]+)?$`; workspace `version` == tag−`v` (Cargo.toml + root/web/
   desktop package.json); `CHANGELOG.md` contains `## [${VERSION}]`; `ci` workflow conclusion on
   `GITHUB_SHA` is `success` (GitHub API, `GITHUB_TOKEN`); `-rc*` ⇒ pass `--prerelease` at
   publish; `workflow_dispatch` restricted (require a tag or dispatch ref input, refuse branch
   refs). Risks: false blocks on a legitimate hotfix ci flake (mitigate: re-run, never weaken
   the check); API flakiness. Effort: small. Do this **first** — it deletes the v0.2.0/v0.2.1
   failure class at the front door.
2. **Release manifest as the single artifact contract.** Build jobs emit `dist/manifest.json`
   (version, GITHUB_SHA, artifact list + sha256 + attestation subject); a verify job (or
   `publish` head) re-hashes every downloaded asset against the manifest and runs
   `gh attestation verify` on each before `gh release create` (0004's draft/upload/verify flow is
   the mechanical host). Risks: schema bikeshedding; keep it minimal (jq-able, no tooling).
   Effort: medium. **Second** — it makes (1) enforceable end-to-end and removes the
   glob/convention coupling.
3. **Environment-gated publish with separation of duties.** `publish` runs under a GitHub
   `release` environment with the owner as required reviewer — this encodes the
   owner-smoke-before-final that currently lives only in the notes prose (and gives prereleases
   a lighter path if the owner wants one). Editing the gate then requires an ADR per AGENTS.md
   (gates may change, never silently); keep `concurrency` and no `id-token` for the publisher.
   Risks: adds latency; misconfigured required reviewers can block an urgent fix (document the
   override in UPGRADING.md). Effort: config + small yaml, but needs the owner. **Third.**
4. **Release notes as data, not code.** Replace the inline heredoc with notes composed from
   `CHANGELOG.md`'s `[VERSION]` section (shell/python one-liner), plus a static, version-agnostic
   "Verify these downloads" block (the gh attestation commands, updated per release); preflight
   fails if the body matches `release candidate` on a non-`-rc` tag (complements 0004's regex
   with a single source of truth — REQ counts drop out of the text entirely). Risks: Keep a
   Changelog section format must stay parseable (pin the extractor; fail closed). Effort:
   small–medium. **Fourth.**
5. **Provenance completeness: SBOM + action pinning.** Generate a CycloneDX SBOM
   (`cargo sbom` / `cdxgen`) for the server bundle and desktop pack, attest it alongside existing
   subjects (`actions/attest-sbom`, same OIDC pattern), and pin every third-party action in the
   file to a full commit SHA with Dependabot on `.github/workflows/`. Risks: SBOM tooling
   maintenance; SHA pinning raises update toil (Dependabot covers both). Also reconcile
   ADR-018↔reality in the same PR (amend ADR-018 or record the deviation in
   `docs/PARITY.md`/STATE — owner decision; spec/ADRs stay authoritative, this only *records*).
   Effort: medium. **Last** — it hardens the chain once (1)–(4) carry truth.

## 4. Non-goals / authority

No code signing (ADR-016 owner decision stands; attestations remain the chain of custody), no
auto-update (UPGRADING.md), no new artifacts (macOS/ARM/images stay out per M35), no spec or
golden changes, no weakening of the boot gates or `cargo test` claims in the notes (the notes
must become evidence-true, e.g. cite the ci run id on the tagged SHA). The pipeline stays the
single release path.

## 5. Evidence

CONFIRMED — `.github/workflows/release.yml` (all lines cited); `docs/STATE.md:358–381, 691`
(v0.2.1 failure, attested flow); `docs/PLAN.md:55,125–127` (cargo-dist plan, unenforced budgets);
`docs/adr/018-deployment-ci.md` (proposed), `docs/adr/016-desktop-mobile.md` (signing);
`docs/UPGRADING.md:130–160` (verify flow, unsigned-by-decision); `CHANGELOG.md [0.3.0-rc5]`;
`.github/workflows/ci.yml` (gate set release must consume). GUESS — owner tolerance for a manual
approval on every release tag, `rc` prerelease visibility, SBOM format preference.
