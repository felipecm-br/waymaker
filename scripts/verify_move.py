#!/usr/bin/env python3
"""Check that functions created by a refactor contain code moved verbatim from the old source.

    verify_move.py --old REV:PATH [--old REV:PATH ...] --new PATH [--new PATH ...] \
                   --fn NAME [--fn NAME ...] [--rename OLD=NEW ...] [--allow-sig]

For each NAME, the body of `fn NAME` in the --new files (between its `{` line and the closing
brace at the same indentation) must appear verbatim in an --old file, after applying --rename to the old text. Whitespace is
ignored entirely, so rustfmt re-wrapping and re-indenting moved code does not count as a change
(rustfmt never alters string literals), and neither is its `=> { expr }` to `=> expr,` rewrite. Exits 1 on any mismatch, with a diff against the closest
old block.
"""
import argparse, difflib, re, subprocess, sys

ap = argparse.ArgumentParser()
ap.add_argument("--old", action="append", required=True)
ap.add_argument("--new", action="append", required=True)
ap.add_argument("--fn", action="append", required=True, dest="fns")
ap.add_argument("--rename", action="append", default=[])
a = ap.parse_args()

def norm(lines):
    return [l.strip() for l in lines if l.strip()]

old = []
for spec in a.old:
    text = subprocess.run(["git", "show", spec], capture_output=True, text=True, check=True).stdout
    for r in a.rename:
        o, n = r.split("=", 1)
        text = re.sub(r"\b" + re.escape(o) + r"\b", n, text) if o[0].isalnum() else text.replace(o, n)
    old += norm(text.split("\n")) + ["\0"]

new_src = [l for p in a.new for l in open(p).read().split("\n") + [""]]

def body_of(name):
    for i, l in enumerate(new_src):
        m = re.match(r"^(\s*)(pub(\([^)]*\))?\s+)?(async\s+)?fn\s+" + re.escape(name) + r"\b", l)
        if not m:
            continue
        indent = m.group(1)
        k = next(k for k in range(i, len(new_src)) if new_src[k].rstrip().endswith("{"))
        end = next(e for e in range(k + 1, len(new_src)) if new_src[e] == indent + "}")
        return new_src[k + 1:end]
    sys.exit(f"fn {name} not found in {a.new}")

def squash(lines):
    text = re.sub(r"\s+", "", "".join(lines))
    # rustfmt rewrites a match arm `=> { expr }` as `=> expr,` once it fits on one line.
    text = re.sub(r"=>\{([^{};]*)\}", r"=>\1,", text)
    return text.replace(",,", ",").replace(",}", "}")

old_squashed = squash(old)

def find(seq, block):
    return squash(block) in old_squashed

ok = True
for name in a.fns:
    body = norm(body_of(name))
    if find(old, body):
        print(f"{name}: {len(body)} lines moved verbatim")
        continue
    ok = False
    sm = max((difflib.SequenceMatcher(None, old[i:i + len(body)], body) for i in range(0, max(1, len(old) - len(body)))),
             key=lambda s: s.ratio())
    print(f"{name}: DIFFERS from the old code (best match {sm.ratio():.0%})")
    print("\n".join(list(difflib.unified_diff(sm.a, sm.b, "old", "new", lineterm=""))[:40]))
sys.exit(0 if ok else 1)
