#!/usr/bin/env python3
"""Check that every REQ id of the spec is named by at least one test declaration.
Usage: trace_check.py [spec_dir=spec] [repo_root=.] [--json]

Evidence counts only on a declaration line (fn, def, func, it, test, describe, scenario, feature) that carries the id, for example
fn req_012_refund_is_idempotent()
it("REQ-012 refunds are idempotent", ...)
A mere mention in a comment or a doc does NOT count: the id must be in the test name.
REQ-012 and req_012 are equivalent.
Ids named by tests but absent from the spec are reported as ORPHAN.
Exit 1 if any REQ has no test.
"""
import json
import pathlib
import re
import sys

REQ = re.compile(r"REQ[-_]?(\d{3,})", re.I)
DECL = re.compile(r"\b(fn|def|func|it|test|describe|scenario|feature)\b[^\n]*REQ[-_]?\d{3,}", re.I)
SKIP = {".git", "node_modules", "__pycache__", ".venv", "venv", "dist", "build", "spec", "coverage", "docs", "target", ".next"}
SRC_EXT = {".rs", ".py", ".ts", ".tsx", ".js", ".jsx", ".mjs", ".go", ".kt", ".java", ".dart", ".svelte", ".vue", ".rb", ".cs", ".php", ".swift", ".feature"}

def main():
    args = [x for x in sys.argv[1:] if not x.startswith("--")]
    as_json = "--json" in sys.argv
    spec = pathlib.Path(args[0] if args else "spec")
    root = pathlib.Path(args[1] if len(args) > 1 else ".")
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
    if as_json:
        print(json.dumps({"total": len(wanted), "covered": len(wanted & set(found)),
                          "missing": [f"REQ-{m}" for m in missing],
                          "orphan": [f"REQ-{o}" for o in orphan],
                          "evidence": found}, indent=2))
    else:
        print(f"{len(wanted & set(found))}/{len(wanted)} REQ ids named by a test declaration")
        for m in missing:
            print(f"MISSING REQ-{m}")
        for o in orphan:
            print(f"ORPHAN REQ-{o} ({found[o]}) is not in the spec")
    sys.exit(1 if missing else 0)

if __name__ == "__main__":
    main()
