# 0004 — `.github/workflows/release.yml` — reliability lens (v0.4.0 proposal)

Status: proposal, no code changed. Target: v0.4.0, one PR, tag-agnostic. Lens is reliability only;
architecture is worker 0003. Everything below was read from `release.yml` @ before-m5 (v0.3.0-rc5).

## Observed / recurring failure modes

1. **Maven Central 429/404 (seen twice).** Embedded Postgres zonkyio jars come from
   `repo1.maven.org` / `repo.maven.apache.org` via `tools/fetch-embedded-postgres.sh`. The script
   already retries 8× at fixed 6 s and falls back to a second mirror, and the SHA-256 gate is the
   real guard — but the same jar is fetched up to **three times per run** (build-linux stage,
   desktop-windows stage, desktop-boot-linux rehearsal). Fixed-delay retry does not honor
   `Retry-After`, and shared `windows-latest`/`ubuntu-24.04` runner IPs get CDN rate-limited.
2. **Windows boot heavy.** `desktop-windows` chains: `vcpkg install openssl:x64-windows-static-md`
   (no pinned baseline, rebuilt every run) → full `cargo build --release` → `npm ci` ×2 →
   `npx playwright install chromium` (~150 MB) → NSIS build → portable unpack → boot smoke. No
   caches, no job-level timeouts anywhere in the file (default 360 min/job); one hung boot step
   burns hours before anyone notices.
3. **Publish has no rollback / is not idempotent.** `publish` runs
   `gh release create "$GITHUB_REF_NAME" ... release-assets/*` in one shot. If the upload leg
   fails after the release object is created, a re-run dies on "release already exists" and the tag
   can end up pointing at an empty or partial public release. v0.2.1 exists precisely because a
   published release shipped broken (`815b103` publish-job fix). No post-publish verification.
4. **v0.2.0-style partial publish.** rc/final notes are hardcoded ("Release candidate 5 …"), so a
   final tag can ship notes describing an rc. No gate catches it.
5. **Soft gates.** "Ensure there is something to publish" is `ls -la dist`; nothing checks asset
   count, checksum coverage, or attestation presence before upload.

## Concrete hardening (exact fields, retry/backoff)

**Job-level timeouts + shell hardening**
- `timeout-minutes:` build-linux 45, desktop-windows 75, desktop-boot-linux 30, publish 10.
- All bash steps keep `set -euo pipefail`; add `shell: bash` where implicit.

**Maven Central fetch (build-linux, desktop-windows, desktop-boot-linux)**
- Wrap `actions/cache@v4`: `path: apps/desktop/vendor` won't work (built), so cache the jar only —
  `path: .cache/embedded-postgres`, `key: embedded-postgres-${{ hashFiles('tools/fetch-embedded-postgres.sh') }}-17.12.0-<os>` with SHA-256 verify on cache hit (fail closed to re-fetch on mismatch). Saves 2 of 3 downloads.
- Replace fixed-delay curl retry with a loop: attempts 1..6, sleep `min(2^n + $RANDOM % 5, 60)` s;
  on HTTP 429 parse `Retry-After` (cap 120 s); `--retry-all-errors` semantics preserved. Keep the
  SHA-256 pin unchanged — mirrors can never supply different bytes.
- Add a third mirror (`https://maven-central.storage-download.googleapis.com/maven2`, same SHA gate).

**npm / cargo / playwright / vcpkg**
- build-linux + desktop-boot-linux: `actions/setup-node@v4` gains
  `cache: npm` + `cache-dependency-path` (web/desktop lockfiles); `npm config set fetch-retries 5 fetch-retry-factor 2 fetch-retry-mintimeout 10000 fetch-retry-maxtimeout 60000` before `npm ci`.
- All cargo legs: `CARGO_NET_RETRY=10`, `CARGO_NET_GIT_FETCH_WITH_CLI=true`; add `Swatinem/rust-cache@v2` (key on `Cargo.lock`) for registry + `target/` on Linux and Windows.
- Windows: cache `C:\vcpkg\installed` keyed on `hashFiles('apps/desktop/**/vcpkg.json')` (and pin `builtin-baseline` in that manifest) — or replace vcpkg with the same pinned-SHA256 jar approach used for Postgres; either removes the unbounded vcpkg rebuild.
- `npx playwright install chromium` → cache `~\AppData\Local\ms-playwright` keyed on the installed `@playwright/test` version in `apps/web/package-lock.json`.

**Checksum / attestation gates (before any upload)**
- build-linux: after writing `SHA256SUMS.txt`, run `sha256sum -c SHA256SUMS.txt` and assert
  `≥ 3` tarballs + matching sum file via a 5-line shell test (fail loudly otherwise).
- desktop-windows: assert exactly one `*-setup.exe` and one `*-portable.zip` (current
  `Select-Object -First 1` silently picks if duplicates appear).
- publish: after undraft (below), `gh release view --json assets` → assert ≥ 6 assets incl. both
  SHA256SUMS files; then `gh attestation verify` **against the uploaded release assets** (download
  to `mktemp -d`) — verification of the exact bytes a stranger receives.

**Publish: draft → upload → verify → undraft (the rollback fix)**
```yaml
- name: Create a draft release          # gh release create "$GITHUB_REF_NAME" --draft --verify-tag \
                                        #   --title ... --notes-file ... (no assets yet)
- name: Upload assets (idempotent)      # gh release upload "$GITHUB_REF_NAME" --clobber release-assets/*
- name: Verify asset count + notes      # shell asserts; fails if notes match /release candidate/i on a non-rc tag
- name: Undraft                         # gh release edit "$GITHUB_REF_NAME" --draft=false
- name: Roll back on failure            # if: failure() — delete ONLY the draft (gh release delete --yes), tag untouched
```
Re-runs become safe (`--clobber`), a failed publish never goes public, and rollback is explicit
instead of "hand-delete the release and hope". `--verify-tag` replaces nothing but fails fast on a
mistyped dispatch ref.

**Heredoc notes gate**: fail publish if `release-notes.md` contains "Release candidate" while
`GITHUB_REF_NAME` does not match `*rc*` — cheap guard against the v0.2.0 class of mistake.

## Risks / notes

- Draft-first publish changes semantics: an interrupted run leaves a **draft**, visible only to
  collaborators; the `if: failure()` step deletes drafts only and never tags/assets of a live release. Owner should be told drafts may linger if the runner dies hard.
- Jar cache is safe only because the SHA-256 pin is re-checked on restore; if the pin changes, bump the key (hashFiles of the fetch script covers this).
- More retries must respect `Retry-After` and cap at 6 attempts/120 s or we hammer Maven's CDN and lengthen 429 storms.
- Shorter timeouts risk false failures on slow Windows runners; 75 min is generous vs. observed rc runs (~whole-release budget) but must be revisited with real timings from v0.4.0.
- rust-cache/vcpkg caches can serve stale toolchains on runner image upgrades — key includes lockfile/manifest hashes; worst case is a cold rebuild, not a wrong artifact (all artifacts remain SHA/attestation gated).
- No REQ/spec impact; release notes and flow change only. Keep `concurrency: release-${{ github.ref }}, cancel-in-progress: false` as-is.
