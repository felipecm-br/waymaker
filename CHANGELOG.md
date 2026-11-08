## [Unreleased]

### ✨ Features

- Query history, like fzf's `--history`: accepted queries are saved (`query.history`, one file per preset under `~/.local/state/waymaker/history/` with the default `"auto"`; `history_size` keeps 1000), and `up` on the first row recalls the previous one. `down` or `ctrl-n` goes forward, ending at the text you had typed; editing the recalled query or moving the cursor goes back to normal list movement. New actions `HistoryPrev`, `HistoryNext`, `UpOrHistory` and `DownOrHistory`. `up` on the first row no longer wraps to the last item once there is history; `ctrl-k` still does
- `matcher.track = true` keeps the cursor on the same item while the query changes, like fzf's `--track`. The item is followed until matching finishes; moving the cursor by hand stops following it

### ⚡ Performance

- Selective queries use far less CPU: nucleo (`041465e`) counts unmatched items after sorting instead of having every worker thread bump one shared atomic per item. On the 2M-line corpus, `wm -f` with a narrow query takes 120 ms instead of 199 ms (fzf: 331 ms) and with no match 111 ms instead of 189 ms (fzf: 175 ms); CPU time drops from about 1.3 s to 0.35 s. Rescoring while typing takes the same path
- Less memory per item: `ColumnRanges` keeps one range inline and boxes two or more (24 → 16 bytes), and a group label is a thin shared pointer (`GroupLabel`, 16 → 8 bytes), so an item is 48 bytes instead of 64. On the 2M-line corpus, `wm -f` peak RSS drops from 357 to 321 MiB (narrow query) and 402 to 373 MiB (broad), with the same output and speed. Items with exactly two columns now allocate their ranges
- Faster input ingestion and headless filtering (`wm -f`): `for_each_record` validates each 256 KB read chunk up to its last delimiter in a single SIMD `std::str::from_utf8` pass (falling back to per-record lossy decoding only when a chunk contains invalid UTF-8), `map_reader_batched` batches 4,096 records per channel send (matching the 256 KB buffer), `SegmentedInjector` builds single-column ranges inline without calling the splitter closure, and `Column::haystack` uses a direct function pointer for column 0. On the 2M-line (113 MB) corpus, `wm -f` drops from 230 ms to 199 ms on narrow queries (faster than `fzf -f` at 202 ms), 576 ms to 424 ms on broad queries (`fzf -f`: 1,128 ms), and 228 ms to 189 ms when nothing matches

### 🐛 Fixed

- A line wider than 65,535 cells was laid out by its width modulo 65,536, so a 65,546-cell line (minified JS, a long log line) was cut to a 10-cell sliver; debug builds panicked on the overflow instead. Widths now saturate at 2,048 cells (`utils::cells`) wherever item, query or header text is measured
- Changing the query moves the cursor to the best match. It used to keep its row, so after moving down, typing and pressing `enter` could accept a worse match (with `apple`, `foobar`, `banana`: two `down`s, then `ba` and `enter` printed `foobar` instead of `banana`)
- `matcher.raw` is removed: it was documented as unimplemented and did nothing; a config that sets it now gets an unknown-key warning. `matcher.reverse` was also documented as unimplemented but works (fzf's `--tac`); the docs say so now
- `#[partial]` on a tuple or unit struct reports a compile error at the struct's fields instead of panicking inside the proc macro

### 🔄 Changed

- Library API: `State::should_quit` and `State::should_quit_nomatch` are replaced by `State::request_finish()` and `State::request_no_match()` (read back with `exit_request()`). Every finish now goes through one exit (`requested_exit`, then `PickerUI::finish`), which also saves the query to history
- The default layout gives the preview 60% of the width (list 40%, preview 60%), and the footer spans both panes (`footer.row_connection = "Full"`), so its hints are no longer cut to the list's width. Set `percentage = 40` in the first `[[preview.layout]]` for the old split
- The default picker has navigation mode: `esc` unwinds as before, but with nothing left to unwind it moves to the results instead of exiting, so exiting from a clean prompt takes `esc esc` (still code 130). In navigation mode, letters are keys: `?` help, `/` or `I` back to the query, `j`/`k`, `gt`/`gb`, `+`/`-` and `}`/`{` resize the preview, `'` preview jump, `o`/`O` open, `w`/`W` print, `c`/`C` copy. These give home-row paths to actions that were only on `alt-*` chords. The footer's help hint, which named `?` while `?` typed into the query, now reads `esc nav │ ? help`. Set `ui.nav.active = false` for the old single-`esc` exit
- `Unwind` with navigation mode active moves focus to the results as its last step in the filter, instead of exiting. This also applies to the `workspace`, `tmux/scrollback`, `pr` and `downloads` presets, which inherit `esc = @unwind`
- The bind-table snapshot test shows each table as the picker starts, with the navigation profile's defaults layered in (`start::layer_nav_profile_defaults`)
- The navigation-mode cursor indicator (`ui.nav.color`) defaults to cyan instead of yellow, keeping warm colours for state (yank, cut, bookmarks); `reference.config.toml` listed the default as `Darkgray`. Set `ui.nav.color = "yellow"` to keep the old look
- `clippy::too_many_arguments` is enabled again for the workspace (it was allowed wholesale until the large files were split). The 37 functions still over 7 parameters carry an explicit `#[allow]`, so a new one has to be marked on purpose. The results renderer builds each row's `RowCtx` in one place (`row_ctx`) instead of three copies, and its helpers take `RowsFrame`/`RowCtx` instead of up to 21 loose parameters; one `make_table` pass is a `TablePass` (frame, results, rows, selection, columns) handed to the row helpers, which took 11–13 parameters
- Internal modular decomposition with no behaviour change (verified by `scripts/verify_move.py`): split `nucleo/worker.rs` into `nucleo/worker/{mod,sort,frizbee,cell,tests}.rs`, `config.rs` into `config/{mod,nav,results,tests}.rs`, `ui/preview.rs`, `utils/markdown.rs` and `utils/mermaid.rs` into `mod.rs` + `tests.rs`, extracted `ui/results/styler.rs`, and decomposed `Results::render_rows`, `Previewer::with_config`, `start::start` and `waymaker-partial-macros` so every file in the workspace is `< 2,000` lines and every function across all 4 crates is `< 200` lines

## [0.9.0] - 2026-11-07

### ✨ Features

- The keys of a fullscreen or diagram preview are configurable: `[ui.nav.preview_binds]` overrides them key by key (`"J" = "PreviewDown(30)"`); unlisted keys keep their defaults
- `[ui.nav.binds]` keys are matched as keys rather than strings, so `shift-j` and `J`, or `Esc` and `esc`, name the same key
- The navigation-mode keys that were built in (outside a fullscreen preview `J`/`K` scroll it, `u`/`d` by half a page, `s`, `+`/`=`, `-`, `0`, `]`/`n` and `[`/`N` drive diagrams, `,` opens the sort menu, `gg`/`gt`/`gb`) are default binds: `[ui.nav.binds]` can rebind each one. A bind replaces only its own key; binding `u` used to disable the `ctrl-u` default too
- Any two-key sequence in `[ui.nav.binds]` works (`zz = "Pos(0)"`), not only the `g` ones. A startup warning names a sequence that can never fire: one whose first key is also bound alone, or one longer than two keys
- In navigation mode, help lists the navigation keys and the preview keys in effect under their own headings

### 🐛 Bug Fixes

- `,` in navigation mode never showed the sort menu: it opened and closed again at once
- Navigation-mode keys typed in quick succession (or pasted) could be read against the focus or preview state from before the previous key took effect; each key now waits for the previous one to be applied
- Previewing a Markdown file whose Mermaid `classDiagram` has non-ASCII names (`Usuário`) crashed wm. Fixed by updating `mermaid-text` to 0.58
- A bind trigger that is a literal control character (a newline or tab inside the TOML key) or empty was accepted as a bind that could never fire; it is now a config error. The key chord `a-@` was saved back as the semantic trigger `@-a`
- With `results.wrap` on, moving up onto a first row that was cut off left it cut, showing only its last line; it is now drawn whole
- A `[binds]` key bound to a built-in file-manager alias (`@create`, `@rename`, `@delete`, `@zip`, `@unzip`, `@yank`, `@cut`, `@undo`, `@redo`, ...) did nothing: the bind was dropped at startup. The default `F2` (`@rename`) now renames
- The `jump` preset showed `a Add`, `r Rename`, `d Trash` and `z/Z Zip` hints but bound none of them; the keys now do what the hints say
- In `jump` the file-manager box (create, rename, trash confirmation, zip) was never drawn, so the prompt and the typed name were invisible
- Rename and zip put the caret before the prefilled name, so typing could only add a prefix; the caret now starts at the end. `ctrl-u` in the box clears the text, and a second press closes it
- `ctrl-r` (redo) in `jump` reloaded the list instead of redoing while the "Undone ..." message was up, which is always right after an undo
- `PreviewHScroll` never scrolled a text preview; it now scrolls unwrapped text
- `Prompt` and `Confirm` lost the typed text: `{input}` in their command was filled in when the box opened (empty). It is now filled in on Enter, shell-quoted
- `SetHeader` / `PushHeader` and the footer equivalents showed only at the next key press
- Jumping to the end of a preview (`PreviewJump`, and tail mode) stopped one line short when the title sits on the first line
- With `previewer.delay_clear` (the default) a preview command showed nothing until it had printed 500 lines or exited, so slow and streaming commands (`tail -f`, a build) looked blank; output staged when the command goes quiet for 200 ms is now shown
- A command preview with invalid UTF-8 froze at the bad byte until the command exited, and `previewer.try_lossy` never applied; bad bytes now show as U+FFFD with `try_lossy` and are dropped otherwise

### 🧪 Testing

- Fuzz targets (`fuzz/`, cargo-fuzz) for bind triggers, actions, the query, status templates, and the Markdown and Mermaid previews; CI runs each on every push. They found the trigger and `mermaid-text` bugs above
- Golden frames now count toward coverage (each `wm` exits normally), plus new frames for clipped and stacked rows, tiers, symlinks, mouse input, every preview action and many picker actions; pty tests for the file manager on real files. Line coverage 71% -> 76%; the CI floor is 72%

## [0.8.0] - 2026-11-05

### ✨ Added

- A second spinner marker: rows with `results.slow_spinner_prefix` animate with `results.slow_spinner` (default `hexfill`, a new Nerd Font spinner that fills and empties a honeycomb cell every 2.5 s), so one list can show fast and slow busy rows side by side

## [0.7.2] - 2026-11-05

### 🔄 Changed

- `ctrl-c` exits at once (new action `Abort`, exit code 130), like fzf, even with a message box open or items selected or marked. `Quit` keeps its step-by-step behaviour (close the action box, then clear selections, then yank and cut marks), so `esc` and `q` in the `jump` preset still unwind one step per press. Configs that bind `"ctrl-c" = "Quit"` keep the old behaviour; change them to `"Abort"`
- With an empty query, items keep their input order, like fzf; they used to be listed shortest first. Typing a query sorts by match quality as before

## [0.7.1] - 2026-11-05

### 🐛 Bug Fixes

- Navigation-mode key hints drop the brackets and the six key colours: keys are cyan and bold, actions darkgray (as in the footer). The `jump` hint says `m Bookmark` (was `[*]`), and the filter-mode hint `C-u` now reads `Clear` (it clears the query since 0.7.0; ancestors are nav-mode `U`)
- A footer hint bar too wide for its pane was cut mid-word with a dangling separator (`ctrl-p previ`, `tab mark │`); it now drops whole hints from the end, so it always ends on a complete key and label
- The `jump` and `pr` presets no longer draw the cursor row as an opaque black-on-darkgray bar; like the default config it is bold on the terminal background
- A query made only of exclusions (`!apple`, `!a !b`) matched nothing; like fzf it now lists every item without those terms, in `wm -f` and in the picker (nucleo fork `b6b254c`, the upstream fix)
- `wm.log` is appended to instead of rewritten at every start (and starts over past 5 MiB): a `wm` run by `wm` (a preview, `wm list`) used to wipe the running picker's log
- With a Markdown file (or, with `--media`, an image) under the cursor, `wm` redrew the screen about 30 times a second while idle; the preview now renders once

### 🧪 Testing

- Golden frames: full styled-screen snapshots of the real `wm` in a pty (filter, scrolling, multi-select, no match, narrow terminal, Markdown preview, `jump`)
- Key-contract tests for navigation mode (`l`/`h`, yank/paste/undo, source cycling) and a snapshot of every bind of the default config and all presets
- CI `regress` job: the previous and the new binary must print identical `wm -f` output, use at most 10% more memory and not be much slower

### 🔄 Changed

- Internal refactors with no behaviour change, each checked line by line against the old code: `start` (1,250 → 455 lines), `handle_action` (1,108 → 581), `Previewer::run` (858 → 145), `draw_frame` (741 → 387), `handle_fm` (670 → 211), `handle_frecency_cli` (555 → 195)


## [0.7.0] - 2026-11-04

### 🚀 Features

- `wm --doc features`: a task-oriented guide (icons, previews, media, `--color` keys, spinners, group headers, tmux sessions, config layering, the file walker), replacing `USAGE.md`, whose config snippets no longer parsed (`results.symlink_target`, `preview.media = true`, `[matcher] frecency = true`)
- Built-in `wm preview-file <path>` subcommand providing fast, portable, shell-free previewing for directories (tree with Nerd Font icons), markdown, mermaid diagrams, images (`chafa`/`img2txt`/metadata), and syntax-highlighted or numbered code/text files across Linux, macOS, and Windows
- `@unwind` action (`Action::Unwind(130)`): unrolls UI state in cascade (overlay -> prompt/live input -> action box -> sort menu -> help/preview -> selections -> query -> quit 130) before exiting, eliminating accidental exits while editing queries
- Zero-background focus & lateral indicator: `current_style` defaults to clean terminal background (`bg = Reset`) with `BOLD`, paired with a cyan lateral indicator bar (`current_nav_bar = Plain`, `current_nav_bar_style = cyan`), eliminating opaque black bars across dark/light terminals
- 5-item default footer hint bar (`enter accept │ tab mark │ ctrl-p preview │ ctrl-/ layout │ esc ? help`) in cyan and darkgray
- Biomechanical home-row ergonomics: `ctrl-g` jumps to the last item (`Pos(-1)`), `ctrl-]` expands the preview (`ExpandPreview(2)`), and `ctrl-u` clears the query (`Cancel`) in the default config and the `jump`/`workspace` presets (their ancestor jump moved to nav-mode `U`)

### ⚡ Performance

- Peak memory for large inputs drops by a third (2M lines: 641 → 430 MiB, and about half at 1M short lines). nucleo initialized a flag in every entry of each item bucket, including the bucket twice the size that it allocates in advance, so that whole bucket was resident while still empty. The flags now sit in a small header (nucleo now comes from the `felipecm-br/nucleo` fork, branch `waymaker`)
- Each input line's text is stored once instead of twice: plain lines are read straight into an `Arc<str>` that the item and the matcher's haystack share (`Utf32String::Shared` in the fork). 2M lines: 430 → 345 MiB with no match, 490 → 398 MiB on a broad query; same output and speed. Lines with ANSI parsing, non-ASCII text, or a column that is only part of the line still get their own matcher copy
- `diagram-images` cargo feature (on by default) gates the Mermaid/SVG rasterizer (`mermaid-rs-renderer`, `resvg`, `usvg`). `cargo build --release -p waymaker-cli --no-default-features -F frizbee` builds a 15.3 MB `wm` instead of 18.5 MB; diagrams then preview through the Unicode text renderer
- Single-column fast path in ingest injector bypassing column loops when a single unformatted column is parsed
- Non-spinning background match wait via matcher notification and `tokio::select!`
- Shared in-process redb database instances across threads, eliminating lock contention backoff delays
- Box `Text<'static>` in the item payload enum (`Either<Box<str>, Box<Text<'static>>>`), shrinking each item from 96 to 64 bytes (−32 bytes per item when `ansi = false`) and reducing 2M-line RSS from 770 MiB to 641 MiB

### 🐛 Bug Fixes

- Windows default config caught up with the Unix one: `ctrl-p` → `SwitchPreview`, `ExpandPreview(2)`/`ShrinkPreview(2)`, `Delete` → `@rm`, `@prev`, Markdown and media previews, rounded borders, and the built-in file walker instead of a PowerShell `fd`/`Get-ChildItem` command. It is now generated at build time from `config.toml` plus `assets/win.overrides.toml` (only the shell-specific `@look`, `@open`, `@open_2` and `@copy*` binds), so it cannot fall behind again: an override for a key `config.toml` no longer has fails the build
- Debug builds no longer replace your config with `dev.toml`: they read `config.toml` like a release build. `WM_DEV=1` (any build) switches to `<config dir>/dev.toml`, rewritten from the shipped `assets/dev.toml` on every start, and keeps the screen on exit
- `--color` warnings (unknown key, unparsable colour, missing `:`) went to the log, hidden at the default verbosity, so a typo such as `--color selected:Red` did nothing silently; they now print at startup
- A `session.toml` (or `sesh.toml`) that failed to parse made every configured session disappear from `wm session` with no message; the parse error is now printed with the file path
- Several presets set `[start]` options under `[start.command]`, where they were silently ignored. `switcher` (Linux and macOS), `dict/en` and `docker/compose` split NUL-terminated multi-line records into one item per line, so the switcher listed titles, app names and PIDs as separate entries; `git/status`, `docker/compose`, `ai/llm` and `win.rg` printed the whole line on Enter instead of the intended column; `ai/pi_sessions` ran outside `~/.pi/agent/sessions`. The keys now live in `[start]` (`input_separator`, `output_template`, `directory`, `trim`)
- `cmd = …` under `[start.command]` was ignored in TOML (it only worked as a CLI override alias), so a preset that set only `cmd` fell back to the directory walker. `cmd` now works in TOML too. `separator` there was parsed but never used; it is removed from the default config, `rg` and the dev playground configs, and from the options docs
- A test now parses every shipped config and preset with `serde_ignored` and fails on any key the schema skips
- `*` and `?` type into the query again: the shipped config bound them to `@bookmark` and `Help` while the query had focus, so a query could never contain them and pressing `*` bookmarked the highlighted line. The default config and the `jump`/`workspace` filter binds no longer bind plain characters; Help stays on `F1` and `alt-h`
- `wm` warns at startup when `[binds]` binds a plain character (`*`, `?`, `a`, space), which would swallow that character while typing

- `CycleSort` did nothing in release builds (it only worked with the `experimental` feature); it now cycles the sort stability in every build
- `HistoryUp` and `HistoryDown` were never implemented and silently did nothing when bound; they are removed, so a bind to them now fails with `Unknown action`
- Input records with invalid UTF-8 bytes no longer abort `wm -f` or get silently dropped in interactive mode; invalid bytes are replaced with `U+FFFD` (matching fzf) while reading with a shared `memchr` record splitter

### 🔄 Changed

- `wm` warns at startup about config keys that have no effect (`<file>: unknown config key `start.command.separator` has no effect`), for the user config, `--config`, `--override`/`-o` presets, their `source` files and path-rule presets. The config still loads; keys inside `[matcher.sort]`, `[matcher.frecency]` and, with `--config`, `[start.command]` are not checked
- In nav mode (`jump`, `workspace`), `m` toggles the bookmark on the focused path; `*` still works as an alias
- `esc` exits with 130 (like `ctrl-c` and fzf) instead of 1, so a script can tell "cancelled" from "nothing matched" (still 1). Bind `"esc" = "Quit(1)"` to keep the old code

## [0.6.0] - 2026-11-02

### 🚀 Features

- `SetStyledFooter(<template>)` replaces the footer with a style template, parsed like `[footer] content` (`{cyan,bold:Key} {darkgray:Label}`), so a `Transform` bind can show state next to the key that changes it, such as `Ctrl+L` stepping an effort level

### 🐛 Bug Fixes

- `install.sh` installed a downloaded binary without checking it against the `.sha256` published next to it; it now refuses a mismatch or a missing checksum (`WM_SKIP_CHECKSUM=1` overrides), and `WM_VERIFY_ATTESTATION=1` also checks the build provenance

### 🔒 Security

- Release archives carry signed build provenance (`gh attestation verify <archive> --repo felipecm-br/waymaker`)

## [0.5.0] - 2026-10-30

### 🚀 Features

- Add the `tmux/scrollback` (commands, paths, URLs and hashes from a pane's scrollback) and `tmux/keybindings` (every bound key, read live; `Enter` presses it) presets, with tmux popup bindings in `presets/tmux/README.md`
- Bring the `session-picker`, `workspace` and `rg` presets up to the author's current versions: `Key Label │` footers, a nav bar marker, mouse support and full-width rows
- Write the scripts bundled presets call (`tmux/open.sh`, `rg/toggle-*.sh`, ...) next to them when missing, so the built-in presets work before `install.sh` or `wm --download` runs

### 🐛 Bug Fixes

- `{key}` (and `{query}`, `{q}`, `{input}`, `{keys}`) never reached a column of that name, because the built-in values win; the names are now documented as reserved and Waymaker warns at startup when a column uses one
- `rg` and `workspace` called scripts from the author's dotfiles (`~/.config/tmux/scrollback-open.sh`, `~/.config/waymaker/scripts/rg-toggle-*.sh`): `e`/`o`/`Ctrl+E` and the `Ctrl+S`/`Ctrl+W` search toggles failed everywhere else. They now use the bundled `tmux/open.sh` (`$EDITOR` beside the origin pane, URLs in the browser) and `rg/toggle-*.sh`
- `session-picker` bound `s`/`Ctrl+S` to a `__SWITCH_WINDOW__` marker nothing handled; the keys are gone
- `install.sh` copied only the `.toml` files of preset folders, so `switcher/`, `docker/` and other presets that call their own scripts were installed without them
- Update `pulldown-cmark` to 0.13.6 for RUSTSEC-2026-0414 (exponential event growth with `ENABLE_WIKILINKS`)

## [0.4.0] - 2026-10-28

### 🚀 Features

- Add an outer-frame border gradient (`WM_BORDER_GRADIENT`)
- Support sesh-compatible extra windows via `[[window]]` and `windows = [...]` in sessions
- Embed the default presets in the binary as a fallback when none are installed locally
- Keep highlights visible under `NO_COLOR`

### 🐛 Bug Fixes

- Exit 1 when nothing matches; make `tab` select consecutive items; stop half-page moves at the ends of the list
- Allow bind overrides for keys containing `=`; display mode-scoped triggers as `mode^^kind`
- Stop status template parsing from panicking on non-ASCII text and adjacent groups; don't underflow when the terminal size reads as 0x0
- Report preset-download I/O errors instead of panicking, download presets from the waymaker repository and explain a missing preset
- Set the guessed `EDITOR` instead of overwriting `PAGER`; stop mutating the environment to track the target item

### ⚡ Performance & Refactoring

- Cut headless filter time by 40-50%
- Split `render` into focused modules and remove the unused file-manager overlays

### 🔒 CI & Security

- Update crates with RustSec advisories, add `cargo-deny`, Dependabot and coverage reporting, pin GitHub Actions to commit SHAs and Rust to 1.99.1
- Drive the real `wm` binary through a pseudo-terminal in tests and add property tests for the input parsers

## [0.3.2] - 2026-10-18

### 🧹 Storage & Worktree Hygiene

- Enforce post-release `cargo clean` and worktree pruning in the release workflow (`.agents/skills/release/SKILL.md`) to prevent unbounded multi-gigabyte `target/` accumulation across worktrees
- Document storage and worktree hygiene guidelines (`dev-gc --dry-run`, `cargo clean-all`, `awt rm`) in `AGENTS.md`

## [0.3.1] - 2026-10-17

### 🚀 Features

- Implement high-performance LiveInput passthrough mode
- Support periodic preview polling and live output streaming via `poll_ms`
- Dynamically style preview border with live input green when active

### 🐛 Bug Fixes

- Safely quote tokens and isolate semicolons for shell and tmux execution in live input
- Prevent double-counting separator height in effective footer layout
- Remove redundant async `Action::Pos` dispatch on reload cursor restoration
- Resolve space bar forwarding, preview tail flicker, and terminal query leaks in live input
- Eliminate flickering and fix tail tracking during live input streaming
- Propagate live input preview change events and force rerun on passthrough
- Resolve blank preview, event loop feedback, and first chunk swap

### 🚀 Features

- Add `Action::PromptInput(<template>)` interactive prompt input mode for modal prompt dispatch with `{input}` / `{=input}` templating and `WM_INPUT` / `MM_INPUT` environment variables
- Add `--benchmark-input <QUERY>` CLI flag to measure and assert continuous streaming keystroke frametimes against the 60 FPS (< 16.67ms) SLA on Ratatui's test backend

### ⚡ Performance

- Implement zero-copy `ColumnRanges` (`ArrayVec<[(u32, u32); 2]>`), completely eliminating 100,000 heap `Box<[(u32, u32)]>` allocations per search in 1-column and 2-column pickers
- Implement parallel multi-injector directory walker (`spawn_walk_parallel`) streaming items directly from worker threads into Nucleo, bypassing the single-threaded `mpsc::channel` funnel
- Stream data with zero intermediate heap allocations via lazy `ExactSizeIterator` batching in `IndexedInjector`, `SegmentedInjector`, and `AnsiInjector`
- Accelerate large batch ANSI parsing and trimming using Rayon parallel chunks for batches $\ge 256$ items
- Bypass background system font directory scanning (`warmup_font_db`) during headless filtering (`wm -f`) and benchmark runs (`wm --benchmark-input`), eliminating background disk I/O and CPU contention

## [0.2.1] - 2026-10-08

### 🐛 Bug Fixes

- Resolve `SwitchPreview` short-circuit, preview layout toggle, and shift-? key cross-matching
- Allow custom `additional_commands` on mode index 1 without intercepting frecency redb
- Dispatch single 'g' focus binding without buffering pending vim sequence

## [0.2.0] - 2026-10-03

### 🚀 Features

- Add dynamic `inactive_group_header_style` for muted group header styling
- Support cyclic `Tab` / `Shift+Tab` multi-source reloading in session-picker
- Curate generic production presets and add `_smart_tab` shell integration
- Add `zsh-vi-mode` vicmd mode hooks to shell integration
- Mount local build in Docker and add `test-docker-interactive` recipe

### 🐛 Bug Fixes

- Remove extraneous leading space in inline spinner replacement
- Properly initialize bottom offset and allow scrolling in preview tail mode
- Clarify `Ctrl+S` and `Ctrl+D` footer action bindings in session-picker
- Sync nucleo match ticks on reload and configure nav binds for session picker
- Update session picker on first kill and reset child directory cursor to top
- Preserve controlling terminal for `/dev/tty` using `sudo` in docker runner
- Automatically export `INSTALL_DIR` to `PATH` in shell rc installer
- Align release asset naming and add docker isolation test

### 🚜 Refactor

- Standardize presets and rewrite `reference.config.toml` in canonical English
- Remove obsolete documentation, plan, and Portuguese draft docs
- Rewrite README as state-of-the-art TUI documentation decoupled from dotfiles

## [0.1.1] - 2026-09-25

### 🐛 Bug Fixes

- Align preview drag divider handle and hover shadow directly over the vertical border, matching the biomechanical ergonomics of lazygitrs

### 🧪 Testing

- Substantially expand unit test coverage across `waymaker-lib` and `waymaker-cli` from 272 to 320 tests, covering `selector`, `color`, `clap`, `action`, `paths`, and matchmaker core logic

### 📚 Documentation & Governance

- Introduce Release Policy & Workflow in `AGENTS.md` and define canonical `release` skill (`.agents/skills/release/SKILL.md`) following the hybrid architecture (manual on-demand trigger + 100% automated CI execution)

## [0.1.0] - 2026-09-24

### 🚀 Features

- Add native Tmux workspace and session engine with `sesh` drop-in compatibility (`session-picker` preset, `session.toml`, `sesh.toml`)
- Fast-path native session commands and support combined flags
- Support horizontal separator bar with preview junction
- Add `current_nav_bar` and `current_nav_bar_style` for independent focused navbar cell styling
- Render full solid block (`█`) for `Thick` navbar border and eliminate partial half-height cuts across all rows
- Add `invert_current_icon` and `uncolor_current_icon` options for fine-grained icon highlights on cursor focus
- Attach interactive child process streams (`stdin`, `stdout`, `stderr`) to `/dev/tty` for flawless editor execution (`nvim`, `$EDITOR`) in subshells
- Record frecency ranking automatically for all opened files/directories when invoking `Execute` or `Become`

### 🚜 Refactor

- Nest prefix-sharing configuration fields monorepo-wide into structured sub-tables:
  - `[ui.nav]`: Navigation mode settings (`active`, `bar`, `blink`, `blink_rate`, `bold`, `color`, `marker`, `prompt`, `notify`, `passthrough`, `basic`, `focus_on_start`, `hints`, `hints_columns`, `binds`).
  - `[preview.media]`: Media preview settings (`active`, `protocol`, `size`, `zoom`, `fit`).
  - `[preview.diagrams]`: Diagram preview settings (`active`, `inline`, `theme`, `background`).
  - `[query.filter]`: Filter mode settings (`prompt`, `prompt_style`, `style`, `underline`, `underline_style`).
  - `[query.local]`, `[query.frecency]`, `[query.bookmarks]`: Query mode-specific prompt & underline styling (`prompt`, `prompt_style`, `underline_style`).
  - `[results.symlink]`: Symlink target display & styling (`active`, `style`).
  - `[results.tier]`: Directory-first tier separator settings (`separator`, `style`).
  - `[results.bookmark]`: Bookmark item icons & styling (`icon`, `file_icon`, `folder_icon`, `icon_style`, `file_icon_style`, `folder_icon_style`).
  - `[results.frecency]`: Frecency item icons & styling (`icon`, `folder_icon`, `icon_style`, `folder_icon_style`).
  - `[matcher.sort]` / `[worker.sort]`: Sorting thresholds and caps (`threshold`, `cap`).
  - `[matcher.frecency]` / `[worker.frecency]`: Frecency scoring settings (`active`, `weight`, `half_life_days`).
  - `[start.command]`: Command execution settings (`default`/`command`, `additional`/`additional_commands`).
- Migrate all presets, asset configurations, documentation, and user dotfile presets to nested TOML keys.

### 📚 Documentation

- Document new results styling options, TTY execution handling, and frecency tracking across all markdown guides
- Update configuration reference and documentation guides for nested sub-table structure

## [0.0.42] - 2026-05-29

### 🚀 Features

- Demo mode on rg
- Copy action using osc52
- Emoji + unicode dict
- Copy action (sync)

## [0.0.41] - 2026-05-28

### 🐛 Bug Fixes

- column sizing regression

## [0.0.39] - 2026-05-28

### 🚀 Features

- Doc revisions
- Dictionary preset
- Improve mode binding resolution and display
- Results.min_wrap_width -> min_width

### ⚙️ Miscellaneous Tasks

- Remove old ps1 installer

## [0.0.38] - 2026-05-27

### 🐛 Bug Fixes

- Rendering bug

### 💼 Other

- cursor_next/prev now returns whether they caused a wraparound

### ⚙️ Miscellaneous Tasks

- Update release

## [0.0.37] - 2026-05-27

### ⚙️ Miscellaneous Tasks

- Update packaging naming

## [0.0.36] - 2026-05-27

### 🚀 Features

- Update win config

### ⚙️ Miscellaneous Tasks

- Setup cargo dist

## [0.0.35] - 2026-05-26

### 🚀 Features

- Adjust default colors for better visibility
- ExecuteAsync

### ⚙️ Miscellaneous Tasks

- Add gif

## [0.0.34] - 2026-05-26

### 🚀 Features

- Defaults for PAGER and EDITOR

### 🐛 Bug Fixes

- Bugfixes and improvements

## [0.0.33] - 2026-05-25

### 🚀 Features

- Refactor directory to use EnvValue, docs update
- Preview resizing (drag, expand, shrink)

### ⚙️ Miscellaneous Tasks

- Update screenshots

## [0.0.32] - 2026-05-25

### 🚀 Features

- Prefix_styles

### 🐛 Bug Fixes

- Preset tweaks
- fix BecomeSilent to correctly exit

## [0.0.32] - 2026-05-25

### 🚀 Features

- results.prefix_styles

### 🐛 Bug Fixes

- Preset tweaks
- fix BecomeSilent incorrect exit

## [0.0.31] - 2026-05-25

### 🚀 Features

- Doc update

### ⚙️ Miscellaneous Tasks

- Dep updates, preset tweaks

## [0.0.30] - 2026-05-25

### 🚀 Features

- Git-grep preset + more previewer configuration settings
- Support disabling of preview layout
- Git presets
- Preset downloading
- Better no_match/empty semantics
- Ssh, ps presets
- PushHeader, command.directory, improved ssh presets
- Env config, various improvements
- Improvements to preview tail, presets, and others
- Doc updates
- Trigger mode
- Bug fixes
- Doc updates, read MM_INDEX from env
- Config inheritance
- RunPreview action
- Improve help display
- Source envs for initial command
- Kube presets

### 🐛 Bug Fixes

- Various bug fixes and improvements
- Bugfixes
- Incorrect COLUMNS for preview
- Doc update
- More flexible download
- Click indexing
- Execute display + improved presets

### ⚙️ Miscellaneous Tasks

- Update ci
## [0.0.29] - 2026-05-18
## [0.0.29] - 2026-05-18

### 🚀 Features

- Clickable headers and status
- optimize mem footprint
- named overrides (`-o`)
- configurable previewer shell executor

## [0.0.27] - 2026-05-17

### 🚀 Features

- Smoother preview switching

## [0.0.26] - 2026-05-17

### 🚀 Features

- Preliminary windows support windows

### 🐛 Bug Fixes

- Correctly clear screen on exit for all layouts
- Autoscroll bugs
- Autoscroll oob panic

## [0.0.25] - 2026-05-15

### 🚀 Features

- Enable experimental features (i.e. sort controls) for github build
- Optional colors

### 🐛 Bug Fixes

- Don't use wildcard versions

### ⚙️ Miscellaneous Tasks

- Add justfile task runner

## [0.0.24] - 2026-03-24

### 🚀 Features

- Add command_input_separator specifically for splitting only non-piped input

### 🐛 Bug Fixes

- fix broken default config.toml splitting on null separator even for piped input

## [0.0.23] - 2026-03-22

### 🚀 Features

- Max_height
- Refactor styles to StyleSetting
- mimalloc

### 🐛 Bug Fixes

- Previewchange now emits correctly
- fixed broken config.toml

## [0.0.22] - 2026-03-18

- deps update
- refactor

## [0.0.21] - 2026-03-17

### 🚀 Features

- --override (layered configs)
- finish implementing header wrapping config option

## [0.0.20] - 2026-03-15

### 🐛 Bug Fixes

- Invisible columns

## [0.0.19] - 2026-03-14

### 🚀 Features

- reverted template braces [] -> {} for fzf compatibility
- improved docs

## [0.0.18] - 2026-03-13

### 🚀 Features

- moved autoscroll options to results.autoscroll
- autoscroll.end (--keep-right) in fzf.

## [0.0.17] - 2026-03-13

### 🚀 Features

- Rename alias prefix :: -> @.
- Update configs to work with newer syntax.

## [0.0.16] - 2026-03-13

### 🚀 Features

- PageUp -> HalfPageUp for more flexibility

### 🐛 Bug Fixes

- Fix panic in PreviewLayout
- Fix bug causing width sizer to not run

## [0.0.14] - 2026-03-11

- Regex capture groups
- `--doc` to display comprehensive help
- Improve (finalized) width sizing and autoscrolling
- Improved rg example and column switching

## [0.0.13] - 2026-03-11

### 🚀 Features

- Span template shrinkers, doc updates
- Reworked semantic triggers now behave like action aliases
- new actions: Transform, PrintKey, Store
- new example: ripgrep (in options.md)
- cli values now split on ||| instead of nesting level
- support StatusLine template in SetPrompt

## [0.0.12] - 2026-03-09

### 🚀 Features

- Cleaner help display
- Column styles
- Finalize templating
- `start.default_column` and `start.additional_commands`
- ExecuteSilent action
- various bugfixes and documentation

### Performance

- Streamline AppendOnly (preview synchronization) using arc-swap

## [0.0.10] - 2026-03-07

### 💼 Other

- fix cli parsing regressions

## [0.0.9] - 2026-03-07

### 🚀 Features

- Auto-scroll to first match index
- Hscroll
- Semantic aliases in keybinds
- Previewer pausing

### 💼 Other

- matchmaker-partial: support attr(clear) to clear all field attributes.
- various bugfixes

### 🚜 Refactor

- Switch to hashmaps for binds + value sort for display

## [0.0.8] - 2026-02-24

### 🚀 Features

- New actions
- dynamic rebinding
- --last-key now displays the last recorded key
- support --no-multi
- support various toggle/set actions (filtering, sorting, header and more).
- Enhance status line styling
- various bugfixes
- per-preview-layout borders
- hidden columns
- bugfixes
- Richer status line (support template and styling)

## [0.0.7] - 2026-02-22

### 🚀 Features

- matchmaker-partial: support recursive set in collections
- matchmaker-cli: support direct override of preview command (alias: px)
- matchmaker-cli: new aliases: see options.md

### 🚜 Refactor

- Move start and exit configs out from under MatcherConfig to top level

## [0.0.6] - 2026-02-22

### 🚀 Features

- Status template

### 🚜 Refactor

- Lints

## [0.0.4] - 2026-02-19

- Bugfix and documentation updates
- Align version cli and library versions

## [0.0.2] - 2026-02-18

- Various bugfixes and improvements
- New configuration options:
  - PreviewScrollSetting
  - print_template

## [0.0.1] - 2026-02-16

- Re-release as workspace crates.
