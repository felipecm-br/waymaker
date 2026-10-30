# tmux pickers

Waymaker pickers for tmux, opened in a popup over the pane you are in. Each one gets that pane as
its origin (`MM_ORIGIN_PANE`) and its directory (`MM_ORIGIN_CWD`), so `Ctrl+V` types the pick into
it and `Ctrl+E` opens a file next to it.

| Picker | Preset | What it does |
| :--- | :--- | :--- |
| Sessions | `session-picker` | Switch tmux sessions with a live preview; `Tab` cycles the sources, `d` kills a session |
| Files | `workspace` | Browse the pane's directory, frecent paths and bookmarks with previews |
| Grep | `rg` | Live ripgrep in the pane's directory; `Enter` opens the match in `$EDITOR` |
| Scrollback | `tmux/scrollback` (via `scrollback.sh`) | Commands, paths, URLs and hashes from the pane's output, newest first; `Enter` copies |
| Keybindings | `tmux/keybindings` | Every key bound in this tmux server, read live; `Enter` presses it |

## Bindings

```tmux
# Sessions, files and grep: the popup runs the preset in the pane's directory
bind s display-popup -E -w 75% -h 60% -b rounded -d '#{pane_current_path}' \
  -e MM_ORIGIN_PANE='#{pane_id}' -e MM_ORIGIN_CWD='#{pane_current_path}' 'wm session'
bind e display-popup -E -w 85% -h 75% -b rounded -d '#{pane_current_path}' \
  -e MM_ORIGIN_PANE='#{pane_id}' -e MM_ORIGIN_CWD='#{pane_current_path}' 'wm -o workspace'
bind / display-popup -E -w 85% -h 75% -b rounded -d '#{pane_current_path}' \
  -e MM_ORIGIN_PANE='#{pane_id}' -e MM_ORIGIN_CWD='#{pane_current_path}' 'wm --no-read -o rg'

# Scrollback: the script captures the pane, then opens tmux/scrollback
bind y display-popup -E -w 85% -h 75% -b rounded -d '#{pane_current_path}' \
  -e MM_ORIGIN_PANE='#{pane_id}' -e MM_ORIGIN_CWD='#{pane_current_path}' \
  '~/.config/waymaker/presets/tmux/scrollback.sh'

# Keybindings: Tab cycles prefix, root, copy mode and all tables
bind ? display-popup -E -w 85% -h 75% -b rounded 'wm -o tmux/keybindings'
```

`prefix + ?` and `prefix + s` replace tmux's own `list-keys` and `choose-tree` bindings; pick
other keys to keep those.

## Requirements

- tmux 3.2 or newer (`display-popup -e`); `Enter` in the keybindings picker needs 3.4
  (`send-keys -K`).
- `python3` for the scrollback and keybindings pickers.
- `rg` and `bat` for grep (`bat` is optional: plain `cat` otherwise).
- A clipboard tool for copying: `wl-copy`, `xclip` or `pbcopy`, else a tmux buffer.

## Scripts

The presets call scripts installed next to them (`~/.config/waymaker/presets/tmux/`, `.../rg/`).
`install.sh` and `wm --download` install them; any missing one is also written there the first
time a bundled preset loads. Edit them freely: an existing file is never replaced.

- `open.sh <file[:line[:col]]> [cwd]`: opens a file in `$EDITOR` beside the origin pane (an open
  Neovim in that window gets it instead), or a URL in the browser. Used by `rg`, `workspace` and
  `tmux/scrollback`.
- `scrollback.sh`: captures the origin pane, extracts tokens into a private temp dir, runs
  `tmux/scrollback`, and copies the pick.
- `keybindings.sh`: the rows, preview and key replay for `tmux/keybindings`.
