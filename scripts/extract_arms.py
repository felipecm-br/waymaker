#!/usr/bin/env python3
"""Move a contiguous run of `match` arms into a function that matches on them, unchanged.

    extract_arms.py FILE FIRST LAST NAME --sig SIG --call ARGS [--var VAR] [--fallthrough EXPR]

FIRST..LAST (1-based, inclusive) must be whole arms of a `match VAR {` whose arms sit 8 spaces
deep. They move verbatim (same indentation) into

    SIG {
        match VAR {
            <arms>
            _ => {}
        }
        EXPR            # --fallthrough, e.g. `Flow::Proceed`; omitted if not given
    }

and are replaced by one arm `a @ (P1 | P2 | ...) => return NAME(a, ARGS),` (without `return`
when there is no --fallthrough), where each Pn is an arm's pattern with its bindings replaced
by `..`. Every moved arm either returns or falls through exactly as before, so control flow is
unchanged. Check with: verify_move.py ... --fn NAME --trim-head 1 --trim-tail 3 (or 2).
"""
import argparse, re, sys

ap = argparse.ArgumentParser()
ap.add_argument("file"); ap.add_argument("first", type=int); ap.add_argument("last", type=int)
ap.add_argument("name"); ap.add_argument("--sig", required=True); ap.add_argument("--call", required=True)
ap.add_argument("--var", default="action"); ap.add_argument("--fallthrough")
ap.add_argument("--doc", default="")
a = ap.parse_args()
src = open(a.file).read().split("\n")
f, l = a.first - 1, a.last - 1
arms = src[f:l + 1]
if not re.match(r"^ {8}\S", arms[0]):
    sys.exit(f"line {a.first} does not start an arm: {arms[0]!r}")
if not (arms[-1] in ("        }", "        },") or re.match(r"^ {8}\S.*,$", arms[-1])):
    sys.exit(f"line {a.last} does not end an arm: {arms[-1]!r}")
if f > 0 and src[f - 1].startswith(" " * 9):
    sys.exit(f"line {a.first - 1} is inside an arm; FIRST must follow a complete arm")

# Arm headers: lines at exactly 8 spaces that contain `=>` and are not comments.
pats, i = [], 0
while i < len(arms):
    line = arms[i]
    if re.match(r"^ {8}[^\s/}]", line):
        head = line.strip()
        while "=>" not in head:           # multi-line patterns
            i += 1; head += " " + arms[i].strip()
        pat = head.split("=>")[0].strip()
        pat = re.sub(r"\b(Action::\w+)\s*\([^()]*\)", r"\1(..)", pat)
        pat = re.sub(r"\b(Action::\w+)\s*\{[^{}]*\}", r"\1 { .. }", pat)
        pat = re.sub(r"\s+if\s+.*$", "", pat)
        if " if " in head.split("=>")[0]:
            sys.exit(f"arm with a guard cannot be grouped safely: {head!r}")
        pats.append(pat)
    i += 1

body = [a.sig + " {", f"    match {a.var} {{"] + arms + ["        _ => {}", "    }"]
if a.fallthrough:
    body.append("    " + a.fallthrough)
body.append("}")
if a.doc:
    body = ["/// " + d for d in a.doc.split("\n")] + body
ret = "return " if a.fallthrough else ""
call = f"        a @ ({' | '.join(pats)}) => {ret}{a.name}(a, {a.call}),"
src[f:l + 1] = [call]
out = "\n".join(src).rstrip("\n") + "\n\n" + "\n".join(body) + "\n"
open(a.file, "w").write(out)
print(f"{a.name}: moved {len(arms)} lines, {len(pats)} arms")
