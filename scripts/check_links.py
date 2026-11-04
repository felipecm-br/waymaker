#!/usr/bin/env python3
"""Check relative links and #anchors in the repository's Markdown files.

Offline and dependency-free: external (http, https, mailto) links are skipped.
For every tracked *.md file (outside .agents/), each [text](target) and
<img src="target"> must point at an existing file or directory, and a
#fragment must match a heading in the target file (GitHub's anchor rules).

Usage: python3 scripts/check_links.py   (exit 1 on any broken link)
"""

import re
import subprocess
import sys
import unicodedata
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LINK = re.compile(r"(?<!!)\[(?:[^\]\[]|\[[^\]]*\])*\]\(\s*<?([^)\s>]+)>?(?:\s+\"[^\"]*\")?\s*\)")
IMAGE = re.compile(r"!\[[^\]]*\]\(\s*<?([^)\s>]+)>?(?:\s+\"[^\"]*\")?\s*\)")
HTML_SRC = re.compile(r"<(?:img|source|a)\b[^>]*?\b(?:src|href)=\"([^\"]+)\"", re.I)
FENCE = re.compile(r"^\s*(```|~~~)")
HEADING = re.compile(r"^\s{0,3}(#{1,6})\s+(.*?)\s*#*\s*$")
EXPLICIT_ANCHOR = re.compile(r"<a\s+(?:name|id)=\"([^\"]+)\"", re.I)


def tracked_markdown() -> list[Path]:
    out = subprocess.run(
        ["git", "ls-files", "-z", "*.md"], cwd=ROOT, capture_output=True, check=True
    ).stdout.decode()
    return [ROOT / p for p in out.split("\0") if p and not p.startswith(".agents/")]


def strip_code(text: str) -> list[str]:
    """Lines of `text` with fenced code blocks and inline code removed."""
    lines, in_fence = [], False
    for line in text.splitlines():
        if FENCE.match(line):
            in_fence = not in_fence
            lines.append("")
            continue
        lines.append("" if in_fence else re.sub(r"`[^`]*`", "", line))
    return lines


def slug(heading: str) -> str:
    """GitHub's heading anchor: lowercase, drop punctuation, spaces to hyphens."""
    text = re.sub(r"<[^>]+>", "", heading)  # inline HTML
    text = re.sub(r"!?\[([^\]]*)\]\([^)]*\)", r"\1", text)  # links -> text
    text = text.replace("`", "").replace("\ufe0f", "").lower()
    kept = []
    for ch in text:
        cat = unicodedata.category(ch)
        if ch in " -_" or cat[0] in "LN" or cat == "Mn":
            kept.append(ch)
    return "".join(kept).replace(" ", "-")


_anchor_cache: dict[Path, set[str]] = {}


def anchors(path: Path) -> set[str]:
    if path not in _anchor_cache:
        seen: dict[str, int] = {}
        found = set()
        in_fence = False
        for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
            if FENCE.match(line):
                in_fence = not in_fence
                continue
            if in_fence:
                continue
            found.update(EXPLICIT_ANCHOR.findall(line))
            m = HEADING.match(line)
            if m:
                base = slug(m.group(2))
                n = seen.get(base, 0)
                seen[base] = n + 1
                found.add(base if n == 0 else f"{base}-{n}")
        _anchor_cache[path] = found
    return _anchor_cache[path]


def check(md: Path) -> list[str]:
    problems = []
    # Links resolve relative to where the file is shown. A symlinked README is
    # rendered at the symlink's location, so resolve from there, not the target.
    base = md.parent
    text = md.read_text(encoding="utf-8", errors="replace")
    for lineno, line in enumerate(strip_code(text), 1):
        targets = LINK.findall(line) + IMAGE.findall(line) + HTML_SRC.findall(line)
        for target in targets:
            if re.match(r"^[a-z][a-z0-9+.-]*:", target, re.I):  # http:, mailto:, ...
                continue
            path_part, _, fragment = target.partition("#")
            dest = md if not path_part else (base / path_part)
            rel = md.relative_to(ROOT)
            if path_part and not dest.exists():
                problems.append(f"{rel}:{lineno}: missing target `{target}`")
                continue
            if fragment and dest.is_file() and dest.suffix == ".md":
                if fragment.lower() not in anchors(dest.resolve()):
                    problems.append(f"{rel}:{lineno}: no heading for `#{fragment}` in {dest.resolve().relative_to(ROOT)}")
    return problems


def main() -> int:
    problems = [p for md in tracked_markdown() for p in check(md)]
    for p in problems:
        print(p)
    print(f"{len(problems)} broken link(s)" if problems else "all relative links resolve")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
