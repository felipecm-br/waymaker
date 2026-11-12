//! Golden frames: the full styled screen of the real `wm` (text, colours, bold, ...) for a few
//! canonical states, compared with snapshots in `tests/snapshots/`. Any change to layout,
//! cursor placement or styling shows up as a snapshot diff. After an intended change, review
//! and accept it with `cargo insta review` (or `INSTA_UPDATE=always cargo test --test golden`).
#![cfg(unix)]

mod support;
use support::*;

/// 30 items: enough to scroll at 24 rows and to have several matches for "an".
fn items() -> String {
    let words = [
        "apple",
        "banana",
        "cherry",
        "mango",
        "orange",
        "pineapple",
        "grape",
        "melon",
        "lemon",
        "lime",
    ];
    (0..30)
        .map(|i| format!("{}/{:02}\n", words[i % words.len()], i))
        .collect()
}

#[test]
fn golden_filter_query() {
    let items = items();
    let wm = Launch::new(&items).start();
    wm.send("an");
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_empty_query_scrolled() {
    let items = items();
    let wm = Launch::new(&items).start();
    for _ in 0..25 {
        wm.send(DOWN);
    }
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_multi_select() {
    let items = items();
    let wm = Launch::new(&items).start();
    wm.send(TAB);
    wm.send(TAB);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_no_match() {
    let items = items();
    let wm = Launch::new(&items).start();
    wm.send("zzzz");
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_narrow_terminal() {
    let items = items();
    let wm = Launch::new(&items).size(15, 50).start();
    wm.send("e");
    insta::assert_snapshot!(wm.styled_screen());
}

const FILES: &[(&str, &str)] = &[
    ("src/main.rs", "fn main() {\n    println!(\"hello\");\n}\n"),
    (
        "src/lib.rs",
        "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n",
    ),
    ("README.md", "# Demo\n\nA small project.\n"),
    ("notes.txt", "one\ntwo\nthree\n"),
];

#[test]
fn golden_preview_wide() {
    // README.md first: wm renders Markdown itself, whereas a code file's preview depends on
    // whether the host has `bat`, so the frame would differ between machines.
    let wm = Launch::new("README.md\nnotes.txt\nsrc/lib.rs\nsrc/main.rs\n")
        .files(FILES)
        .size(40, 120)
        .start();
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_jump_nav_mode() {
    let files: &[(&str, &str)] = &[
        ("alpha/one.txt", "1\n"),
        ("alpha/two.txt", "2\n"),
        ("beta/three.txt", "3\n"),
        ("gamma/", ""),
        ("top.md", "# top\n"),
        // Fills the last list row. With an empty query, deeper entries keep the order the
        // parallel walk delivers them in, which depends on the filesystem's readdir order
        // (CI showed `alpha/one.txt`, `alpha/two.txt` and `beta/three.txt` there), so none of
        // them may be on screen.
        ("zeta.txt", "z\n"),
    ];
    let wm = Launch::new("")
        .files(files)
        .no_input()
        .args(&["-o", "jump"])
        .size(30, 100)
        .ready("zeta.txt")
        .start();
    insta::assert_snapshot!(wm.styled_screen());
}

// ---- Results table (make_table) paths ----

/// 20 entries long enough to wrap twice at 60 columns.
fn long_items() -> String {
    (0..20)
        .map(|i| {
            format!(
                "entry_{i:02} {}\n",
                "lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor"
            )
        })
        .collect()
}

#[test]
fn golden_wrapped_rows_scrolled() {
    // Multi-line rows scrolled so the top row is only partly visible: the cursor-above path.
    let items = long_items();
    let wm = Launch::new(&items)
        .args(&["results.wrap=true"])
        .size(24, 60)
        .ready("entry_00")
        .start();
    for _ in 0..9 {
        wm.send(DOWN);
    }
    insta::assert_snapshot!(wm.styled_screen());
}

const TABLE: &str = "alpha   12K  2026-01-02\nbeta    3M   2025-12-31\ngamma   980  2026-03-04\ndelta   1.2G 2024-07-08\n";

#[test]
fn golden_split_columns_right_aligned() {
    let wm = Launch::new(TABLE)
        .args(&["columns.split=\\s+", "results.right_align_last=true"])
        .size(12, 80)
        .ready("delta")
        .start();
    wm.send(DOWN);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_stacked_columns() {
    let wm = Launch::new(TABLE)
        .args(&["columns.split=\\s+", "results.stacked_columns=true"])
        .size(16, 80)
        .ready("alpha")
        .start();
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_group_headers() {
    let wm = Launch::new("# Fruit\napple\nbanana\n# Veg\ncarrot\nleek\n")
        .args(&["--group-prefix", "#"])
        .size(12, 60)
        .ready("4/4") // the two header lines are not items
        .start();
    wm.send(DOWN);
    wm.send(DOWN);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_reverse_layout() {
    let items = items();
    let wm = Launch::new(&items)
        .args(&["results.reverse=true"])
        .size(16, 60)
        .start();
    wm.send("an");
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_file_icons() {
    let wm = Launch::new(
        "src/main.rs\nREADME.md\nCargo.toml\nassets/logo.png\nnotes.txt\nscripts/run.sh\n",
    )
    .args(&["--icons"])
    .size(12, 60)
    .start();
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_horizontal_scroll() {
    let items: String = (0..6)
        .map(|i| format!("row{i} {}\n", "abcdefghij".repeat(12)))
        .collect();
    let wm = Launch::new(&items).size(10, 50).ready("row0").start();
    for _ in 0..8 {
        wm.send("\x1b[1;3C"); // alt-right: HScroll(1)
    }
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_jump_marks() {
    // A selected entry (beta/) next to a yanked one, with the counter in the gap.
    let files: &[(&str, &str)] = &[
        ("alpha/one.txt", "1\n"),
        ("beta/", ""),
        ("notes.md", "# n\n"),
        ("top.md", "# top\n"),
    ];
    let wm = Launch::new("")
        .files(files)
        .no_input()
        .args(&["-o", "jump"])
        .size(30, 100)
        .ready("top.md")
        .start();
    wm.send("j"); // to beta/
    wm.send(" "); // select it; Toggle also moves down, to notes.md
    wm.send("j"); // to top.md
    wm.send("y"); // yank top.md
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_wrapped_icons_at_the_end() {
    // Jumping to the last of many wrapped rows leaves the top row only partly visible, with
    // icons: the cut-off-first-row path of make_table.
    let items: String = (0..25)
        .map(|i| format!("src/module_{i:02}/a_rather_long_file_name_that_wraps_{i:02}.rs\n"))
        .collect();
    let wm = Launch::new(&items)
        .args(&["--icons", "results.wrap=true"])
        .size(20, 56)
        .ready("module_00")
        .start();
    wm.send("\x07"); // ctrl-g: last item
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_split_columns_icons_scrolled() {
    let items: String = (0..30)
        .map(|i| {
            format!(
                "file_{i:02}.rs  {}K  2026-01-{:02}\n",
                i * 3 + 1,
                i % 28 + 1
            )
        })
        .collect();
    let wm = Launch::new(&items)
        .args(&["--icons", "columns.split=\\s+"])
        .size(14, 70)
        .ready("file_00")
        .start();
    for _ in 0..15 {
        wm.send(DOWN);
    }
    insta::assert_snapshot!(wm.styled_screen());
}

const UP: &str = "\x1b[A";

#[test]
fn golden_initial_pos_from_end() {
    // `results.pos` negative: the cursor starts third from the end (make_table's initial_pos).
    let items = items();
    let wm = Launch::new(&items)
        .args(&["results.pos=-3"])
        .size(16, 60)
        .ready("30/30")
        .start();
    insta::assert_snapshot!(wm.styled_screen());
}

/// Rows of uneven height (1 or 3 lines at 60 columns), so scrolling cuts the top row mid-item.
fn mixed_items() -> String {
    (0..20)
        .map(|i| {
            if i % 3 == 0 {
                format!("entry_{i:02} lorem ipsum dolor sit amet consectetur adipiscing elit\n")
            } else {
                format!("entry_{i:02}\n")
            }
        })
        .collect()
}

#[test]
fn golden_wrapped_rows_scrolled_back() {
    // Scrolled down until the top row is clipped, then back up while the clip is kept: the
    // bottom_clip path, which redraws the partly visible top row from the previous frame.
    let items = mixed_items();
    let wm = Launch::new(&items)
        .args(&["results.wrap=true"])
        .size(24, 60)
        .ready("entry_00")
        .start();
    for _ in 0..4 {
        wm.send(DOWN);
    }
    wm.send(UP);
    insta::assert_snapshot!(wm.styled_screen());
}

/// Rows of three columns whose first column wraps, for the stacked layout.
fn stacked_items() -> String {
    (0..20)
        .map(|i| {
            format!(
                "item_{i:02}{}  {}K  2026-01-{:02}\n",
                if i % 3 == 0 {
                    "_with_a_rather_long_name_that_wraps_in_the_list"
                } else {
                    ""
                },
                i + 1,
                i + 1
            )
        })
        .collect()
}

#[test]
fn golden_stacked_columns_scrolled() {
    // Stacked columns scrolled so the top item is cut: the stacked branch of the clipped row.
    let items = stacked_items();
    let wm = Launch::new(&items)
        .args(&[
            "columns.split=\\s\\s+",
            "results.stacked_columns=true",
            "results.wrap=true",
        ])
        .size(30, 80)
        .ready("20/20")
        .start();
    for _ in 0..3 {
        wm.send(DOWN);
    }
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_stacked_columns_scrolled_back() {
    let items = stacked_items();
    let wm = Launch::new(&items)
        .args(&[
            "columns.split=\\s\\s+",
            "results.stacked_columns=true",
            "results.wrap=true",
        ])
        .size(30, 80)
        .ready("20/20")
        .start();
    for _ in 0..3 {
        wm.send(DOWN);
    }
    wm.send(UP);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_stacked_columns_icons() {
    let wm = Launch::new(
        "src/main.rs  4K  2026-01-02\nREADME.md  1K  2026-01-03\nCargo.toml  2K  2026-01-04\n",
    )
    .args(&[
        "--icons",
        "columns.split=\\s\\s+",
        "results.stacked_columns=true",
    ])
    .size(14, 60)
    .ready("3/3")
    .start();
    wm.send(DOWN);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_reverse_wrapped_scrolled() {
    let items = mixed_items();
    let wm = Launch::new(&items)
        .args(&["results.wrap=true", "results.reverse=true"])
        .size(24, 60)
        .ready("entry_00")
        .start();
    for _ in 0..4 {
        wm.send(UP); // reverse: up moves away from the first item
    }
    insta::assert_snapshot!(wm.styled_screen());
}

// ---- make_table: tiers, symlinks, separators, row connection, nav bar, vscroll ----

/// Directories, files at two depths and symlinks: `jump` sorts them into tiers and shows link
/// targets.
const TREE: &[(&str, &str)] = &[
    ("docs/", ""),
    ("src/", ""),
    ("src/main.rs", "fn main() {}\n"),
    ("Cargo.toml", "[package]\n"),
    ("README.md", "# readme\n"),
    ("link_to_src", "-> src"),
    ("link_to_readme", "-> README.md"),
];

fn jump(args: &'static [&'static str], rows: u16, cols: u16) -> Session {
    Launch::new("")
        .files(TREE)
        .no_input()
        .args(args)
        .size(rows, cols)
        .ready("README.md")
        .start()
}

#[test]
fn golden_jump_symlinks_underline_tiers() {
    let wm = jump(&["-o", "jump"], 40, 100);
    wm.send("j");
    // The symlink's tree preview comes from a command; under load it can land after the
    // screen has been quiet long enough for `styled_screen`, so wait for its content.
    wm.wait_for_screen(|s| s.contains("main.rs (13 B)"));
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_jump_light_tier_separator() {
    let wm = jump(&["-o", "jump", "results.tier.separator=light"], 40, 100);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_jump_light_tier_separator_stacked() {
    let wm = jump(
        &[
            "-o",
            "jump",
            "results.tier.separator=light",
            "results.stacked_columns=true",
        ],
        40,
        100,
    );
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_jump_double_nav_bar() {
    let wm = jump(&["-o", "jump", "results.current_nav_bar=Double"], 40, 100);
    wm.send("j");
    // The symlink's tree preview comes from a command; under load it can land after the
    // screen has been quiet long enough for `styled_screen`, so wait for its content.
    wm.wait_for_screen(|s| s.contains("main.rs (13 B)"));
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_jump_wrapped_tiers_scrolled() {
    // Long names wrap, so scrolling cuts the top row next to an underlined tier boundary.
    let files: Vec<(String, &str)> = (0..6)
        .map(|i| {
            (
                format!("dir_{i}_with_a_name_long_enough_to_wrap_in_the_list/"),
                "",
            )
        })
        .chain((0..6).map(|i| {
            (
                format!("file_{i}_with_a_name_long_enough_to_wrap_in_the_list.txt"),
                "x\n",
            )
        }))
        .collect();
    let files: Vec<(&str, &str)> = files.iter().map(|(p, c)| (p.as_str(), *c)).collect();
    let wm = Launch::new("")
        .files(&files)
        .no_input()
        .args(&["-o", "jump", "results.wrap=true"])
        .size(40, 70)
        .ready("dir_0")
        .start();
    for _ in 0..7 {
        wm.send("j");
    }
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_row_separator_scrolled() {
    let items = items();
    let wm = Launch::new(&items)
        .args(&["results.separator=light"])
        .size(16, 60)
        .start();
    for _ in 0..8 {
        wm.send(DOWN);
    }
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_row_connection_capped() {
    let wm = Launch::new(TABLE)
        .args(&["columns.split=\\s+", "results.row_connection=Capped"])
        .size(12, 80)
        .ready("delta")
        .start();
    wm.send(DOWN);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_vscroll_current_only() {
    // alt-down (VScroll(-1)) scrolls the wrapped text of the current row only.
    let items = long_items();
    let wm = Launch::new(&items)
        .args(&["results.wrap=true", "results.vscroll_current_only=true"])
        .size(24, 60)
        .ready("entry_00")
        .start();
    wm.send(DOWN);
    wm.send("\x1b[1;3B");
    wm.send("\x1b[1;3B");
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_row_connection_capped_stacked() {
    let wm = Launch::new(TABLE)
        .args(&[
            "columns.split=\\s+",
            "results.row_connection=Capped",
            "results.stacked_columns=true",
        ])
        .size(16, 80)
        .ready("4/4")
        .start();
    wm.send(DOWN);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_vscroll_current_only_stacked() {
    let items = stacked_items();
    let wm = Launch::new(&items)
        .args(&[
            "columns.split=\\s\\s+",
            "results.stacked_columns=true",
            "results.wrap=true",
            "results.vscroll_current_only=true",
        ])
        .size(30, 80)
        .ready("20/20")
        .start();
    wm.send("\x1b[1;3B");
    wm.send("\x1b[1;3B");
    insta::assert_snapshot!(wm.styled_screen());
}

// ---- Mouse (render/mouse.rs): SGR sequences, 1-based column;row ----

fn press(x: u16, y: u16) -> String {
    format!("\x1b[<0;{x};{y}M")
}
fn release(x: u16, y: u16) -> String {
    format!("\x1b[<0;{x};{y}m")
}
fn drag(x: u16, y: u16) -> String {
    format!("\x1b[<32;{x};{y}M")
}
fn wheel(code: u8, x: u16, y: u16) -> String {
    format!("\x1b[<{code};{x};{y}M")
}
const WHEEL_UP: u8 = 64;
const WHEEL_DOWN: u8 = 65;
const WHEEL_LEFT: u8 = 66;
const WHEEL_RIGHT: u8 = 67;

#[test]
fn golden_mouse_wheel_scrolls_results() {
    let items = items();
    let wm = Launch::new(&items).size(16, 60).start();
    for _ in 0..4 {
        wm.send(&wheel(WHEEL_DOWN, 5, 6));
    }
    wm.send(&wheel(WHEEL_UP, 5, 6));
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_mouse_click_moves_cursor() {
    let items = items();
    let wm = Launch::new(&items).size(16, 60).start();
    // Row 4 (1-based) is the first item; click the third.
    wm.send(&press(5, 6));
    wm.send(&release(5, 6));
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn mouse_click_then_enter_accepts_the_clicked_item() {
    let items = items();
    let wm = Launch::new(&items).size(16, 60).start();
    wm.send(&press(5, 6));
    wm.send(&release(5, 6));
    wm.send(ENTER);
    let (code, out) = wm.finish();
    assert_eq!((code, out.as_str()), (0, "cherry/02\n"));
}

#[test]
fn golden_mouse_click_in_reverse_layout() {
    let items = items();
    let wm = Launch::new(&items)
        .args(&["results.reverse=true"])
        .size(16, 60)
        .start();
    // Reverse layout lists upwards: row 6 (1-based) is banana/01.
    wm.send(&press(5, 6));
    wm.send(&release(5, 6));
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_mouse_click_in_query_moves_the_caret() {
    let items = items();
    let wm = Launch::new(&items).size(16, 60).start();
    wm.send("aple");
    // Row 2 is " > aple": one column of padding, then "> ". Click on the "p" (column 5).
    wm.send(&press(5, 2));
    wm.send(&release(5, 2));
    wm.send("p");
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_mouse_wheel_scrolls_horizontally() {
    let items: String = (0..6)
        .map(|i| format!("{i}_{}\n", "abcdefghij".repeat(8)))
        .collect();
    let wm = Launch::new(&items)
        .args(&["results.wrap=false"])
        .size(12, 50)
        .ready("6/6")
        .start();
    for _ in 0..3 {
        wm.send(&wheel(WHEEL_RIGHT, 5, 4));
    }
    wm.send(&wheel(WHEEL_LEFT, 5, 4));
    insta::assert_snapshot!(wm.styled_screen());
}

fn drag_preview(args: &'static [&'static str], from: (u16, u16), to: (u16, u16)) -> Session {
    let wm = Launch::new("README.md\nnotes.txt\nsrc/lib.rs\nsrc/main.rs\n")
        .files(FILES)
        .args(args)
        .size(40, 120)
        .start();
    wm.send(&press(from.0, from.1));
    wm.send(&drag((from.0 + to.0) / 2, (from.1 + to.1) / 2));
    wm.send(&drag(to.0, to.1));
    wm.send(&release(to.0, to.1));
    wm
}

#[test]
fn golden_mouse_drag_gap_grows_preview() {
    // The default layout has a one-column drag gap at the preview's left edge.
    let wm = drag_preview(&[], (49, 8), (29, 8));
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_mouse_drag_gap_shrinks_preview() {
    let wm = drag_preview(&[], (49, 8), (69, 8));
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_mouse_drag_edge_without_gap() {
    // `gap=0`: the first `drag_width` columns of the preview are the handle instead.
    let wm = drag_preview(&["p.l", "gap=0", "preview.drag_width=2"], (49, 8), (29, 8));
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_mouse_wheel_scrolls_preview() {
    let wm = Launch::new("long.txt\n")
        .files(&[("long.txt", LONG_TEXT)])
        .size(20, 100)
        .ready("line 01")
        .start();
    for _ in 0..3 {
        wm.send(&wheel(WHEEL_DOWN, 80, 6));
    }
    insta::assert_snapshot!(wm.styled_screen());
}

const LONG_TEXT: &str = "line 01\nline 02\nline 03\nline 04\nline 05\nline 06\nline 07\nline 08\nline 09\nline 10\nline 11\nline 12\nline 13\nline 14\nline 15\nline 16\nline 17\nline 18\nline 19\nline 20\n";

#[test]
fn golden_mouse_drag_gap_left_preview() {
    // Preview on the left: its gap is its last column, and dragging right grows it.
    let wm = drag_preview(&["p.l", "side=left"], (72, 8), (92, 8));
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_mouse_click_status_and_footer() {
    // Neither has click interactions by default: the clicks must leave the screen unchanged.
    let items = items();
    let wm = Launch::new(&items).size(16, 60).start();
    wm.send(DOWN);
    wm.send(&press(4, 3));
    wm.send(&release(4, 3));
    wm.send(&press(6, 9));
    wm.send(&release(6, 9));
    insta::assert_snapshot!(wm.styled_screen());
}

// ---- Preview actions (render/actions.rs preview_action), each bound to F1/F2 ----

const F1: &str = "\x1bOP";
const F2: &str = "\x1bOQ";

/// 60 numbered lines, every fifth one wide enough to scroll horizontally.
fn preview_text() -> String {
    (1..=60)
        .map(|i| {
            if i % 5 == 0 {
                format!("line {i:02} {}\n", "wide ".repeat(30))
            } else {
                format!("line {i:02}\n")
            }
        })
        .collect()
}

fn preview_session(args: &'static [&'static str]) -> Session {
    let text: &'static str = Box::leak(preview_text().into_boxed_str());
    let files: &'static [(&'static str, &'static str)] =
        Box::leak(vec![("doc.txt", text), ("other.txt", "other\n")].into_boxed_slice());
    Launch::new("doc.txt\nother.txt\n")
        .files(files)
        .args(args)
        .size(24, 100)
        .ready("line 01")
        .start()
}

#[test]
fn golden_preview_down() {
    let wm = preview_session(&["b.f1", "PreviewDown(5)"]);
    wm.send(F1);
    wm.send(F1);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_preview_down_then_up() {
    let wm = preview_session(&["b.f1", "PreviewDown(9)", "b.f2", "PreviewUp(4)"]);
    wm.send(F1);
    wm.send(F2);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_preview_up_zero_resets() {
    let wm = preview_session(&["b.f1", "PreviewDown(9)", "b.f2", "PreviewUp(0)"]);
    wm.send(F1);
    wm.send(F2);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_preview_half_page_down() {
    let wm = preview_session(&["b.f1", "PreviewHalfPageDown"]);
    wm.send(F1);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_preview_half_page_up() {
    let wm = preview_session(&["b.f1", "PreviewHalfPageDown", "b.f2", "PreviewHalfPageUp"]);
    wm.send(F1);
    wm.send(F1);
    wm.send(F2);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_preview_unwrapped_scroll_down() {
    let wm = preview_session(&["b.f1", "PreviewDown(5)", "b.f2", "TogglePreviewWrap"]);
    wm.send(F2);
    wm.send(F1);
    insta::assert_snapshot!(wm.styled_screen());
}

/// PreviewHScroll shifts unwrapped text (the offset used to reach the Paragraph only when
/// wrapping, where it is ignored).
#[test]
fn preview_hscroll_shifts_unwrapped_text() {
    let wm = preview_session(&["b.f1", "PreviewHScroll(10)", "b.f2", "TogglePreviewWrap"]);
    wm.send(F2);
    wm.send(F1);
    // 10 columns is past "line 05" with or without a line-number gutter (bat adds one when the
    // host has it), and the wide tail of the line is still in view.
    let screen = wm.current_screen();
    assert!(!screen.contains("line 05"), "{screen}");
    assert!(screen.contains("wide wide"), "{screen}");
}

#[test]
fn golden_preview_scroll() {
    let wm = preview_session(&["b.f1", "PreviewScroll(7)"]);
    wm.send(F1);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_preview_jump() {
    let wm = preview_session(&["b.f1", "PreviewDown(20)", "b.f2", "PreviewJump"]);
    wm.send(F1);
    wm.send(F2);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_preview_expand() {
    let wm = preview_session(&["b.f1", "ExpandPreview(15)"]);
    wm.send(F1);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_preview_shrink() {
    let wm = preview_session(&["b.f1", "ShrinkPreview(15)"]);
    wm.send(F1);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_preview_toggle_wrap() {
    let wm = preview_session(&["b.f1", "TogglePreviewWrap"]);
    wm.send(F1);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_preview_command() {
    let wm = preview_session(&["b.f1", "Preview(echo from-a-command {})"]);
    wm.send(F1);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_preview_help() {
    let wm = preview_session(&["b.f1", "Help"]);
    wm.send(F1);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_preview_switch_layout() {
    let wm = preview_session(&[
        "p.l",
        "gap=1",
        "p.l",
        "x=echo second layout",
        "b.f1",
        "SwitchPreview(1)",
    ]);
    wm.send(F1);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_preview_set_layout() {
    let wm = preview_session(&[
        "p.l",
        "gap=1",
        "p.l",
        "x=echo second layout",
        "b.f1",
        "SetPreview(1)",
    ]);
    wm.send(F1);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_preview_cycle_layout() {
    let wm = preview_session(&[
        "p.l",
        "gap=1",
        "p.l",
        "x=echo second layout",
        "b.f1",
        "CyclePreview",
    ]);
    wm.send(F1);
    wm.send(F1);
    wm.send(F1);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_preview_zoom_in() {
    let wm = preview_session(&["b.f1", "PreviewZoomIn"]);
    wm.send(F1);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_preview_zoom_out_reset() {
    let wm = preview_session(&["b.f1", "PreviewZoomOut", "b.f2", "PreviewResetZoom"]);
    wm.send(F1);
    wm.send(F2);
    insta::assert_snapshot!(wm.styled_screen());
}

const DIAGRAMS_MD: &str = "# Diagrams\n\nFirst:\n\n```mermaid\ngraph TD\n  A --> B\n```\n\nSecond:\n\n```mermaid\ngraph LR\n  C --> D\n```\n";

fn diagram_session(args: &'static [&'static str]) -> Session {
    Launch::new("diagrams.md\n")
        .files(&[("diagrams.md", DIAGRAMS_MD)])
        .args(args)
        .size(30, 100)
        .ready("Diagrams")
        .start()
}

#[test]
fn golden_preview_next_diagram() {
    let wm = diagram_session(&["b.f1", "NextDiagram", "b.f2", "PrevDiagram"]);
    wm.send(F1);
    wm.send(F1);
    wm.send(F2);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn preview_toggle_diagram_and_zoom() {
    // The image itself is drawn with terminal graphics placeholders that vary by host, so
    // only the diagram indicator is checked.
    let wm = diagram_session(&["b.f1", "ToggleDiagram", "b.f2", "DiagramZoomIn"]);
    wm.send(F1);
    wm.send(F2);
    wm.wait_for_screen(|s| s.contains("[1/2 · 125%]"));
}

// ---- handle_action (render/actions.rs): actions bound to F1..F3 ----

fn bound(items: &str, args: &'static [&'static str]) -> Session {
    Launch::new(Box::leak(items.to_string().into_boxed_str()))
        .args(args)
        .size(16, 60)
        .start()
}

/// Wait until `wm` (or a command it ran) has created `name` in the working directory.
fn wait_for_file(wm: &Session, name: &str) -> std::path::PathBuf {
    let path = wm.dir.path().join("work").join(name);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !path.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "{name} never appeared"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    path
}

#[test]
fn golden_action_toggle_wrap() {
    let wm = Launch::new(Box::leak(long_items().into_boxed_str()))
        .args(&["b.f1", "ToggleWrap"])
        .size(16, 60)
        .ready("entry_00")
        .start();
    wm.send(F1);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_action_toggle_footer_and_header() {
    let wm = bound(&items(), &["b.f1", "ToggleFooter", "b.f2", "ToggleHeader"]);
    wm.send(F1);
    wm.send(F2);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_action_toggle_action_box() {
    let wm = bound(&items(), &["b.f1", "ToggleActionBox"]);
    wm.send(F1);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_action_toggle_action_box_then_esc() {
    let wm = bound(&items(), &["b.f1", "ToggleActionBox"]);
    wm.send(F1);
    wm.send(ESC);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_action_query_pos() {
    let wm = bound(&items(), &["b.f1", "QueryPos(1)"]);
    wm.send("ale");
    wm.send(F1);
    wm.send("p");
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_action_sort_alphabetical() {
    let wm = bound(&items(), &["b.f1", "Sort(Alphabetical)"]);
    wm.send(F1);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_action_sort_menu() {
    let wm = bound(&items(), &["b.f1", "SortMenu"]);
    wm.send(F1);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_action_prompt_input() {
    let wm = bound(&items(), &["b.f1", "PromptInput(touch {q})"]);
    wm.send(F1);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_action_prompt_input_then_esc() {
    let wm = bound(&items(), &["b.f1", "PromptInput(touch {q})"]);
    wm.send(F1);
    wm.send(ESC);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_action_live_input_then_esc() {
    // While live input is open wm keeps redrawing, so it never goes idle: send without
    // waiting and watch for the LIVE badge instead.
    let wm = bound(&items(), &["b.f1", "LiveInput(echo {q})"]);
    wm.send_raw(F1);
    wm.wait_for_screen(|s| s.contains("LIVE"));
    wm.send_raw("ab");
    wm.send_raw(ESC);
    wm.wait_for_screen(|s| !s.contains("LIVE"));
    wm.wait_until_idle();
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn action_execute_runs_the_command() {
    let wm = bound(&items(), &["b.f1", "Execute(touch ran_execute)"]);
    wm.send(F1);
    wait_for_file(&wm, "ran_execute");
}

#[test]
fn action_execute_silent_runs_the_command() {
    let wm = bound(&items(), &["b.f1", "ExecuteSilent(touch ran_silent)"]);
    wm.send(F1);
    wait_for_file(&wm, "ran_silent");
}

#[test]
fn action_execute_async_runs_the_command() {
    let wm = bound(&items(), &["b.f1", "ExecuteAsync(touch ran_async)"]);
    wm.send(F1);
    wait_for_file(&wm, "ran_async");
}

#[test]
fn action_execute_substitutes_the_current_item() {
    let wm = bound(&items(), &["b.f1", "ExecuteSilent(echo {} > current)"]);
    wm.send(DOWN);
    wm.send(F1);
    let path = wait_for_file(&wm, "current");
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "banana/01\n");
}

#[test]
fn action_store_is_visible_to_later_commands() {
    let wm = bound(
        &items(),
        &[
            "b.f1",
            "Store(kept)",
            "b.f2",
            "ExecuteSilent(echo $WM_STORE > stored)",
        ],
    );
    wm.send(F1);
    wm.send(F2);
    let path = wait_for_file(&wm, "stored");
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "kept\n");
}

#[test]
fn action_print_then_accept() {
    let wm = bound(&items(), &["b.f1", "Print(printed {})"]);
    wm.send(F1);
    wm.send(ENTER);
    let (code, out) = wm.finish();
    // `{}` is shell-quoted in templates, Print included; the accepted item follows.
    assert_eq!((code, out.as_str()), (0, "printed 'apple/00'\napple/00\n"));
}

#[test]
fn action_become_replaces_wm() {
    let wm = bound(&items(), &["b.f1", "Become(echo became {})"]);
    wm.send(DOWN);
    wm.send_raw(F1);
    let (code, out) = wm.finish();
    assert_eq!((code, out.as_str()), (0, "became banana/01\n"));
}

// ---- CLI actions (waymaker-cli action_handler), bound to F1/F2 ----

fn accept_output(wm: Session) -> String {
    wm.send(ENTER);
    let (code, out) = wm.finish();
    assert_eq!(code, 0);
    out
}

#[test]
fn golden_cli_set_header_and_footer() {
    let wm = bound(
        &items(),
        &[
            "b.f1",
            "SetHeader(a header line)",
            "b.f2",
            "SetFooter(a footer line)",
        ],
    );
    wm.send(F1);
    wm.send(F2);
    insta::assert_snapshot!(wm.styled_screen());
}

/// Only PushHeader: a pushed footer text is never drawn (the footer draws its first text
/// only), and with a pushed header the footer hints are fitted to the wrong width.
#[test]
fn golden_cli_push_header() {
    let wm = bound(&items(), &["b.f1", "PushHeader(pushed header)"]);
    wm.send(F1);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_cli_set_prompt_and_status() {
    let wm = bound(
        &items(),
        &[
            "b.f1",
            "SetPrompt(pick: )",
            "b.f2",
            "SetStatus(custom status)",
        ],
    );
    wm.send(F1);
    wm.send(F2);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_cli_set_styled_prompt_footer_status() {
    let wm = bound(
        &items(),
        &[
            "b.f1",
            "SetStyledPrompt({cyan:P} )|||SetStyledFooter({red:F})|||SetStyledStatus({green:S})",
        ],
    );
    wm.send(F1);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn cli_transform_runs_the_printed_actions() {
    let wm = bound(&items(), &["b.f1", "Transform(printf 'Down\\nDown\\n')"]);
    wm.send(F1);
    wm.wait_for_screen(|s| {
        s.contains("> ") && s.lines().nth(1).is_some_and(|l| l.contains("cherry/02"))
    });
    assert_eq!(accept_output(wm), "cherry/02\n");
}

#[test]
fn golden_cli_transform_config() {
    let wm = bound(
        &items(),
        &["b.f1", "TransformConfig(echo results.reverse=true)"],
    );
    wm.send(F1);
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn cli_confirm_runs_the_command_on_enter() {
    let wm = bound(&items(), &["b.f1", "Confirm(Really? ::: touch confirmed)"]);
    wm.send(F1);
    wm.wait_for_screen(|s| s.contains("Really?"));
    wm.send(ENTER);
    wait_for_file(&wm, "confirmed");
}

#[test]
fn cli_confirm_esc_skips_the_command() {
    let wm = bound(&items(), &["b.f1", "Confirm(Really? ::: touch confirmed)"]);
    let path = wm.dir.path().join("work/confirmed");
    wm.send(F1);
    wm.wait_for_screen(|s| s.contains("Really?"));
    wm.send(ESC);
    wm.wait_for_screen(|s| !s.contains("Really?"));
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(!path.exists());
}

#[test]
fn cli_prompt_passes_the_typed_text() {
    let wm = bound(
        &items(),
        &["b.f1", "Prompt(Name: | echo {input} > out | prefilled_)"],
    );
    wm.send(F1);
    wm.wait_for_screen(|s| s.contains("Name:") && s.contains("prefilled_"));
    wm.send(" x;y"); // shell-quoted: the space and ';' reach echo as text
    wm.send(ENTER);
    let out = wait_for_file(&wm, "out");
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(std::fs::read_to_string(out).unwrap(), "prefilled_ x;y\n");
}

#[test]
fn cli_bind_rebinds_a_key() {
    let wm = bound(&items(), &["b.f1", "Bind(f3=Down)"]);
    wm.send(F1);
    wm.send("\x1bOR"); // F3
    wm.send("\x1bOR");
    assert_eq!(accept_output(wm), "cherry/02\n");
}

#[test]
fn cli_unbind_removes_a_key() {
    let wm = bound(&items(), &["b.f3", "Down", "b.f1", "Unbind(f3)"]);
    wm.send(F1);
    wm.send("\x1bOR"); // F3: unbound now
    assert_eq!(accept_output(wm), "apple/00\n");
}

#[test]
fn cli_push_bind_then_pop_bind() {
    let wm = bound(
        &items(),
        &[
            "b.f3",
            "Down",
            "b.f1",
            "PushBind(f3=Pos(-1))",
            "b.f2",
            "PopBind(f3)",
        ],
    );
    wm.send(F1);
    wm.send("\x1bOR"); // F3 = Pos(-1): last item
    wm.send(F2);
    wm.send(UP);
    wm.send("\x1bOR"); // F3 = Down again
    assert_eq!(accept_output(wm), "lime/29\n");
}

#[test]
fn golden_cli_run_preview() {
    let wm = bound(
        &items(),
        &["b.f1", "RunPreview(echo preview from RunPreview)"],
    );
    wm.send(F1);
    wm.wait_for_screen(|s| s.contains("preview from RunPreview"));
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_cli_filtering_off_keeps_all_items() {
    // Filtering(false): typing edits the query but no longer narrows the list.
    let wm = bound(&items(), &["b.f1", "Filtering(false)"]);
    wm.send(F1);
    wm.send("zzz");
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_cli_cycle_sort() {
    let wm = bound(&items(), &["b.f1", "CycleSort"]);
    wm.send("an");
    wm.send(F1);
    insta::assert_snapshot!(wm.styled_screen());
}

// ---- Command previews (preview/previewer.rs run_command_preview) ----

fn cmd_preview(args: &'static [&'static str], ready: &'static str) -> Session {
    Launch::new("alpha\nbeta\ngamma\n")
        .args(args)
        .size(16, 80)
        .ready(ready)
        .start()
}

#[test]
fn golden_preview_cmd_last_line_without_newline() {
    let wm = cmd_preview(
        &["p.l", "x=printf 'first\\nlast line, no newline'"],
        "no newline",
    );
    insta::assert_snapshot!(wm.styled_screen());
}

/// With `previewer.try_lossy` a line with invalid UTF-8 shows U+FFFD in place of the bad bytes
/// (it used to show as an empty line).
#[test]
fn preview_cmd_invalid_utf8_lossy_shows_replacement_char() {
    let wm = cmd_preview(
        &[
            "p.l",
            "x=printf 'good line\\nbad \\377 byte\\nafter\\n'",
            "previewer.try_lossy=true",
        ],
        "after",
    );
    wm.wait_for_screen(|s| s.contains("bad") && s.contains("byte"));
    // The vt100 parser shows U+FFFD as a blank cell, so look for it in what wm wrote.
    assert!(wm.screen_text().contains('\u{fffd}'));
}

/// One invalid byte used to stall a streaming preview until the command exited (each read cut
/// the valid text at the bad byte); without try_lossy the bad bytes are now dropped.
#[test]
fn preview_cmd_invalid_utf8_does_not_stall_streaming() {
    let wm = cmd_preview(
        &[
            "p.l",
            "x=sh -c \"printf 'bad \\377 byte\\n'; echo streamed after; sleep 30\"",
            // see preview_cmd_streams_before_exit_with_delay_clear
            "previewer.delay_clear=false",
        ],
        "3/3",
    );
    wm.wait_for_screen(|s| s.contains("streamed after"));
}

#[test]
fn preview_cmd_long_output_with_and_without_delay_clear() {
    // delay_clear (the default) stages the first 500 lines before swapping them in; both ways
    // the whole output arrives and PreviewJump reaches its end.
    for delay in ["previewer.delay_clear=true", "previewer.delay_clear=false"] {
        let args: &'static [&'static str] =
            Box::leak(vec!["p.l", "x=seq 3000", delay, "b.f1", "PreviewJump"].into_boxed_slice());
        let wm = cmd_preview(args, "1");
        wm.send(F1);
        wm.wait_for_screen(|s| s.contains("3000"));
    }
}

#[test]
fn preview_cmd_shows_the_current_item_after_fast_moves() {
    // Earlier previews still running when the cursor moves on are dropped: the pane ends up
    // with the current item's output only.
    let wm = cmd_preview(&["p.l", "x=sh -c 'sleep 0.2; echo preview of {}'"], "alpha");
    wm.send_raw(DOWN);
    wm.send_raw(DOWN);
    wm.wait_for_screen(|s| s.contains("preview of gamma"));
    std::thread::sleep(std::time::Duration::from_millis(500));
    let screen = wm.current_screen();
    assert!(screen.contains("preview of gamma"), "{screen}");
    assert!(
        !screen.contains("preview of alpha") && !screen.contains("preview of beta"),
        "{screen}"
    );
}

/// With `previewer.delay_clear` (the default) output is staged until it has 500 lines or the
/// command exits; a command that goes quiet with output staged has it shown after a moment, so
/// a slow or streaming command (`tail -f`, a long build) is not blank while it runs.
#[test]
fn preview_cmd_streams_before_exit_with_delay_clear() {
    let wm = cmd_preview(&["p.l", "x=sh -c \"echo streamed hello; sleep 30\""], "3/3");
    wm.wait_for_screen(|s| s.contains("streamed hello"));
}

#[test]
fn golden_cli_push_footer_adds_a_column() {
    // PushHeader / PushFooter append a column, aligned with the result columns (binds.md).
    // Records current behaviour: once the footer has two columns, its hint bar is squeezed into
    // the first result column and cut mid-word ("ente"), like the pushed text ("seco").
    let wm = Launch::new(TABLE)
        .args(&[
            "columns.split=\\s+",
            "b.f1",
            "PushHeader(second col header)",
            "b.f2",
            "PushFooter(second col footer)",
        ])
        .size(12, 100)
        .ready("delta")
        .start();
    wm.send(F1);
    wm.send(F2);
    insta::assert_snapshot!(wm.styled_screen());
}
