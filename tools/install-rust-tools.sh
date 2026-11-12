#!/usr/bin/env bash
# Installs the Rust tooling used by the quality gates. Run it on the host, or use sandbox.sh.
# Prefer ./tools/sandbox.sh, which has everything preinstalled.
set -euo pipefail

command -v rustup >/dev/null || { echo "Install rustup first: https://rustup.rs" >&2; exit 1; }
rustup component add clippy rustfmt llvm-tools-preview
cargo install --locked cargo-deny cargo-nextest cargo-llvm-cov cargo-machete
echo "Optional: cargo install --locked cargo-mutants sqlx-cli"
echo "Optional secret scanner: gitleaks (see its release page); /audit falls back to grep."
echo "Playwright: after npm install in the UI package, run: npx playwright install --with-deps chromium"
