# Features

Task-oriented examples for the features Waymaker adds on top of a plain fuzzy picker. Each section gives the CLI flag (if any), the `key=value` override, and the `config.toml` form; they are interchangeable. Override syntax is in `wm --doc options`, binds in `wm --doc binds`.

## Sorting input

Sort stdin lines alphabetically before they reach the picker. Applies to piped input, not to `start.command` output.

```bash
printf 'zebra\napple\nmango\n' | wm --sort
printf 'z\na\nb\n' | wm s.sort=true
```

```toml
[start]
sort = true
```

## Icons and symlink targets

`--icons` prepends a Nerd Font file-type icon (needs a Nerd Font). `--symlink-target` appends `→ <target>` to entries that are symlinks.

```bash
wm --icons
fd --strip-cwd-prefix | wm --icons --symlink-target
wm --symlink-target results.symlink.style.fg=Cyan
```

```toml
[results]
icons = true

[results.symlink]
active = true
style.fg = "DarkGray"
```

## Match count in the query bar

Moves the `matched/total` counter into the right edge of the query bar instead of its own row.

```bash
wm --status-inline
wm q.status_inline=true
```

```toml
[query]
status_inline = true
```

## Preview pane

### Title

The preview border shows the current item's name by default. Set a static or interpolated title per layout:

```bash
wm p.l.border.type=Plain p.l.title='File: {item}' --color preview-title:Yellow
```

```toml
[[preview.layout]]
side = "right"
percentage = 40
border.type = "Plain"
title = "File: {item}"
```

### Gap and drag-to-resize

The gap between the results and the preview (default 1 cell) is a drag handle: it highlights on hover, and dragging it resizes the preview. `0` removes the gap; dragging then uses the border edge.

```bash
wm p.l.gap=2
```

```toml
[[preview.layout]]
gap = 2
```

From the keyboard, `ctrl-]` expands the preview (`ExpandPreview(2)`); `ExpandPreview(n)` and `ShrinkPreview(n)` can be bound to any key.

### Images, video and PDF

`--media` renders media in the preview using the terminal's graphics protocol (Kitty, Sixel or iTerm2, detected at startup; half-blocks otherwise).

- Images (`png`, `jpg`, `gif`, `webp`, ...) are decoded in-process.
- Video thumbnails need `ffmpegthumbnailer`; PDF first pages need `pdftoppm`.

```bash
wm --media
wm p.media.active=true
```

```toml
[preview.media]
active = true
# protocol = "kitty"   # force one of: kitty, sixel, halfblocks, iterm2
```

## Selecting and marking rows

`tab` toggles the current row and moves down; `shift-backtab` (`ToggleUp`) toggles and moves up. Rebind `tab = "Toggle"` alone if you prefer `Down` not to follow; there is no separate setting.

Selected rows that are not under the cursor get their own styles:

| Key | Default | Styles |
| :--- | :--- | :--- |
| `results.selected_style` | bold | the row text |
| `results.selected_prefix_style` | cyan, bold | the prefix glyph |

```bash
wm results.selected_style.bg=Cyan results.selected_style.fg=Black
wm --color 'selected-fg:White,selected-bg:DarkGray,selected-prefix:Yellow'
```

### Yank and cut marks

`FmSetYankPaths(paths)` marks rows whose first column is in a newline-separated list with `results.yank_prefix_style` (default yellow, bold); `FmSetYankPaths()` clears the marks. The prefix glyph priority is yank, then selected, then default. The cut equivalents are `FmSetCutPaths` and `results.cut_prefix_style`.

```bash
wm b 'ctrl-y=FmSetYankPaths({+1})' b 'ctrl-shift-y=FmSetYankPaths()'
```

```toml
[results]
yank_prefix_style.fg = "Magenta"

[binds]
"ctrl-y" = ["Store({+1})", "FmSetYankPaths({MM_STORE})"]
"ctrl-shift-y" = "FmSetYankPaths()"
```

## Colours: `--color`

One fzf-style flag sets many colours at once: `--color key:value[,key:value...]`. It can be repeated; later values win. A value is a named colour (`Red`, `DarkGray`, `LightCyan`), hex (`#ff5f87`) or a 256-palette index (`200`).

| Key (aliases) | Colours |
| :--- | :--- |
| `fg`, `bg` | result text |
| `hl-fg`, `hl-bg` (`current-fg`, `current-bg`) | the cursor row |
| `border`, `label` (`title`) | the outer border and its title |
| `preview-border`, `preview-label` (`preview-title`, `preview-border-title`) | the preview pane |
| `list-border`, `list-label` (`list-title`) | the results pane |
| `input-border`, `input-label` (`input-title`) | the query bar |
| `header-border`, `header-label` (`header-title`) | the header |
| `group-header`, `inactive-group-header` | group headers (see below) |
| `nav` | the navigation-mode indicator |
| `selected-fg`, `selected-bg`, `selected-prefix`, `unselected-prefix` | selected and unselected rows |
| `yank`, `cut` | yank and cut marks |
| `symlink` | symlink targets |
| `spinner` | row spinners |
| `tier-separator` | the line between sort tiers |

```bash
# Catppuccin Mocha
wm --color 'fg:#cdd6f4,bg:#1e1e2e,hl-fg:#cba6f7,hl-bg:#313244,border:#6c7086,selected-prefix:#89b4fa,yank:#f38ba8,symlink:#a6e3a1,preview-border:#45475a'

wm --color 'border:Blue' --color 'hl-bg:DarkGray,selected-fg:White'
```

An unknown key or colour is skipped with a warning.

## Spinners and the unselected marker

A row whose text starts with `results.spinner_prefix` shows an animated spinner in place of its prefix (the marker itself is stripped). Named spinners: `dot` (default), `line`, `jump`, `pulse`, `points`, `meter`, `hamburger`, `ellipsis`, `globe`, `moon`, `monkey`, `arc`, `nerd`, `nerdarc`, `minidot`.

`results.unselected_prefix` (default two spaces) is the marker of rows that are not selected.

```bash
printf '@Building…\nReady item\n' | wm results.spinner_prefix='@' results.spinner=moon preview.show=false --color spinner:Yellow
wm results.unselected_prefix='-' --color unselected-prefix:DarkGray
```

```toml
[results]
spinner = "moon"
spinner_prefix = "@"
spinner_style.fg = "Yellow"
unselected_prefix = "-"
unselected_prefix_style.fg = "DarkGray"
```

## Group headers

`--group-prefix <prefix>` turns input lines starting with the prefix into bold, non-selectable headers; the cursor and multi-select skip them.

```bash
printf '# Languages\nPython\nRust\n# Frameworks\nDjango\nAxum\n' | wm --group-prefix '#'
```

## Navigation mode extras

With navigation mode on (`--nav`, or `[ui.nav] active = true`; see `wm --doc jump`):

- `--nav-hints` shows the navigation keys in the footer while the results pane has focus.
- `--parent-peek` adds a third pane on the left with the parent directory's contents.

## Config layering

Waymaker builds its config from these layers, each overriding the previous:

1. the built-in defaults;
2. `~/.config/waymaker/config.toml`, if present (only the keys you set change);
3. each `-o`/`--override` file or preset, left to right (a bare name such as `-o jump` is looked up in the presets directory, then in the presets built into the binary);
4. `key=value` overrides on the command line.

`--config <file>` replaces layers 1 and 2: the file is loaded as the complete config.

```bash
wm -o jump
wm -o ~/dotfiles/wm-work.toml -o ~/wm-dark.toml
```

At startup `wm` warns about any key a layer sets that has no effect (a misspelling, or an option in the wrong table), for example:

```
[WARN] ~/.config/waymaker/config.toml: unknown config key `start.command.separator` has no effect
```

## Tmux sessions

`wm session` opens a picker of running tmux sessions plus the ones defined in `~/.config/waymaker/session.toml` (a `~/.config/sesh/sesh.toml` is read too); `wm connect <name>` creates or attaches one, and `wm last` switches to the previous session.

```toml
# Every directory under ~/dev/github becomes a session that starts nvim
[[wildcard]]
pattern = "~/dev/github/**"
startup_command = "nvim"
windows = ["server"]

# An explicit session; it wins over a matching wildcard
[[session]]
name = "Downloads"
path = "~/Downloads"
startup_command = "wm -o jump"

# A reusable extra window, opened detached when the session is created
[[window]]
name = "server"
startup_script = "npx -y live-server --port=5500"   # `startup_command` also works
# path = "~/other/dir"                              # defaults to the session path
```

A window name without a matching `[[window]]` is skipped. If `session.toml` does not parse, `wm` prints the error with the file name and lists only the running sessions.

## The default file walker

With no piped input and no `start.command`, `wm` lists files itself with an in-process parallel walker (the `ignore` crate, as in ripgrep):

- hidden files are included; `.gitignore`, `.ignore`, `.git/info/exclude` and the global gitignore are respected; `.git/` itself is skipped;
- the listing is cached per directory in `~/.local/state/waymaker/dir_cache.redb` and revalidated by directory modification time, so a second run in the same directory starts from the cache;
- to include ignored files, set your own command, e.g. `[start.command] command = "fd --type f --hidden --no-ignore"`;
- `wm clean` removes entries for paths that no longer exist from the frecency and directory caches.

## Measuring performance

- `wm -f <query>` filters headlessly and prints the ranked matches, like `fzf -f`.
- `wm --benchmark-input <query>` replays typing the query one character at a time against the input on a test backend and prints frame-time percentiles.

```bash
seq 1 300000 | awk '{print "src/module_" $1 % 977 "/file_" $1 ".rs"}' | wm --benchmark-input mod97
```

Reproducible numbers and the comparison with fzf and skim are in [docs/BENCHMARKS.md](../../../docs/BENCHMARKS.md) (`just bench-filter`).

## Default keys worth knowing

| Key | Action |
| :--- | :--- |
| `esc` | `@unwind`: closes an overlay or prompt, then clears the query, then quits (exit 130) |
| `tab` / `shift-backtab` | toggle and move down / up |
| `ctrl-p` | `SwitchPreview`: cycle the preview layouts |
| `ctrl-g` | jump to the last item |
| `ctrl-]` | expand the preview |
| `F1`, `alt-h` | help |

Plain characters such as `?` and `*` are not bound while typing, so they reach the query. A plain character bound under `[binds]` would swallow it; bind it under `[ui.nav.binds]` instead, where it applies once focus has left the query (`wm` warns at startup about the former).
