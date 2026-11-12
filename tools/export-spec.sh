#!/usr/bin/env bash
# Usage: ./tools/export-spec.sh <path-to-re-workspace> [--review]
# Copies notes/spec/ into ./spec and writes spec/EXPORT.md (hashes, counts, source).
# No blocking gate: missing files and suspicious lines are warnings. With --review,
# the script pauses after printing the summary so you can inspect before the copy.
# TRACE.md is exported with the rest; spec/ stays writable.
set -euo pipefail

[ $# -ge 1 ] || { echo "Usage: $0 <path-to-re-workspace> [--review]" >&2; exit 1; }
HERE="$(cd "$(dirname "$0")/.." && pwd)"
RE="$(cd "$1" && pwd)"
SPEC="$RE/notes/spec"
REVIEW=0
[ "${2:-}" = "--review" ] && REVIEW=1

[ -d "$SPEC" ] || { echo "No $SPEC. Run /spec in the analysis workspace first." >&2; exit 1; }
[ -f "$SPEC/01-requirements.md" ] || { echo "No 01-requirements.md: the spec is not usable." >&2; exit 1; }
for f in 00-overview.md 02-data-model.md 04-nonfunctional.md 05-known-defects.md 06-decisions-needed.md 07-glossary.md; do
  [ -f "$SPEC/$f" ] || echo "WARNING: missing $f" >&2
done
[ -f "$SPEC/08-ui-ux.md" ] || echo "NOTE: no 08-ui-ux.md, the rebuild will have no UI lane (fine if the software has no UI)" >&2
[ -d "$SPEC/golden" ] || echo "WARNING: no golden/ examples, contract replay will be empty" >&2

echo "== Golden examples =="
python3 - "$SPEC/golden" <<'PY'
import json, pathlib, sys
d = pathlib.Path(sys.argv[1])
files = sorted(d.glob("*.json")) if d.is_dir() else []
bad = 0
for p in files:
    try:
        o = json.loads(p.read_text(encoding="utf-8"))
    except ValueError as e:
        print(f"INVALID JSON {p}: {e}"); bad += 1; continue
    missing = {"request", "response", "notes"} - set(o)
    if missing:
        print(f"MISSING KEYS {p}: {sorted(missing)}"); bad += 1
print(f"{len(files)} API golden example(s), {bad} problem(s).")
PY
python3 "$HERE/tools/check_ui_golden.py" "$SPEC/golden/ui" || true

echo "== Secret scan (advisory) =="
python3 - "$SPEC" <<'PY'
import pathlib, re, sys
PATS = {
    "jwt": r"eyJ[\w-]+\.[\w-]+\.[\w-]",
    "bearer": r"(?i)bearer\s+[\w.~+/=-]{8,}",
    "pem": r"-----BEGIN [A-Z ]*PRIVATE KEY-----",
    "aws": r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b",
    "github": r"\bgh[pousr][A-Za-z0-9]{20,}\b",
    "openai": r"\bsk-[A-Za-z0-9-]{16,}\b",
    "email": r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}",
    "license": r"(?i)\b(?:license|licence)[-]?key\b\s[:=]",
}
hits = 0
for p in sorted(pathlib.Path(sys.argv[1]).rglob("*")):
    if not p.is_file() or p.suffix not in {".md", ".json", ".yaml", ".yml"}:
        continue
    try:
        text = p.read_text(encoding="utf-8")
    except (UnicodeDecodeError, OSError):
        continue
    for n, line in enumerate(text.splitlines(), 1):
        for kind, rx in PATS.items():
            if re.search(rx, line):
                hits += 1
                print(f"SECRET?[{kind}] {p}:{n}")
print(f"{hits} suspicious line(s). Review; this is a warning, not a gate.")
PY

echo "== Summary =="
find "$SPEC" -type f | wc -l | xargs echo "files:"
du -sh "$SPEC" | cut -f1 | xargs echo "size:"
if [ "$REVIEW" = 1 ]; then
  [ -t 0 ] || { echo "Run --review in a terminal." >&2; exit 1; }
  read -r -p "Copy this spec into ./spec now? (type YES): " ok
  [ "$ok" = "YES" ] || { echo "Aborted."; exit 1; }
fi

mkdir -p "$HERE/spec"
BAK="$HERE/spec.bak-$(date -u +%Y%m%d-%H%M%S)"
moved=0
for f in "$HERE/spec"/* "$HERE/spec"/.[!.]*; do
  [ -e "$f" ] || continue
  [ "$(basename "$f")" = "README.md" ] && continue
  mkdir -p "$BAK"
  mv "$f" "$BAK/"
  moved=1
done
[ "$moved" = 1 ] && echo "Previous spec backed up to $BAK"

python3 - "$SPEC" "$HERE/spec" <<'PY'
import shutil, sys
shutil.copytree(sys.argv[1], sys.argv[2], dirs_exist_ok=True)
PY

python3 - "$HERE/spec" "$RE" <<'PY'
import hashlib, pathlib, re, sys, datetime
spec, re_ws = pathlib.Path(sys.argv[1]), sys.argv[2]
rows, reqs, golden, ui = [], set(), 0, 0
for p in sorted(spec.rglob("*")):
    if not p.is_file() or p.name in {"README.md", "EXPORT.md"}:
        continue
    b = p.read_bytes()
    rows.append((p.relative_to(spec).as_posix(), hashlib.sha256(b).hexdigest(), len(b)))
    if p.name == "01-requirements.md":
        reqs = set(re.findall(r"REQ-\d{3,}", b.decode("utf-8", "replace")))
    if p.parent.name == "golden" and p.suffix == ".json":
        golden += 1
    if p.parent.name == "ui" and p.suffix == ".json":
        ui += 1
stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
out = ["# Spec export manifest", "",
       f"- Exported (UTC): {stamp}",
       f"- Source workspace: {re_ws}",
       f"- Requirements: {len(reqs)}",
       f"- Golden examples: {golden}",
       f"- UI golden examples: {ui}",
       "- Gates: none blocking; golden sanity and secret scan are advisory",
       "",
       "| file | sha256 | bytes |",
       "|---|---|---|"]
out += [f"| {n} | {h} | {s} |" for n, h, s in rows]
out += ["", "Verify later with: `sha256sum -c` against this table.", ""]
(spec / "EXPORT.md").write_text("\n".join(out), encoding="utf-8")
print(f"Manifest: {len(rows)} files, {len(reqs)} REQ ids, {golden} API golden examples, {ui} UI golden examples.")
PY
echo "Exported to $HERE/spec."
echo "Next: /plan in this workspace."
