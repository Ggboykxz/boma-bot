#!/usr/bin/env bash
# Rehearse the desktop's first-launch bring-up against staged vendor/ resources, without a GUI:
# the exact commands and environment the desktop runs, in order. Catches packaging mistakes
# (missing resources, wrong layout, broken args) in CI on every push.
#
#   tools/desktop-stack-smoke.sh [vendor_dir]
set -euo pipefail

VENDOR="${1:-apps/desktop/vendor}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
[ -d "$VENDOR" ] || VENDOR="$ROOT/$VENDOR"
[ -x "$VENDOR/postgres/bin/initdb" ] || { echo "missing staged postgres in $VENDOR (run tools/stage-desktop-vendor.sh)" >&2; exit 1; }
[ -f "$VENDOR/boma/boma" ] || [ -f "$VENDOR/boma/boma.exe" ] || { echo "missing staged boma binary in $VENDOR" >&2; exit 1; }
[ -f "$VENDOR/web/index.html" ] || { echo "missing staged web export in $VENDOR" >&2; exit 1; }

SCRATCH="$(mktemp -d -t boma-stack-smoke.XXXXXX)"
trap 'rm -rf "$SCRATCH"' EXIT

MASTER_KEY="$(python3 -c 'import secrets; print(secrets.token_hex(32))')"
PGPORT="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"
HTTP_PORT="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"

echo "== initdb"
"$VENDOR/postgres/bin/initdb" -D "$SCRATCH/pgdata" -U boma --auth=trust --no-locale -E UTF8 >"$SCRATCH/initdb.log"
echo "== postgres on $PGPORT"
"$VENDOR/postgres/bin/pg_ctl" -D "$SCRATCH/pgdata" -l "$SCRATCH/postgres.log" -o "-p $PGPORT" -w start >/dev/null

BOMA_BIN="$VENDOR/boma/${BOMA_BIN_NAME:-boma}"
[ -f "$VENDOR/boma/boma.exe" ] && BOMA_BIN="$VENDOR/boma/boma.exe"
# Rehearsal hook: point the loader at a locally built OpenSSL when the system lacks one.
[ -n "${BOMA_LD_LIBRARY_PATH:-}" ] && export LD_LIBRARY_PATH="${BOMA_LD_LIBRARY_PATH}${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

export BOMA_LOCAL_FIRST=true
export BOMA_DATABASE_URL="postgres://boma@127.0.0.1:$PGPORT/postgres"
export BOMA_BIND_ADDR="127.0.0.1:$HTTP_PORT"
export BOMA_WEB_ROOT="$VENDOR/web"
export BOMA_MASTER_KEY="$MASTER_KEY"
export BOMA_ARTIFACT_DIR="$SCRATCH/artifacts"
export BOMA_LOG_FORMAT=pretty
export SANDBOX_PROVIDER=none

echo "== migrate"
"$BOMA_BIN" migrate >/dev/null

echo "== serve + worker"
"$BOMA_BIN" serve >"$SCRATCH/boma.log" 2>&1 &
SERVE_PID=$!
"$BOMA_BIN" worker >>"$SCRATCH/boma.log" 2>&1 &
WORKER_PID=$!

CODE=000
for _ in $(seq 1 90); do
  CODE="$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$HTTP_PORT/health" || true)"
  [ "$CODE" = "200" ] && break
  sleep 1
done

if [ "$CODE" != "200" ]; then
  echo "stack never became healthy (last code $CODE); log tail:" >&2
  tail -30 "$SCRATCH/boma.log" >&2 || true
  kill "$SERVE_PID" "$WORKER_PID" 2>/dev/null || true
  "$VENDOR/postgres/bin/pg_ctl" -D "$SCRATCH/pgdata" -m fast -w stop >/dev/null 2>&1 || true
  exit 1
fi

INDEX="$(curl -s "http://127.0.0.1:$HTTP_PORT/")"
case "$INDEX" in
  *"<html"*|*"<!DOCTYPE"*|*"<!doctype"*) echo "web export served at /" ;;
  *) echo "the web root did not serve HTML; refusing to call this a working app" >&2; exit 1 ;;
esac

BOT="$(curl -s -X POST "http://127.0.0.1:$HTTP_PORT/rpc/bots/create" \
  -H 'content-type: application/json' \
  -d '{"name":"Smoke","description":"staged-stack smoke"}' \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
echo "rpc round-trip ok (created $BOT)"

kill "$SERVE_PID" "$WORKER_PID" 2>/dev/null || true
wait "$SERVE_PID" "$WORKER_PID" 2>/dev/null || true
"$VENDOR/postgres/bin/pg_ctl" -D "$SCRATCH/pgdata" -m fast -w stop >/dev/null
echo "smoke ok"
