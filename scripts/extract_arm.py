#!/usr/bin/env python3
"""Move the body of a `match` arm into its own function, unchanged.

    extract_arm.py FILE LINE NAME --sig SIG --call CALL [--dest DEST] [--doc DOC]

LINE is the 1-based line of `PATTERN => {`. The arm ends at the first later line that is
`}` or `},` at the arm's own indentation (rustfmt layout). The body is re-indented to sit
directly inside `SIG { ... }`, appended to DEST (default: FILE), and the arm becomes
`PATTERN => CALL,`. Prints whether the body uses `.await`, `return`, `?`, `break` or
`continue`, which change meaning when moved into a function: review those by hand.
"""
import argparse, re, sys

ap = argparse.ArgumentParser()
ap.add_argument("file"); ap.add_argument("line", type=int); ap.add_argument("name")
ap.add_argument("--sig", required=True); ap.add_argument("--call", required=True)
ap.add_argument("--dest"); ap.add_argument("--doc", default="")
a = ap.parse_args()

src = open(a.file).read().split("\n")
i = a.line - 1
head = src[i]
m = re.match(r"^(\s*)(.*\S)\s*=>\s*\{$", head)
if not m:
    sys.exit(f"line {a.line} is not `PATTERN => {{`: {head!r}")
indent, pattern = m.group(1), m.group(2)
j = next((k for k in range(i + 1, len(src)) if src[k] in (indent + "}", indent + "},")), None)
if j is None:
    sys.exit("no closing brace at the arm's indentation")
body = src[i + 1:j]
inner = indent + "    "
for l in body:
    if l and not l.startswith(inner):
        sys.exit(f"body line not indented under the arm: {l!r}")
moved = [l[len(inner):] if l else l for l in body]
flags = {k: sum(len(re.findall(p, l.split("//")[0])) for l in moved)
         for k, p in [(".await", r"\.await\b"), ("return", r"\breturn\b"), ("?", r"\?(?=[;.)\s])"),
                      ("break", r"\bbreak\b"), ("continue", r"\bcontinue\b")]}
fn = []
if a.doc:
    fn += ["/// " + d for d in a.doc.split("\n")]
fn += [a.sig + " {"] + ["    " + l if l else l for l in moved] + ["}"]

src[i:j + 1] = [f"{indent}{pattern} => {a.call},"]
if a.dest and a.dest != a.file:
    open(a.file, "w").write("\n".join(src))
    d = open(a.dest).read().rstrip("\n")
    open(a.dest, "w").write(d + "\n\n" + "\n".join(fn) + "\n")
else:
    out = "\n".join(src).rstrip("\n") + "\n\n" + "\n".join(fn) + "\n"
    open(a.file, "w").write(out)
print(f"{a.name}: moved {len(body)} lines from {a.file}:{a.line}; " +
      (", ".join(f"{k}={v}" for k, v in flags.items() if v) or "no control-flow keywords"))
