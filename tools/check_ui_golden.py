#!/usr/bin/env python3
"""Validate the UI golden examples in spec/golden/ui/*.json.
Usage: check_ui_golden.py [dir=spec/golden/ui] [--json]

Shape: {"state": {"route": "/orders", "seed": "empty", "role": "admin", "viewport": "360x800", "media": {"colorScheme": "dark", "reducedMotion": "reduce"}}, "expect": [{"role": "table", "count": 0}, {"role": "status", "state": "visible"}], "notes": "why this state matters"}

A golden pins a seeded screen state, never a click path: the click path belongs to the REQ acceptance criteria, implemented as a Playwright test.
A name is allowed only when the spec itself defines the wording (never the original's).
Exit 0 when the directory is absent, 1 on any problem.
"""
import argparse
import json
import pathlib
import re
import sys

NAME = re.compile(r"^REQ-\d{3,}-[a-z0-9]+(?:-[a-z0-9]+)*\.json$")
TOP = {"state", "expect", "notes"}
STATE = {"route", "seed", "role", "viewport", "media"}
MEDIA = {"colorScheme": {"light", "dark"}, "reducedMotion": {"reduce", "no-preference"}}
EXPECT = {"role", "name", "state", "count"}
STATES = {"visible", "hidden", "disabled", "enabled", "checked", "unchecked", "focused", "absent"}

def problems(p):
    out = []
    if not NAME.match(p.name):
        out.append(f"{p}: file name must be REQ-<3+ digits>-<slug>.json")
    try:
        o = json.loads(p.read_text(encoding="utf-8"))
    except (ValueError, OSError) as e:
        return out + [f"{p}: not valid JSON ({e})"]
    if not isinstance(o, dict):
        return out + [f"{p}: top level must be an object"]
    extra, miss = set(o) - TOP, TOP - set(o)
    if extra:
        out.append(f"{p}: unknown top-level keys {sorted(extra)}")
    if miss:
        out.append(f"{p}: missing keys {sorted(miss)}")
    st = o.get("state")
    if not isinstance(st, dict):
        out.append(f"{p}: state must be an object")
    else:
        e = set(st) - STATE
        if e:
            out.append(f"{p}: unknown state keys {sorted(e)}")
        route = st.get("route")
        if not isinstance(route, str) or not route.startswith("/"):
            out.append(f"{p}: state.route must be a string starting with /")
        for k in ("seed", "role"):
            if k in st and not isinstance(st[k], str):
                out.append(f"{p}: state.{k} must be a string")
        if "viewport" in st:
            v = st["viewport"]
            if not isinstance(v, str) or not re.match(r"^\d{2,4}x\d{2,4}$", v):
                out.append(f"{p}: state.viewport must look like 360x800")
        if "media" in st:
            m = st["media"]
            if not isinstance(m, dict):
                out.append(f"{p}: state.media must be an object")
            else:
                for k, v in m.items():
                    if k not in MEDIA:
                        out.append(f"{p}: unknown media key {k}")
                    elif v not in MEDIA[k]:
                        out.append(f"{p}: media.{k} must be one of {sorted(MEDIA[k])}")
    ex = o.get("expect")
    if not isinstance(ex, list) or not ex:
        out.append(f"{p}: expect must be a non-empty list")
    else:
        for i, it in enumerate(ex):
            if not isinstance(it, dict):
                out.append(f"{p}: expect[{i}] must be an object")
                continue
            e = set(it) - EXPECT
            if e:
                out.append(f"{p}: expect[{i}] unknown keys {sorted(e)}")
            if not isinstance(it.get("role"), str) or not it.get("role"):
                out.append(f"{p}: expect[{i}].role is required")
            if "name" in it and not isinstance(it["name"], str):
                out.append(f"{p}: expect[{i}].name must be a string")
            if "state" in it and it["state"] not in STATES:
                out.append(f"{p}: expect[{i}].state must be one of {sorted(STATES)}")
            if "count" in it:
                c = it["count"]
                if not isinstance(c, int) or isinstance(c, bool) or c < 0:
                    out.append(f"{p}: expect[{i}].count must be an integer >= 0")
            if len(it) < 2:
                out.append(f"{p}: expect[{i}] needs role plus name, state or count")
    if not isinstance(o.get("notes"), str) or not o.get("notes", "").strip():
        out.append(f"{p}: notes must be a non-empty string")
    return out

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("directory", nargs="?", default="spec/golden/ui")
    ap.add_argument("--json", action="store_true")
    a = ap.parse_args()
    d = pathlib.Path(a.directory)
    files = sorted(d.glob("*.json")) if d.is_dir() else []
    issues = []
    for p in files:
        issues.extend(problems(p))
    if a.json:
        print(json.dumps({"files": len(files), "issues": issues}, indent=2))
    else:
        for i in issues:
            print(f"UI-GOLDEN {i}")
        print(f"{len(files)} UI golden example(s), {len(issues)} problem(s).")
    sys.exit(1 if issues else 0)

if __name__ == "__main__":
    main()
