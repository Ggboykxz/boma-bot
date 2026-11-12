#!/usr/bin/env python3
"""Check that every REQ id of the spec is named by at least one test declaration.
Usage: trace_check.py [spec_dir=spec] [repo_root=.] [--json] [--max-missing N]

Evidence counts only on a declaration line (fn, def, func, it, test, describe, scenario, feature) that carries the id, for example
fn req_012_refund_is_idempotent()
it("REQ-012 refunds are idempotent", ...)
A mere mention in a comment or a doc does NOT count: the id must be in the test name.
REQ-012 and req_012 are equivalent.
Ids named by tests but absent from the spec are reported as ORPHAN.

Exit 1 if any REQ has no test, unless --max-missing N is given: the gate is then a ratchet that
fails when more than N requirements are still uncovered. The budget only ever decreases; see
docs/STATE.md for its value per milestone.
"""
import json
import pathlib
import re
import sys

REQ = re.compile(r"REQ[-_]?(\d{3,})", re.I)
DECL = re.compile(r"\b(fn|def|func|it|test|describe|scenario|feature)\b[^\n]*REQ[-_]?\d{3,}", re.I)
SKIP = {".git", "node_modules", "__pycache__", ".venv", "venv", "dist", "build", "spec", "coverage", "docs", "target", ".next"}
SRC_EXT = {".rs", ".py", ".ts", ".tsx", ".js", ".jsx", ".mjs", ".go", ".kt", ".java", ".dart", ".svelte", ".vue", ".rb", ".cs", ".php", ".swift", ".feature"}


def parse_args(argv):
    positional, as_json, max_missing = [], False, None
    i = 0
    while i < len(argv):
        arg = argv[i]
        if arg == "--json":
            as_json = True
        elif arg == "--max-missing":
            i += 1
            if i >= len(argv):
                sys.exit("--max-missing needs a value")
            max_missing = int(argv[i])
        elif arg.startswith("--max-missing="):
            max_missing = int(arg.split("=", 1)[1])
        elif arg.startswith("--"):
            sys.exit(f"unknown option: {arg}")
        else:
            positional.append(arg)
        i += 1
    return positional, as_json, max_missing


def main():
    positional, as_json, max_missing = parse_args(sys.argv[1:])
    spec = pathlib.Path(positional[0] if positional else "spec")
    root = pathlib.Path(positional[1] if len(positional) > 1 else ".")
    wanted = set()
    for p in spec.rglob("01-requirements.md"):
        wanted |= {m.group(1) for m in REQ.finditer(p.read_text(encoding="utf-8"))}
    if not wanted:
        sys.exit(f"No REQ ids found in {spec}/01-requirements.md")
    found = {}
    for p in sorted(root.rglob("*")):
        rel = p.relative_to(root).parts
        if not p.is_file() or set(rel) & SKIP or p.suffix not in SRC_EXT:
            continue
        try:
            text = p.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        for n, line in enumerate(text.splitlines(), 1):
            if DECL.search(line):
                for rid in {m.group(1) for m in REQ.finditer(line)}:
                    found.setdefault(rid, f"{p}:{n}")
    missing = sorted(wanted - set(found))
    orphan = sorted(set(found) - wanted)
    covered = len(wanted & set(found))
    if as_json:
        print(json.dumps({"total": len(wanted), "covered": covered,
                          "missing": [f"REQ-{m}" for m in missing],
                          "orphan": [f"REQ-{o}" for o in orphan],
                          "max_missing": max_missing,
                          "evidence": found}, indent=2))
    else:
        print(f"{covered}/{len(wanted)} REQ ids named by a test declaration")
        for m in missing:
            print(f"MISSING REQ-{m}")
        for o in orphan:
            print(f"ORPHAN REQ-{o} ({found[o]}) is not in the spec")
        if max_missing is not None:
            print(f"budget: {len(missing)} missing, allowed {max_missing}")
    if max_missing is None:
        sys.exit(1 if missing else 0)
    sys.exit(1 if len(missing) > max_missing else 0)


if __name__ == "__main__":
    main()
