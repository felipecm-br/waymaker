#!/usr/bin/env python3
"""Check that a match-arm body moved into a function is unchanged, up to rustfmt.

    verify_fmt_move.py --old REV:PATH --arm 'HEADER' --new PATH --fn NAME [--drop-tail N] [--method]

Moving code to a shallower indent lets rustfmt re-wrap it (joining lines, dropping the block
it had added around a long closure body), which verify_move.py reports as a difference. This
takes the old arm (the line equal to HEADER, up to its closing brace), drops its last N body
lines (e.g. a trailing `continue;` that stays at the call site), wraps it in a function at the
new nesting (inside an `impl` with --method), runs rustfmt, and compares the result with the
new function's body, ignoring whitespace. Exit 1 if they differ.
"""
import argparse, os, re, subprocess, sys

ap = argparse.ArgumentParser()
ap.add_argument("--old", required=True); ap.add_argument("--arm", required=True)
ap.add_argument("--new", required=True); ap.add_argument("--fn", required=True, dest="name")
ap.add_argument("--drop-tail", type=int, default=0); ap.add_argument("--method", action="store_true")
a = ap.parse_args()
env = dict(os.environ, PATH=os.environ.get("PATH", "") + ":" + os.path.expanduser("~/.cargo/bin"))

old = subprocess.run(["git", "show", a.old], capture_output=True, text=True, check=True).stdout.split("\n")
i = next((k for k, l in enumerate(old) if l.strip() == a.arm.strip()), None)
if i is None:
    sys.exit(f"arm header not found in {a.old}: {a.arm!r}")
ind = len(old[i]) - len(old[i].lstrip())
j = next(k for k in range(i + 1, len(old)) if old[k] in (" " * ind + "}", " " * ind + "},"))
body = old[i + 1:j - a.drop_tail] if a.drop_tail else old[i + 1:j]
inner = " " * (ind + 4)
body = [l[len(inner):] if l.startswith(inner) else l for l in body]

pad = "        " if a.method else "    "
head = f"fn {a.name}() {{"
src = ("impl X {\n    " + head + "\n" if a.method else head + "\n") + \
      "\n".join(pad + l if l else l for l in body) + ("\n    }\n}\n" if a.method else "\n}\n")
fmt = subprocess.run(["rustfmt", "--edition", "2024", "--emit", "stdout"], input=src,
                     capture_output=True, text=True, env=env)
if fmt.returncode != 0:
    sys.exit("rustfmt failed on the old body:\n" + fmt.stderr[:2000])
out = fmt.stdout.split("\n")
close = "    }" if a.method else "}"
s = next(k for k, l in enumerate(out) if l.strip() == head) + 1
e = max(k for k, l in enumerate(out) if l == close)

new = open(a.new).read().split("\n")
n = next(k for k, l in enumerate(new) if re.match(r"^\s*(pub(\([^)]*\))?\s+)?(async\s+)?fn\s+" + re.escape(a.name) + r"\b", l))
nind = len(new[n]) - len(new[n].lstrip())
ns = next(k for k in range(n, len(new)) if new[k].rstrip().endswith("{")) + 1
ne = next(k for k in range(ns, len(new)) if new[k] == " " * nind + "}")

squash = lambda lines: re.sub(r"\s+", "", "".join(lines))
if squash(out[s:e]) == squash(new[ns:ne]):
    print(f"{a.name}: equal to rustfmt of the old arm body ({e - s} lines)")
    sys.exit(0)
print(f"{a.name}: DIFFERS from rustfmt of the old arm body")
import difflib
print("\n".join(list(difflib.unified_diff([l.strip() for l in out[s:e]], [l.strip() for l in new[ns:ne]], "old (rustfmt)", "new", lineterm=""))[:40]))
sys.exit(1)
