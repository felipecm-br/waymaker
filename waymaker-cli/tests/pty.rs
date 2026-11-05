//! End-to-end tests that drive the real `wm` binary through a pseudo-terminal:
//! the event loop, action dispatch and frame drawing run exactly as they do for
//! a user, which the in-process unit tests cannot reach.
#![cfg(unix)]

mod support;
use support::*;

use std::{
    thread,
    time::{Duration, Instant},
};

const FRUIT: &str = "apple\nfoobar\nbanana\n";

#[test]
fn query_then_accept_prints_the_match() {
    let wm = Session::start(FRUIT, &[]);
    wm.send("foo");
    wm.send(ENTER);
    let (code, out) = wm.finish();
    assert_eq!((code, out.as_str()), (0, "foobar\n"));
}

#[test]
fn escape_quits_with_130() {
    // Cancelling exits 130 like ctrl-c (and fzf), so scripts can tell it from "no match" (1).
    let wm = Session::start(FRUIT, &[]);
    wm.send(ESC);
    assert_eq!(wm.finish(), (130, String::new()));
}

#[test]
fn escape_cascade_clears_query_before_quitting() {
    let wm = Session::start(FRUIT, &[]);
    wm.send("ban");
    // First Esc clears the query back to empty without quitting
    wm.send(ESC);
    // Enter now accepts the first item from the unfiltered list ("apple"), proving query was cleared
    wm.send(ENTER);
    assert_eq!(wm.finish(), (0, "apple\n".to_string()));

    // When query is already empty, Esc quits with 130
    let wm2 = Session::start(FRUIT, &[]);
    wm2.send("ban");
    wm2.send(ESC); // clears query
    wm2.send(ESC); // quits with 130
    assert_eq!(wm2.finish(), (130, String::new()));
}

#[test]
fn ctrl_g_jumps_to_last_item() {
    let wm = Session::start(FRUIT, &[]);
    wm.send("\x07"); // ctrl-g
    wm.send(ENTER);
    assert_eq!(wm.finish(), (0, "banana\n".to_string()));
}

#[test]
fn enter_accepts_the_first_item() {
    let wm = Session::start(FRUIT, &[]);
    wm.send(ENTER);
    assert_eq!(wm.finish(), (0, "apple\n".to_string()));
}

#[test]
fn down_moves_the_cursor() {
    let wm = Session::start(FRUIT, &[]);
    wm.send(DOWN);
    wm.send(ENTER);
    assert_eq!(wm.finish(), (0, "foobar\n".to_string()));
}

#[test]
fn backspace_edits_the_query() {
    let wm = Session::start(FRUIT, &[]);
    wm.send("ban");
    wm.send(BACKSPACE);
    wm.send(BACKSPACE);
    wm.send(BACKSPACE);
    wm.send("foo");
    wm.send(ENTER);
    assert_eq!(wm.finish(), (0, "foobar\n".to_string()));
}

/// `*` and `?` reach the query: the shipped config used to bind them to bookmark and Help.
#[test]
fn star_and_question_mark_type_into_the_query() {
    let wm = Session::start("apple\nsrc/*.rs?\nbanana\n", &[]);
    wm.send("*?");
    wm.send(ENTER);
    assert_eq!(wm.finish(), (0, "src/*.rs?\n".to_string()));
}

/// Bind overrides on the command line reach the running picker.
#[test]
fn cli_bind_override_takes_effect() {
    let wm = Session::start(FRUIT, &["b.enter=Quit(7)"]);
    wm.send(ENTER);
    assert_eq!(wm.finish(), (7, String::new()));
}

#[test]
fn enter_with_no_match_exits_1() {
    let wm = Session::start(FRUIT, &[]);
    wm.send("zzzz");
    wm.send(ENTER);
    assert_eq!(wm.finish(), (1, String::new()));
}

/// An output template can print something without a match, so that still succeeds.
#[test]
fn output_template_without_match_succeeds() {
    let wm = Session::start(FRUIT, &["o={q}"]);
    wm.send("zzzz");
    wm.send(ENTER);
    // Templates shell-quote placeholders.
    assert_eq!(wm.finish(), (0, "'zzzz'\n".to_string()));
}

#[test]
fn tab_selects_multiple_items() {
    let wm = Session::start(FRUIT, &[]);
    wm.send(TAB);
    wm.send(TAB);
    wm.send(ENTER);
    assert_eq!(wm.finish(), (0, "apple\nfoobar\n".to_string()));
}

const UP: &str = "\x1b[A";
const LEFT: &str = "\x1b[D";
const PAGE_DOWN: &str = "\x1b[6~";
const PAGE_UP: &str = "\x1b[5~";

#[test]
fn up_from_the_top_stays_put() {
    let wm = Session::start(FRUIT, &[]);
    wm.send(UP);
    wm.send(ENTER);
    assert_eq!(wm.finish(), (0, "apple\n".to_string()));
}

#[test]
fn down_past_the_end_stays_on_the_last_item() {
    let wm = Session::start(FRUIT, &[]);
    for _ in 0..3 {
        wm.send(DOWN);
    }
    wm.send(ENTER);
    assert_eq!(wm.finish(), (0, "banana\n".to_string()));
}

/// Half-page moves clamp at the ends instead of circling a short list.
#[test]
fn page_down_stops_at_the_last_item() {
    let wm = Session::start(FRUIT, &[]);
    wm.send(PAGE_DOWN);
    wm.send(ENTER);
    assert_eq!(wm.finish(), (0, "banana\n".to_string()));
}

/// With scroll_wrap on, a half page longer than the list used to circle it.
#[test]
fn page_down_with_scroll_wrap_stops_at_the_last_item() {
    let wm = Session::start(FRUIT, &["results.scroll_wrap=true"]);
    wm.send(PAGE_DOWN);
    wm.send(ENTER);
    assert_eq!(wm.finish(), (0, "banana\n".to_string()));
}

#[test]
fn page_up_stops_at_the_first_item() {
    let wm = Session::start(FRUIT, &[]);
    wm.send(DOWN);
    wm.send(PAGE_UP);
    wm.send(ENTER);
    assert_eq!(wm.finish(), (0, "apple\n".to_string()));
}

#[test]
fn alt_digits_jump_to_the_ends() {
    let wm = Session::start(FRUIT, &[]);
    wm.send("\x1b0");
    wm.send(ENTER);
    assert_eq!(wm.finish(), (0, "banana\n".to_string()));

    let wm = Session::start(FRUIT, &[]);
    wm.send(DOWN);
    wm.send("\x1b1");
    wm.send(ENTER);
    assert_eq!(wm.finish(), (0, "apple\n".to_string()));
}

#[test]
fn ctrl_u_clears_the_query() {
    let wm = Session::start(FRUIT, &[]);
    wm.send("zzzz");
    wm.send("\x15");
    wm.send(ENTER);
    assert_eq!(wm.finish(), (0, "apple\n".to_string()));
}

#[test]
fn cursor_movement_inserts_mid_query() {
    let wm = Session::start(FRUIT, &[]);
    wm.send("oba");
    for _ in 0..3 {
        wm.send(LEFT);
    }
    wm.send("f");
    wm.send(ENTER);
    assert_eq!(wm.finish(), (0, "foobar\n".to_string()));
}

#[test]
fn ctrl_a_selects_all() {
    let wm = Session::start(FRUIT, &[]);
    wm.send("\x01");
    wm.send(ENTER);
    assert_eq!(wm.finish(), (0, FRUIT.to_string()));
}

/// shift-tab walks back up, deselecting.
#[test]
fn shift_tab_deselects_upwards() {
    let wm = Session::start(FRUIT, &[]);
    wm.send(TAB);
    wm.send(TAB);
    wm.send(BACKTAB);
    wm.send(ENTER);
    assert_eq!(wm.finish(), (0, "apple\n".to_string()));
}

#[test]
fn ctrl_c_quits_with_130() {
    let wm = Session::start(FRUIT, &[]);
    wm.send("\x03");
    assert_eq!(wm.finish(), (130, String::new()));
}

#[test]
fn nav_bind_resolves_custom_semantic_confirm_action() {
    let wm = Session::start(
        FRUIT,
        &[
            "ui.nav.active=true",
            "ui.nav.focus_on_start=picker",
            "--nav-bind",
            "d:@delete_worktree",
            "binds.@delete_worktree=Confirm(Delete {=}? | printf '%s' {=} > \"$WM_TEST_DIR/deleted\")",
        ],
    );
    wm.send("d");
    wm.wait_for(|text| text.contains("Delete") && text.contains("apple?"));
    wm.send(ENTER);
    wm.wait_until_idle();
    let deleted_path = wm.dir.path().join("deleted");
    let deadline = Instant::now() + Duration::from_secs(3);
    while !deleted_path.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        std::fs::read_to_string(&deleted_path).unwrap_or_default(),
        "apple"
    );
    wm.send("q");
    assert_eq!(wm.finish().0, 130);
}

#[test]
fn preview_file_subcommand_previews_directory_text_and_image() {
    let bin = env!("CARGO_BIN_EXE_wm");
    let tmp = tempfile::tempdir().unwrap();

    // 1. Directory preview renders directory tree
    let sub = tmp.path().join("subdir");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("hello.txt"), "world").unwrap();

    let dir_out = std::process::Command::new(bin)
        .args(["preview-file", tmp.path().to_str().unwrap()])
        .output()
        .expect("failed to run preview-file on directory");
    assert!(dir_out.status.success());
    let dir_text = String::from_utf8_lossy(&dir_out.stdout);
    assert!(dir_text.contains("subdir") || dir_text.contains("hello.txt"));

    // 2. Text/code preview renders line numbers
    let file_path = tmp.path().join("code.rs");
    std::fs::write(&file_path, "fn main() {\n    println!(\"hello\");\n}\n").unwrap();
    let text_out = std::process::Command::new(bin)
        .args(["preview-file", file_path.to_str().unwrap()])
        .output()
        .expect("failed to run preview-file on text file");
    assert!(text_out.status.success());
    let file_text = strip_ansi(&String::from_utf8_lossy(&text_out.stdout));
    assert!(file_text.contains("fn main"));

    // 3. Image preview renders image metadata or visual output
    let img_path = tmp.path().join("test.png");
    let png_bytes = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    std::fs::write(&img_path, png_bytes).unwrap();
    let img_out = std::process::Command::new(bin)
        .args(["preview-file", img_path.to_str().unwrap()])
        .output()
        .expect("failed to run preview-file on image");
    assert!(img_out.status.success());
    let img_text = String::from_utf8_lossy(&img_out.stdout);
    assert!(
        img_text.contains("Image") || img_text.contains("PNG") || img_text.contains("test.png")
    );
}
