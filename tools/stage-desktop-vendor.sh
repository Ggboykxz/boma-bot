#!/usr/bin/env bash
# Stage the self-contained desktop ingredients into apps/desktop/vendor (gitignored): the
# pinned embedded Postgres, the boma server binary (migrations embedded), and the web export.
# electron-builder packs that directory into resources/. Package-time only; never committed.
#
#   tools/stage-desktop-vendor.sh [linux|windows|auto]
#
# Local rehearsals can point the vendor root elsewhere with VENDOR_ROOT=/tmp/... and skip the
# release build with BOMA_BINARY=target/debug/boma.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR="${VENDOR_ROOT:-$ROOT/apps/desktop/vendor}"

OS="${1:-auto}"
case "$OS" in
  auto) OS="$(uname -s)" ;;
  Linux*) OS=linux ;;
  Windows*|mingw*|MSYS*|MINGW*|CYGWIN*) OS=windows ;;
  linux|windows) ;;
  *) echo "usage: $0 [linux|windows|auto]" >&2; exit 2 ;;
esac

echo "staging $OS into $VENDOR"

# Clean previous staging so a platform switch never leaves stale files behind.
rm -rf "$VENDOR/postgres" "$VENDOR/boma" "$VENDOR/web"
mkdir -p "$VENDOR/postgres" "$VENDOR/boma" "$VENDOR/web"

# 1) Embedded Postgres (SHA-256 pinned, Maven Central), staged FLAT: the pack expects
# resources/postgres/bin/initdb, not resources/postgres/<os>/bin/initdb.
bash "$ROOT/tools/fetch-embedded-postgres.sh" "$VENDOR/postgres-tmp" "$OS"
mv "$VENDOR/postgres-tmp/$OS"/* "$VENDOR/postgres"/
rm -rf "$VENDOR/postgres-tmp"

# 2) The boma server binary; release by default, overridable for rehearsals.
cargo build --release --locked --bin boma --manifest-path "$ROOT/Cargo.toml"
BOMA_BIN="${BOMA_BINARY:-$ROOT/target/release/boma}"
if [ "$OS" = "windows" ]; then
  cp "${BOMA_BIN}.exe" "$VENDOR/boma/boma.exe"
else
  cp "$BOMA_BIN" "$VENDOR/boma/boma"
fi

# 3) The web export.
if [ ! -f "$ROOT/apps/web/out/index.html" ]; then
  echo "apps/web/out is missing; run npm run build in apps/web first" >&2
  exit 1
fi
cp -r "$ROOT/apps/web/out/." "$VENDOR/web/"

echo "staged $OS -> $VENDOR"
