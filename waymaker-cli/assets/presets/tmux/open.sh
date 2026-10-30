#!/bin/sh
# open.sh — open a `file[:line[:col]]` token from a picker, beside the pane it was opened from.
#
#   open.sh <token> [cwd]          Open the file in $EDITOR (default nvim) at line and column
#   open.sh --check <token> [cwd]  Print "file|line|col|exists" and do nothing (tests)
#
# Inside tmux with an origin pane ($MM_ORIGIN_PANE, else the last pane): a Neovim already open in
# that window gets the file (through its socket, else keystrokes) and focus; otherwise a split opens
# to the left of the origin pane, and the popup closes. Outside tmux the editor runs here. A token
# that is not a file but looks like a URL goes to the system opener (xdg-open, open).
set -u

check=0
if [ "${1:-}" = "--check" ]; then check=1; shift; fi
token="${1:-}"
cwd="${2:-}"

file="$token"; line=1; col=1
suffix=$(printf '%s' "$token" | grep -oE ':[0-9]+(:[0-9]+)?$' || true)
if [ -n "$suffix" ]; then
  file="${token%"$suffix"}"
  line=$(printf '%s' "$suffix" | cut -d: -f2)
  col=$(printf '%s' "$suffix" | cut -d: -f3)
  [ -n "$col" ] || col=1
fi
case "$file" in '~'*) file="$HOME${file#'~'}" ;; esac

exists=0
case "$file" in
  /*) [ -f "$file" ] && exists=1 ;;
  *)
    if [ -n "$cwd" ] && [ -f "$cwd/$file" ]; then
      exists=1; file="$cwd/$file"
    elif [ -f "$file" ]; then
      exists=1
    fi
    ;;
esac

if [ "$check" = 1 ]; then
  printf '%s|%s|%s|%s\n' "$file" "$line" "$col" "$exists"
  exit 0
fi

# One shell word: 'it'\''s' for it's.
quote() { printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"; }

if [ "$exists" != 1 ]; then
  url=$(printf '%s' "$token" | sed -E 's/^[[:space:]<"'\''([{]+//; s/[]})>"'\''[:space:]]+$//; s/[.,;:]+$//')
  if printf '%s' "$url" | grep -qE '^(https?://|www\.|localhost(:|/)|127\.0\.0\.1(:|/))'; then
    case "$url" in www.*) url="https://$url" ;; esac
    for opener in xdg-open open; do
      if command -v "$opener" >/dev/null 2>&1; then
        "$opener" "$url" >/dev/null 2>&1 &
        exit 0
      fi
    done
  fi
  [ -n "${TMUX:-}" ] && tmux display-message "open: not a file: $file" 2>/dev/null
  exit 0
fi

editor="${EDITOR:-nvim}"
# vim and Neovim take the column too; other editors (nano, micro, emacs) take +line.
case "${editor##*/}" in
  *vim*) jump="call cursor($line,$col)" ;;
  *) jump="$line" ;;
esac
if [ -z "${TMUX:-}" ]; then
  exec $editor +"$jump" "$file"
fi

origin="${MM_ORIGIN_PANE:-}"
[ -n "$origin" ] || origin=$(tmux display-message -p -t '{last}' '#{pane_id}' 2>/dev/null || true)
[ -n "$origin" ] || origin=$(tmux display-message -p '#{pane_id}' 2>/dev/null || true)
window=$(tmux display-message -t "$origin" -p '#{window_id}' 2>/dev/null || true)
[ -n "$cwd" ] || cwd=$(tmux display-message -t "$origin" -p '#{pane_current_path}' 2>/dev/null || pwd)

# A Neovim already open in the origin window takes the file.
nvim_pane=""; nvim_pid=""
panes=$(tmux list-panes -t "$window" -F '#{pane_id} #{pane_pid} #{pane_current_command}' 2>/dev/null || true)
while IFS=' ' read -r id pid cmd; do
  case "$cmd" in nvim | vim) nvim_pane=$id; nvim_pid=$pid; break ;; esac
done <<EOF
$panes
EOF

if [ -n "$nvim_pane" ]; then
  sock="${XDG_RUNTIME_DIR:-/tmp}/nvim.$nvim_pid.0"
  if [ -S "$sock" ] && command -v nvim >/dev/null 2>&1; then
    nvim --server "$sock" --remote-send "<C-\\><C-n>:e $(printf '%s' "$file" | sed 's/ /\\ /g')<CR>:call cursor($line,$col)<CR>" 2>/dev/null || true
  else
    tmux send-keys -t "$nvim_pane" Escape ":e $(printf '%s' "$file" | sed 's/ /\\ /g')" Enter ":call cursor($line,$col)" Enter 2>/dev/null || true
  fi
  tmux select-pane -t "$nvim_pane" 2>/dev/null || true
else
  pane=$(tmux split-window -d -h -b -l 62% -P -F '#{pane_id}' -t "$origin" -c "$cwd" \
    "$editor +$(quote "$jump") $(quote "$file")" 2>/dev/null || true)
  [ -n "$pane" ] && tmux select-pane -t "$pane" 2>/dev/null
fi

tmux display-popup -C 2>/dev/null || true
exit 0
