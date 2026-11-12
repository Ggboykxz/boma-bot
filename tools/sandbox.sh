#!/usr/bin/env bash
# Optional: run opencode in a reproducible container with Rust, Playwright and
# the quality tools preinstalled. It is not a jail: the workspace is mounted
# read-write, spec/ is writable, and the network stays on (model API, crates.io,
# the Playwright browser, the original app if you mount or expose it).
# cargo build/test run third-party build scripts and proc-macros, so building
# in here also protects your machine. First image build is long (Rust tools
# compile). Chromium as root: if the browser refuses to start, add
# -e PLAYWRIGHT_MCP_NO_SANDBOX=1
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

docker build -t rebuild-opencode -f "$ROOT/tools/Dockerfile" "$ROOT/tools"
exec docker run --rm -it \
  --add-host=host.docker.internal:host-gateway \
  -p 127.0.0.1:3000:3000 \
  -e BOARDUI_TELEMETRY_DISABLED=1 \
  -e DO_NOT_TRACK=1 \
  -v "$ROOT":/work \
  -v rebuild-opencode-auth:/root/.local/share/opencode \
  -v rebuild-opencode-config:/root/.config/opencode \
  -v rebuild-cargo-registry:/usr/local/cargo/registry \
  -v rebuild-rustup:/usr/local/rustup \
  -w /work \
  rebuild-opencode opencode
