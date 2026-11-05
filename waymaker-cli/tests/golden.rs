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
