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
    // The first Esc hands the keys to navigation mode; the second exits.
    let wm = Session::start(FRUIT, &[]);
    wm.send(ESC);
    wm.send(ESC);
    assert_eq!(wm.finish(), (130, String::new()));
}

#[test]
fn escape_quits_at_once_without_nav_mode() {
    let wm = Session::start(FRUIT, &["ui.nav.active=false"]);
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

    // With the query empty, Esc enters navigation mode, and Esc there quits with 130
    let wm2 = Session::start(FRUIT, &[]);
    wm2.send("ban");
    wm2.send(ESC); // clears query
    wm2.send(ESC); // navigation mode
    wm2.send(ESC); // quits with 130
    assert_eq!(wm2.finish(), (130, String::new()));
}

#[test]
fn escape_enters_navigation_mode_where_letters_are_keys() {
    let wm = Session::start(FRUIT, &[]);
    wm.send(ESC);
    wm.send("j"); // moves down instead of typing into the query
    wm.send(ENTER);
    assert_eq!(wm.finish(), (0, "foobar\n".to_string()));
}

#[test]
fn escape_closes_help_in_navigation_mode_without_quitting() {
    let wm = Session::start(FRUIT, &[]);
    wm.send(ESC);
    wm.send("?");
    wm.wait_for_screen(|s| s.contains("@accept"));
    wm.send(ESC); // closes the help
    wm.send("/"); // back to the filter
    wm.send("ban");
    wm.send(ENTER);
    assert_eq!(wm.finish(), (0, "banana\n".to_string()));
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

/// `,` in navigation mode opens the sort menu, and the next key picks the order.
#[test]
fn nav_comma_opens_the_sort_menu() {
    let wm = Session::start(
        FRUIT,
        &["ui.nav.active=true", "ui.nav.focus_on_start=picker"],
    );
    wm.send(",");
    wm.wait_for_screen(|s| s.contains("Alpha (rev)"));
    wm.send("A"); // alphabetical, reversed
    wm.wait_for_screen(|s| !s.contains("Alpha (rev)"));
    wm.send(ENTER);
    assert_eq!(wm.finish(), (0, "foobar\n".to_string()));
}

/// Keys written at once still resolve in order: the event loop resolves `A` only after the
/// render loop has opened the menu that `,` asked for.
#[test]
fn nav_keys_sent_together_see_the_state_the_earlier_key_left() {
    let wm = Session::start(
        FRUIT,
        &["ui.nav.active=true", "ui.nav.focus_on_start=picker"],
    );
    wm.send_raw(",A");
    wm.wait_until_idle();
    wm.send(ENTER);
    assert_eq!(wm.finish(), (0, "foobar\n".to_string()));
}

/// In navigation mode, help lists the nav binds in effect, then the preview keys, each under its
/// own heading. `u` scrolls the help up: the list profile does not bind it, so this is the
/// fallback bind (half a page up) at work.
#[test]
fn nav_help_lists_nav_and_preview_binds() {
    let wm = Session::start(
        FRUIT,
        &[
            "ui.nav.active=true",
            "ui.nav.focus_on_start=picker",
            "b.f1",
            "Help",
        ],
    );
    wm.send("\x1bOP"); // F1
    wm.wait_for_screen(|s| s.contains("= @Accept"));
    wm.send("G"); // to the end of the help: the preview keys
    wm.wait_for_screen(|s| s.contains("shift-up = PreviewUp(15)"));
    let mut seen = (false, false);
    for _ in 0..40 {
        let screen = wm.current_screen();
        seen.0 |= screen.contains("Preview (fullscreen or diagram)");
        seen.1 |= screen.contains("Navigation");
        if seen == (true, true) {
            break;
        }
        wm.send("u");
    }
    assert_eq!(seen, (true, true), "{}", wm.current_screen());
    wm.send("q");
    assert_eq!(wm.finish().0, 130);
}

#[test]
fn preview_file_subcommand_previews_directory_text_and_image() {
    let bin = &wm_bin();
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

#[test]
fn markdown_preview_goes_idle() {
    // A Markdown preview used to re-render in a loop (its render ends by announcing a preview
    // change, which requested the same render again), redrawing ~30 times a second while idle.
    let files: &[(&str, &str)] = &[
        ("README.md", "# Demo\n\nA small project.\n"),
        ("notes.txt", "x\n"),
    ];
    let wm = Launch::new("notes.txt\nREADME.md\n").files(files).start();
    wm.send(DOWN); // waits until the screen has been quiet for 300 ms, or panics after 10 s
    let before = wm.screen_text().len();
    thread::sleep(Duration::from_secs(1));
    assert_eq!(
        wm.screen_text().len(),
        before,
        "wm kept redrawing an idle Markdown preview"
    );
    wm.send(ESC);
}

// ---- Navigation-mode key contract (the jump preset over real files) ----

const TREE: &[(&str, &str)] = &[
    ("alpha/one.txt", "1\n"),
    ("beta/three.txt", "3\n"),
    ("gamma/", ""),
    ("top.md", "# top\n"),
];

/// The jump preset draws inline in 43% of the terminal height, so give it room for every entry.
fn jump() -> Session {
    let (rows, cols) = (50, 100);
    Launch::new("")
        .files(TREE)
        .no_input()
        .args(&["-o", "jump"])
        .size(rows, cols)
        .ready("top.md")
        .start()
}

/// Puts the cursor on the first item matching `query`, then gives focus back to the list.
fn nav_to(wm: &Session, query: &str) {
    wm.send("/");
    wm.send("\x15"); // ctrl-u: clear the query
    wm.send(query);
    wm.send(ESC); // back to the list (ToggleFocus), keeping the filter
}

#[test]
fn jump_l_enters_and_h_returns_to_the_directory_left() {
    let wm = jump();
    nav_to(&wm, "alpha");
    // Wait on the rendered screen, not the output so far: "one.txt" (alpha's tree preview) and
    // "top.md" are in the stream before either key is handled, so Enter could win the race
    // and accept a file inside alpha. Only the top-level listing shows gamma/.
    wm.send("l");
    wm.wait_for_screen(|s| s.contains("one.txt") && !s.contains("gamma/"));
    wm.send("h");
    wm.wait_for_screen(|s| s.contains("gamma/"));
    // The cursor is back on the directory we came out of.
    wm.send(ENTER);
    let (code, out) = wm.finish();
    assert_eq!(code, 0);
    assert!(
        out.trim_end().trim_end_matches('/').ends_with("alpha"),
        "{out:?}"
    );
}

#[test]
fn jump_yank_paste_then_undo() {
    let wm = jump();
    let work = wm.dir.path().join("work");
    nav_to(&wm, "top.md");
    wm.send("y");
    nav_to(&wm, "gamma");
    wm.send("l"); // into the empty gamma/
    wm.send("p");
    let pasted = work.join("gamma/top.md");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !pasted.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(pasted.exists(), "p did not paste into gamma/");
    assert!(
        work.join("top.md").exists(),
        "a yank (copy) must keep the original"
    );
    wm.send("u");
    let deadline = Instant::now() + Duration::from_secs(5);
    while pasted.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(!pasted.exists(), "u did not undo the paste");
    assert!(work.join("top.md").exists());
    wm.send("\x03"); // ctrl-c (Abort) exits at once; see ctrl_c_quits_at_once
    assert_eq!(wm.finish().0, 130);
}

/// Gets to the state after `u` undid a paste: the "Undone ..." box is up and the yank of
/// top.md is restored.
fn after_undo() -> Session {
    let wm = jump();
    nav_to(&wm, "top.md");
    wm.send("y");
    nav_to(&wm, "gamma");
    wm.send("l");
    wm.send("p");
    wm.send("u");
    wm
}

#[test]
fn ctrl_c_quits_at_once() {
    // ctrl-c is Abort: it exits on the first press even with the "Undone ..." box up and a yank
    // mark restored, where Quit (`q` in nav mode) clears those first, one press each.
    let wm = after_undo();
    wm.send("\x03");
    assert_eq!(wm.finish().0, 130);
}

#[test]
fn jump_f_switches_to_frecency_and_tab_cycles_back() {
    let wm = jump();
    wm.send("f"); // frecency: only the directory wm was started in
    wm.wait_for_screen(|t| t.contains("\u{f0abb}")); // the frecency folder icon in the list
    wm.send(TAB); // next source (bookmarks: empty)
    wm.send(TAB); // and back to the local listing
    wm.wait_for_screen(|t| !t.contains("\u{f0abb}") && t.contains("top.md"));
    wm.send("q");
    assert_eq!(wm.finish().0, 130);
}

#[test]
fn escape_closes_help_before_quitting() {
    let wm = Session::start(FRUIT, &[]);
    wm.send("\x1bOP"); // F1
    wm.wait_for(|t| t.to_lowercase().contains("help"));
    wm.send(ESC); // closes help, does not quit
    wm.send(ENTER);
    assert_eq!(wm.finish(), (0, "apple\n".to_string()));
}

#[test]
fn filter_with_only_negative_terms_lists_the_rest() {
    // `!apple` alone used to match nothing (a pure negation scores 0, which was read as "no
    // match"); like fzf it lists every line without "apple".
    let run = |query: &str| {
        let mut child = std::process::Command::new(wm_bin())
            .args([
                "--config",
                concat!(env!("CARGO_MANIFEST_DIR"), "/assets/config.toml"),
            ])
            .args(["-f", query])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"apple\nbanana\ncherry\napple tart\n")
            .unwrap();
        String::from_utf8(child.wait_with_output().unwrap().stdout).unwrap()
    };
    assert_eq!(run("!apple"), "banana\ncherry\n");
    assert_eq!(run("!apple !cherry"), "banana\n");
    assert_eq!(run("apple !tart"), "apple\n");
}

// ---- File manager (action/fm.rs): create, rename, trash, cut, zip, undo/redo ----
//
// The jump preset's own keys: a create, r rename, d trash, z zip, Z unzip, y/x/p/P, u undo.

fn fm() -> Session {
    Launch::new("")
        .files(TREE)
        .no_input()
        .args(&["-o", "jump"])
        .size(50, 100)
        .ready("top.md")
        .start()
}

/// Polls `cond` for up to five seconds.
fn eventually(what: &str, cond: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !cond() {
        assert!(Instant::now() < deadline, "timed out waiting for: {what}");
        thread::sleep(Duration::from_millis(20));
    }
}

fn abort(wm: Session) {
    wm.send("\x03");
    assert_eq!(wm.finish().0, 130);
}

#[test]
fn fm_create_file_then_undo() {
    let wm = fm();
    let new = wm.dir.path().join("work/new.txt");
    wm.send("a");
    wm.send("new.txt");
    wm.send(ENTER);
    eventually("new.txt created", || new.is_file());
    wm.send("u");
    eventually("undo removes new.txt", || !new.exists());
    abort(wm);
}

/// The "Undone ..." info box must not swallow the nav keys: ctrl-r (`@redo` in jump) used to
/// fall through to the global ctrl-r (`@reload`) while it was up.
#[test]
fn fm_redo_after_undo() {
    let wm = fm();
    let new = wm.dir.path().join("work/new.txt");
    wm.send("a");
    wm.send("new.txt");
    wm.send(ENTER);
    eventually("new.txt created", || new.is_file());
    wm.send("u");
    eventually("undo removes new.txt", || !new.exists());
    wm.send("\x12"); // ctrl-r
    eventually("redo recreates new.txt", || new.is_file());
    abort(wm);
}

#[test]
fn fm_create_directory_with_trailing_slash() {
    let wm = fm();
    let dir = wm.dir.path().join("work/newdir");
    wm.send("a");
    wm.send("newdir/");
    wm.send(ENTER);
    eventually("newdir/ created", || dir.is_dir());
    abort(wm);
}

#[test]
fn fm_create_is_cancelled_by_esc() {
    let wm = fm();
    let never = wm.dir.path().join("work/never.txt");
    wm.send("a");
    wm.send("never.txt");
    wm.send(ESC);
    thread::sleep(Duration::from_millis(300)); // see fm_trash_is_cancelled_by_esc
    wm.send(ENTER); // with the box gone, Enter accepts the current item instead
    thread::sleep(Duration::from_millis(300));
    assert!(!never.exists(), "Esc must not create the file");
    // The session directory still exists: the check above is not vacuous.
    assert!(wm.dir.path().join("work/top.md").is_file());
    assert_eq!(wm.finish().0, 0);
}

#[test]
fn fm_rename_then_undo() {
    let wm = fm();
    let work = wm.dir.path().join("work");
    nav_to(&wm, "top.md");
    wm.send("r");
    wm.send("\x15"); // ctrl-u: clear the prefilled name
    wm.send("renamed.md");
    wm.send(ENTER);
    eventually("top.md renamed", || {
        work.join("renamed.md").is_file() && !work.join("top.md").exists()
    });
    assert_eq!(
        std::fs::read_to_string(work.join("renamed.md")).unwrap(),
        "# top\n"
    );
    wm.send("u");
    eventually("undo restores top.md", || {
        work.join("top.md").is_file() && !work.join("renamed.md").exists()
    });
    abort(wm);
}

/// The rename box is prefilled with the name and the caret starts after it.
#[test]
fn fm_rename_caret_starts_at_the_end() {
    let wm = fm();
    let work = wm.dir.path().join("work");
    nav_to(&wm, "top.md");
    wm.send("r");
    wm.send(BACKSPACE);
    wm.send(BACKSPACE);
    wm.send("txt");
    wm.send(ENTER);
    eventually("top.md renamed to top.txt", || {
        work.join("top.txt").is_file()
    });
    abort(wm);
}

#[test]
fn fm_trash_moves_to_the_xdg_trash_and_undo_restores() {
    let wm = fm();
    let work = wm.dir.path().join("work");
    // fm::move_to_trash uses dirs::data_local_dir(): $XDG_DATA_HOME on Linux, but always
    // ~/Library/Application Support on macOS (HOME is the test dir in both).
    let trash = wm.dir.path().join(if cfg!(target_os = "macos") {
        "Library/Application Support/Trash"
    } else {
        "XDG_DATA_HOME/Trash"
    });
    let trashed = trash.join("files/top.md");
    nav_to(&wm, "top.md");
    wm.send("d");
    wm.send(ENTER);
    eventually("top.md trashed", || {
        !work.join("top.md").exists() && trashed.is_file()
    });
    assert!(
        trash.join("info/top.md.trashinfo").is_file(),
        "no .trashinfo written"
    );
    wm.send("u");
    eventually("undo restores top.md", || work.join("top.md").is_file());
    abort(wm);
}

#[test]
fn fm_trash_is_cancelled_by_esc() {
    let wm = fm();
    let top = wm.dir.path().join("work/top.md");
    nav_to(&wm, "top.md");
    wm.send("d");
    wm.send(ESC);
    // A lone ESC followed at once by "\r" can read as alt-enter; the hidden box gives no
    // redraw to wait for, so leave a gap.
    thread::sleep(Duration::from_millis(300));
    wm.send(ENTER); // with the box gone, Enter accepts the current item instead
    // Check before finish(): it drops the session and its directory.
    thread::sleep(Duration::from_millis(300));
    assert!(top.is_file(), "Esc must not trash the file");
    let (code, out) = wm.finish();
    assert_eq!(code, 0);
    assert!(out.trim_end().ends_with("top.md"), "{out:?}");
}

/// jump fixes the results height (`results.max`), which used to squeeze the action box to zero
/// rows, so its prompt and the typed text were invisible.
#[test]
fn fm_action_box_is_drawn() {
    let wm = fm();
    nav_to(&wm, "top.md");
    wm.send("d");
    wm.wait_for_screen(|s| s.contains("top.md?"));
    abort(wm);
}

#[test]
fn fm_trash_several_selected_items() {
    let wm = fm();
    let work = wm.dir.path().join("work");
    nav_to(&wm, "top.md");
    wm.send(" "); // Toggle
    nav_to(&wm, "gamma");
    wm.send(" ");
    wm.send("d");
    wm.send(ENTER);
    eventually("both trashed", || {
        !work.join("top.md").exists() && !work.join("gamma").exists()
    });
    abort(wm);
}

#[test]
fn fm_cut_and_paste_moves_the_file() {
    let wm = fm();
    let work = wm.dir.path().join("work");
    nav_to(&wm, "top.md");
    wm.send("x");
    nav_to(&wm, "gamma");
    wm.send("l");
    wm.send("p");
    eventually("top.md moved into gamma/", || {
        work.join("gamma/top.md").is_file() && !work.join("top.md").exists()
    });
    abort(wm);
}

#[test]
fn fm_paste_into_the_directory_under_the_cursor() {
    let wm = fm();
    let work = wm.dir.path().join("work");
    nav_to(&wm, "top.md");
    wm.send("y");
    nav_to(&wm, "beta");
    wm.send("P"); // paste into beta/ without entering it
    eventually("top.md copied into beta/", || {
        work.join("beta/top.md").is_file()
    });
    assert!(work.join("top.md").is_file());
    abort(wm);
}

#[test]
fn fm_zip_then_unzip() {
    let wm = fm();
    let work = wm.dir.path().join("work");
    nav_to(&wm, "alpha");
    wm.send("z");
    wm.send(ENTER); // default name: alpha.zip
    eventually("alpha.zip written", || work.join("alpha.zip").is_file());
    std::fs::remove_dir_all(work.join("alpha")).unwrap();
    nav_to(&wm, "alpha.zip");
    wm.send("Z");
    wm.send(ENTER); // default target: alpha
    // The archive keeps its top-level alpha/ and is extracted into a new alpha/, so the file
    // lands one level deeper than it was zipped from.
    eventually("alpha.zip extracted", || {
        std::fs::read_to_string(work.join("alpha/alpha/one.txt")).is_ok_and(|s| s == "1\n")
    });
    abort(wm);
}

/// The shipped `"F2" = "@rename"`: a `[binds]` key bound to a built-in file-manager alias.
/// EventLoop::with_binds drops binds whose `@alias` the bind map cannot resolve, so the CLI
/// defines the built-in aliases a config uses (define_used_builtin_aliases).
#[test]
fn default_f2_renames_the_current_file() {
    let wm = Launch::new("top.md\n").files(TREE).size(20, 80).start();
    let work = wm.dir.path().join("work");
    wm.send("\x1bOQ"); // F2
    wm.send(BACKSPACE);
    wm.send(BACKSPACE);
    wm.send("txt");
    wm.send(ENTER);
    eventually("F2 renamed top.md", || work.join("top.txt").is_file());
    abort(wm);
}
