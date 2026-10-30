#!/bin/sh
# scrollback.sh — pick a command, path, URL or hash from a tmux pane's scrollback and copy it.
#
# Run it in a popup over the pane (see tmux/README.md for the binding). It captures the origin
# pane ($MM_ORIGIN_PANE, else the last pane), extracts tokens newest first into a private temp
# dir ($WM_SCROLLBACK_DIR), and opens `wm -o tmux/scrollback`. Enter copies the pick (wl-copy,
# xclip, pbcopy, else a tmux buffer); the preset itself inserts (Ctrl+V) and opens (Ctrl+E,
# Ctrl+B). Needs python3 for the extraction.
#
#   scrollback.sh --extract <capture file> <out dir>   only write all/cmd/path/url/sha.txt (tests)
set -u

extract() { # <capture file> <out dir>
  python3 - "$1" "$2" <<'PYEOF'
import os, re, sys

src, out = sys.argv[1], sys.argv[2]
with open(src, encoding="utf-8", errors="ignore") as f:
    lines = [l.rstrip("\n") for l in f if l.strip()]
lines.reverse()  # newest output first

verbs = set("""sudo doas pacman yay paru apt apt-get dnf yum brew flatpak snap rm mv cp mkdir rmdir
touch ln chmod chown git gh cargo rustc just make systemctl journalctl docker podman kubectl curl
wget ssh scp rsync tar unzip gzip nvim vim bat cat rg grep find fd wm tmux kill pkill python
python3 node npm pnpm bun uv zig go""".split())

re_url = re.compile(r'https?://[^\s"\'<>]+|git@[^\s"\'<>]+|www\.[a-zA-Z0-9.-]+[^\s"\'<>]*')
re_path = re.compile(r'(?:~?/[A-Za-z0-9._~:/?#@!$&()*+,;=%-]+|\.\.?/[A-Za-z0-9._~:/?#@!$&()*+,;=%-]+)')
re_sha = re.compile(r'\b[0-9a-f]{7,40}\b|\b[0-9]{1,3}(?:\.[0-9]{1,3}){3}(?::[0-9]+)?\b')
re_prompt = re.compile(r'^[#$%>❯➜→]\s+(.+)$')
# A shell call an agent printed, e.g. Claude Code's "● Bash(cargo test)"
re_tool = re.compile(r'●\s*Bash\((.+?)\)(?:\s*\(ctrl\+o.*)?$')

lists = {k: [] for k in ("all", "cmd", "path", "url", "sha")}
seen = {k: set() for k in lists}

def add(kind, tok):
    for k in (kind, "all"):
        if tok not in seen[k]:
            seen[k].add(tok)
            lists[k].append(tok)

def clean(t):
    return re.sub(r'^[<"\'(\[{\]]+|[][>"\'})\s.,;:)]+$', "", t)

for line in lines:
    for u in re_url.findall(line):
        u = clean(u)
        if len(u) > 4:
            add("url", u)
    m = re_tool.search(line) or re_prompt.search(line)
    cmd = m.group(1).strip() if m else None
    if cmd is None and line.split() and line.split()[0] in verbs:
        cmd = line.strip()
    if cmd and len(cmd) >= 3 and not cmd.isdigit():
        add("cmd", re.sub(r"\s*\(ctrl\+o.*$", "", cmd).strip())
    for p in re_path.findall(re_url.sub("", line)):
        p = clean(p)
        if len(p) > 2:
            add("path", p)
    for s in re_sha.findall(line):
        s = clean(s)
        if len(s) >= 7:
            add("sha", s)

for k, items in lists.items():
    with open(os.path.join(out, k + ".txt"), "w", encoding="utf-8") as f:
        f.write("".join(i + "\n" for i in items))
PYEOF
}

if [ "${1:-}" = "--extract" ]; then
  extract "$2" "$3"
  exit $?
fi

[ -n "${TMUX:-}" ] || { echo "scrollback.sh: run it inside tmux" >&2; exit 1; }
command -v python3 >/dev/null 2>&1 || { tmux display-message "scrollback: python3 not found"; exit 1; }

origin="${MM_ORIGIN_PANE:-}"
[ -n "$origin" ] || origin=$(tmux display-message -p -t '{last}' '#{pane_id}' 2>/dev/null || true)
export MM_ORIGIN_PANE="$origin" WM_ORIGIN_PANE="$origin"
export MM_ORIGIN_CWD="${MM_ORIGIN_CWD:-$(tmux display-message -p -t "$origin" '#{pane_current_path}' 2>/dev/null)}"
export WM_ORIGIN_CWD="$MM_ORIGIN_CWD"

dir=$(mktemp -d "${XDG_RUNTIME_DIR:-${TMPDIR:-/tmp}}/waymaker-scrollback.XXXXXX") || exit 1
trap 'rm -rf "$dir"' EXIT
export WM_SCROLLBACK_DIR="$dir"

if ! tmux capture-pane -pJS - -t "$origin" 2>/dev/null | sed '/^$/d' >"$dir/src.txt"; then
  tmux display-message "scrollback: cannot capture pane $origin"
  exit 1
fi
extract "$dir/src.txt" "$dir"
if [ ! -s "$dir/all.txt" ]; then
  tmux display-message "scrollback: no commands, paths, URLs or hashes"
  exit 0
fi

chosen=$(wm -o tmux/scrollback) || exit 0
[ -n "$chosen" ] || exit 0

if command -v wl-copy >/dev/null 2>&1 && [ -n "${WAYLAND_DISPLAY:-}" ]; then
  printf '%s' "$chosen" | wl-copy
elif command -v xclip >/dev/null 2>&1 && [ -n "${DISPLAY:-}" ]; then
  printf '%s' "$chosen" | xclip -in -selection clipboard
elif command -v pbcopy >/dev/null 2>&1; then
  printf '%s' "$chosen" | pbcopy
else
  printf '%s' "$chosen" | tmux load-buffer -
fi
tmux display-message "copied: $chosen"
