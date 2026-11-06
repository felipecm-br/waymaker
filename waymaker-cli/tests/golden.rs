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
    ];
    let wm = Launch::new("")
        .files(files)
        .no_input()
        .args(&["-o", "jump"])
        .size(30, 100)
        .ready("top.md")
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
    let wm = drag_preview(&[], (72, 8), (52, 8));
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_mouse_drag_gap_shrinks_preview() {
    let wm = drag_preview(&[], (72, 8), (92, 8));
    insta::assert_snapshot!(wm.styled_screen());
}

#[test]
fn golden_mouse_drag_edge_without_gap() {
    // `gap=0`: the first `drag_width` columns of the preview are the handle instead.
    let wm = drag_preview(&["p.l", "gap=0", "preview.drag_width=2"], (72, 8), (52, 8));
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
    let wm = drag_preview(&["p.l", "side=left"], (49, 8), (69, 8));
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

/// BUG: `PreviewHScroll` moves no text. `PreviewUI` passes its scroll offset to the text
/// `Paragraph` only when wrapping (ui/preview.rs), and a wrapped paragraph ignores the
/// horizontal offset, so text previews never scroll sideways in either mode.
#[test]
#[ignore = "known bug: PreviewHScroll does not scroll text previews"]
fn preview_hscroll_shifts_unwrapped_text() {
    let wm = preview_session(&["b.f1", "PreviewHScroll(5)", "b.f2", "TogglePreviewWrap"]);
    wm.send(F2);
    wm.send(F1);
    // Scrolled 5 columns, "line 05 wide wide ..." no longer shows its "line " prefix.
    let screen = wm.screen_text();
    assert!(!screen.contains("line 05 wide"), "{screen}");
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
