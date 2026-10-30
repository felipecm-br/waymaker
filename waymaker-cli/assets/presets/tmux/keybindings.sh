#!/bin/sh
# keybindings.sh — rows, preview and replay for the `tmux/keybindings` preset: every key bound in
# this tmux server, read live from `tmux list-keys`.
#
#   keybindings.sh --items <prefix|root|copy|all>   "key<TAB>table<TAB>note or command" rows
#   keybindings.sh --preview <table> <key>          the binding as tmux prints it, with its note
#   keybindings.sh --run <table> <key>              press the key in the client that opened the
#                                                   popup, after the popup closes (tmux 3.4+)
#
# Needs python3 to read the bindings (tmux escapes keys and quotes commands).
set -u

# Reads `tmux list-keys` once. `items <want>` prints the rows; `preview <table> <key>` prints that
# binding (tmux 3.7 cannot list a single key, so the whole table is filtered here).
bindings() { # <items|preview> <args...>
  tmux list-keys 2>/dev/null | python3 -c '
import subprocess, sys

mode, args = sys.argv[1], sys.argv[2:]

def head_tokens(line):
    """bind-key [flags] -T <table> <key> <command...>: tokens up to the key, and the rest."""
    tokens, i, n = [], 0, len(line)
    while i < n:
        while i < n and line[i] == " ":
            i += 1
        tok, quote = "", None
        while i < n and (quote or line[i] != " "):
            c = line[i]
            if quote:
                if c == quote:
                    quote = None
                elif c == "\\" and quote == "\"" and i + 1 < n:
                    i += 1
                    tok += line[i]
                else:
                    tok += c
            elif c in "\"\x27":
                quote = c
            elif c == "\\" and i + 1 < n:
                i += 1
                tok += line[i]
            else:
                tok += c
            i += 1
        tokens.append(tok)
        if "-T" in tokens and len(tokens) == tokens.index("-T") + 3:
            return tokens, line[i:].strip()
    return tokens, ""

notes = {}
def note_for(table, key):
    """The binding note (`bind -N`, tmux defaults), read once per table."""
    if table not in notes:
        # -a: tmux 3.7 lists no root-table notes without it
        out = subprocess.run(["tmux", "list-keys", "-N", "-a", "-P", "", "-T", table],
                             capture_output=True, text=True).stdout
        notes[table] = dict(l.strip().split(None, 1) for l in out.splitlines() if len(l.split(None, 1)) == 2)
    return notes[table].get(key, "")

order = {"prefix": 0, "root": 1}
rows = []
for line in sys.stdin:
    line = line.rstrip("\n")
    tokens, command = head_tokens(line)
    if "-T" not in tokens or len(tokens) < tokens.index("-T") + 3:
        continue
    t = tokens.index("-T")
    table, key = tokens[t + 1], tokens[t + 2]
    note = tokens[tokens.index("-N") + 1] if "-N" in tokens[:t] else note_for(table, key)
    rows.append((order.get(table, 2), table, key, note, command, line))

if mode == "preview":
    for _, table, key, note, _, line in rows:
        if [table, key] == args[:2]:
            if note:
                print(f"\033[1m{note}\033[0m\n")
            print(line.replace(" \\; ", "\n    \\; "))
    sys.exit(0)

want = args[0] if args else "prefix"
for _, table, key, note, command, _ in sorted(rows, key=lambda r: (r[0], r[1])):
    if want == "copy" and not table.startswith("copy-mode"):
        continue
    if want in ("prefix", "root") and table != want:
        continue
    print(f"{key}\t{table}\t{note or command}")
' "$@"
}

# One shell word: 'it'\''s' for it's.
quote() { printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"; }

case "${1:-}" in
  --items)
    bindings items "${2:-prefix}"
    ;;
  --preview)
    bindings preview "${2:-}" "${3:-}"
    ;;
  --run)
    table="${2:-}" key="${3:-}"
    client=$(tmux display-message -p '#{client_name}' 2>/dev/null)
    case "$table" in
      root) keys=$(quote "$key") ;;
      prefix) keys="$(quote "$(tmux show-options -gv prefix)") $(quote "$key")" ;;
      *)
        tmux display-message "keybindings: $key runs only in $table"
        exit 0
        ;;
    esac
    # The popup has to close first, or the key would go to it.
    # run-shell expands formats: a literal # (a key name) is written ##.
    cmd=$(printf 'sleep 0.15; tmux send-keys -K -c %s %s' "$(quote "$client")" "$keys" | sed 's/#/##/g')
    tmux run-shell -b "$cmd"
    ;;
  *)
    sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'
    exit 2
    ;;
esac
