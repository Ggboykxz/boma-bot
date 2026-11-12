#!/usr/bin/env python3
"""Refuse a credential in the tree.
Usage: secret_scan.py [root=.] [--json]

High-confidence patterns only: a scanner that cries wolf gets ignored. Files under .git,
node_modules, target, dist, coverage and the spec backup directory are skipped. A test fixture that
must contain a connection string marks its own line with `secret-scan: allow` and a reason, so the
exemption stays visible in review instead of exempting a whole file.
Exit 1 on any finding.
"""
import json
import pathlib
import re
import sys

PATTERNS = [
    ("github-token", re.compile(r"gh[pousr]_[A-Za-z0-9]{30,}")),
    ("aws-access-key", re.compile(r"\bAKIA[0-9A-Z]{16}\b")),
    ("google-api-key", re.compile(r"\bAIza[0-9A-Za-z_-]{35}\b")),
    ("slack-token", re.compile(r"\bxox[abposr]-[A-Za-z0-9-]{10,}")),
    ("private-key", re.compile(r"-----BEGIN (RSA |EC |OPENSSH |PGP )?PRIVATE KEY-----")),
    ("openai-key", re.compile(r"\bsk-[A-Za-z0-9]{32,}\b")),
    ("anthropic-key", re.compile(r"\bsk-ant-[A-Za-z0-9_-]{20,}\b")),
    ("postgres-url-with-password", re.compile(r"postgres(?:ql)?://[^:\s/]+:[^@\s]{6,}@")),
]

SKIP_DIRS = {".git", "node_modules", "target", "dist", "build", "coverage", "__pycache__", ".next", "test-results", "playwright-report"}
SKIP_SUFFIXES = {".png", ".jpg", ".jpeg", ".gif", ".ico", ".woff", ".woff2", ".pdf", ".zip", ".lock"}
# A connection string in a documentation example or in the sample environment is intentional.
ALLOW = {
    ("postgres-url-with-password", "compose.yaml"),
    ("postgres-url-with-password", ".env.example"),
    ("postgres-url-with-password", "docs/DEPENDENCIES.md"),
    ("postgres-url-with-password", "README.md"),
}


def main():
    argv = [a for a in sys.argv[1:] if not a.startswith("--")]
    as_json = "--json" in sys.argv
    root = pathlib.Path(argv[0] if argv else ".")
    findings = []
    for path in sorted(root.rglob("*")):
        if not path.is_file():
            continue
        rel_parts = path.relative_to(root).parts
        if set(rel_parts) & SKIP_DIRS or path.suffix in SKIP_SUFFIXES:
            continue
        if path.name == ".env" or (path.name.startswith(".env.") and path.name != ".env.example"):
            findings.append({"file": str(path), "line": 0, "rule": "committed-env-file"})
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        rel = str(path.relative_to(root))
        for number, line in enumerate(text.splitlines(), 1):
            # An inline marker keeps a deliberate fixture visible in review instead of a
            # file-wide exemption that could later hide a real credential.
            if "secret-scan: allow" in line:
                continue
            for rule, pattern in PATTERNS:
                if pattern.search(line) and (rule, rel) not in ALLOW:
                    findings.append({"file": rel, "line": number, "rule": rule})
    if as_json:
        print(json.dumps({"findings": findings}, indent=2))
    elif findings:
        for finding in findings:
            print(f"SECRET {finding['file']}:{finding['line']} ({finding['rule']})")
    else:
        print("no credential pattern found")
    sys.exit(1 if findings else 0)


if __name__ == "__main__":
    main()
