#!/usr/bin/env bash
# Fetch and extract the zonkyio embedded PostgreSQL binaries (Apache-2.0) for the
# self-contained desktop (M35). Build-time only: users never download anything at install
# time; the desktop packs the extracted tree under resources/postgres/.
#
#   tools/fetch-embedded-postgres.sh [target_dir] [linux|windows|all]
#
# Re-runnable: the jar is checksummed (published SHA-256 from Maven Central), extracted, and
# the extracted top level is printed for the receipt in docs/DEPENDENCIES.md.
set -euo pipefail

PG_VERSION="17.12.0"
BASE="https://repo1.maven.org/maven2/io/zonky/test/postgres"

isha() { # sha256 of a file, portable
  python3 - "$1" <<'PY'
import hashlib, sys
print(hashlib.sha256(open(sys.argv[1], "rb").read()).hexdigest())
PY
}

extract() { # jar -> inner txz -> target dir
  python3 - "$1" "$2" <<'PY'
import io, lzma, sys, tarfile, zipfile
jar, target = sys.argv[1], sys.argv[2]
with zipfile.ZipFile(jar) as zf:
    txz_names = [n for n in zf.namelist() if n.endswith(".txz")]
    if len(txz_names) != 1:
        raise SystemExit(f"expected exactly one .txz in the jar, found {txz_names}")
    txz = zf.read(txz_names[0])
with lzma.open(io.BytesIO(txz)) as xz:
    with tarfile.open(fileobj=xz) as tar:
        tar.extractall(target, filter="data")
PY
}

fetch_one() { # artifact os-label
  local artifact="$1" label="$2" expected tmp out
  case "$label" in
    linux)   expected="7a8951dc117490f2d4161600836c8b1055306aff86e0a472aad55457651e5cbd" ;;
    windows) expected="bebadbe868ed53adb1c12033a381e41005915205ea9fedaa2349f5467fa7016b" ;;
    *) echo "unknown os: $label" >&2; exit 2 ;;
  esac
  tmp="$(mktemp -d)"; trap "rm -rf '$tmp'" EXIT
  # Maven's CDN rate-limits shared runner IPs (429) as well as returning transient 404s.
  # Retry with a real backoff, then try the second canonical host; the SHA-256 gate below is
  # unchanged, so a different host can never supply different bytes.
  local urls=(
    "$BASE/$artifact/$PG_VERSION/$artifact-$PG_VERSION.jar"
    "https://repo.maven.apache.org/maven2/io/zonky/test/postgres/$artifact/$PG_VERSION/$artifact-$PG_VERSION.jar"
  )
  local url fetched=0
  for url in "${urls[@]}"; do
    if curl -fSL --retry 8 --retry-all-errors --retry-delay 6 -o "$tmp/pg.jar" "$url"; then
      fetched=1
      break
    fi
  done
  if [ "$fetched" -ne 1 ]; then
    echo "could not fetch $artifact-$PG_VERSION.jar from any mirror" >&2
    exit 1
  fi
  local actual
  actual="$(isha "$tmp/pg.jar")"
  if [ "$actual" != "$expected" ]; then
    echo "sha256 mismatch for $artifact-$PG_VERSION.jar: got $actual, want $expected" >&2
    exit 1
  fi
  out="$TARGET_DIR/$label"
  rm -rf "$out" && mkdir -p "$out"
  extract "$tmp/pg.jar" "$out"
  rm -rf "$tmp"; trap - EXIT
  echo "extracted $label (postgres ${PG_VERSION}, sha256 $actual) -> $out"
  du -sh "$out" | awk '{print "  size: " $1}'
  ls "$out"
}

TARGET_DIR="${1:-apps/desktop/vendor/postgres}"
case "${2:-auto}" in
  linux)   fetch_one "embedded-postgres-binaries-linux-amd64" linux ;;
  windows) fetch_one "embedded-postgres-binaries-windows-amd64" windows ;;
  all)     fetch_one "embedded-postgres-binaries-linux-amd64" linux
           fetch_one "embedded-postgres-binaries-windows-amd64" windows ;;
  auto)
    case "$(uname -s)" in
      Linux*)  fetch_one "embedded-postgres-binaries-linux-amd64" linux ;;
      MINGW*|MSYS*|CYGWIN*) fetch_one "embedded-postgres-binaries-windows-amd64" windows ;;
      *) echo "unknown platform $(uname -s); pass linux|windows|all explicitly" >&2; exit 2 ;;
    esac ;;
  *) echo "usage: $0 [target_dir] [linux|windows|all|auto]" >&2; exit 2 ;;
esac
