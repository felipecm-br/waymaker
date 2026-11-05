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
    let wm = Launch::new("src/main.rs\nsrc/lib.rs\nREADME.md\nnotes.txt\n")
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
