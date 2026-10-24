//! End-to-end tests that drive the real `wm` binary through a pseudo-terminal:
//! the event loop, action dispatch and frame drawing run exactly as they do for
//! a user, which the in-process unit tests cannot reach.
#![cfg(unix)]

use std::{
    io::{Read, Write},
    path::Path,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use portable_pty::{Child, CommandBuilder, PtySize, native_pty_system};

const ESC: &str = "\x1b";
const ENTER: &str = "\r";
const DOWN: &str = "\x1b[B";
const TAB: &str = "\t";
const BACKSPACE: &str = "\x7f";
const BACKTAB: &str = "\x1b[Z";

/// A running `wm` with its own config and state directories.
struct Session {
    child: Box<dyn Child + Send + Sync>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    screen: Arc<Mutex<Vec<u8>>>,
    dir: tempfile::TempDir,
}

impl Session {
    /// Start `wm args...` reading `items` on stdin, with stdout captured to a file.
    fn start(items: &str, args: &[&str]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("input"), items).unwrap();

        let pty = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();

        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.args([
            "-c",
            r#"exec "$0" "$@" < "$WM_TEST_DIR/input" > "$WM_TEST_DIR/output""#,
        ]);
        cmd.arg(env!("CARGO_BIN_EXE_wm"));
        // Debug builds otherwise load (and rewrite) the developer playground
        // config, dev.toml. --config with the shipped defaults is exactly what a
        // release build uses when the user has no config.
        cmd.args([
            "--config",
            concat!(env!("CARGO_MANIFEST_DIR"), "/assets/config.toml"),
        ]);
        cmd.args(args);
        cmd.env_clear();
        isolate_env(&mut cmd, dir.path());

        let child = pty.slave.spawn_command(cmd).unwrap();
        drop(pty.slave);

        let writer = Arc::new(Mutex::new(pty.master.take_writer().unwrap()));
        let screen = Arc::new(Mutex::new(Vec::new()));
        let mut reader = pty.master.try_clone_reader().unwrap();
        {
            let (writer, screen) = (writer.clone(), screen.clone());
            thread::spawn(move || {
                // Keep the master alive for as long as the reader runs.
                let _master = pty.master;
                let mut buf = [0u8; 4096];
                let mut answered = 0;
                while let Ok(n @ 1..) = reader.read(&mut buf) {
                    let mut screen = screen.lock().unwrap();
                    screen.extend_from_slice(&buf[..n]);
                    // Answer cursor-position queries like a terminal would, so
                    // startup does not wait for them to time out.
                    let queries = count(&screen, b"\x1b[6n");
                    for _ in answered..queries {
                        let _ = writer.lock().unwrap().write_all(b"\x1b[1;1R");
                    }
                    answered = queries;
                }
            });
        }

        let session = Self {
            child,
            writer,
            screen,
            dir,
        };
        // Keys sent before the picker is up are lost, so wait for the last item.
        let last_item = items.lines().last().unwrap_or_default();
        session.wait_for(|text| text.contains(last_item));
        session.wait_until_idle();
        session
    }

    fn wait_for(&self, ready: impl Fn(&str) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !ready(&strip_ansi(&self.screen_text())) {
            if Instant::now() > deadline {
                panic!("wm did not draw; screen so far: {}", self.screen_text());
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    /// Wait until `wm` has stopped writing for a moment.
    fn wait_until_idle(&self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut last_len = 0;
        let mut quiet_since = Instant::now();
        while Instant::now() < deadline {
            let len = self.screen.lock().unwrap().len();
            if len != last_len {
                last_len = len;
                quiet_since = Instant::now();
            } else if len > 0 && quiet_since.elapsed() > Duration::from_millis(300) {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        panic!("wm did not settle; screen so far: {}", self.screen_text());
    }

    fn send(&self, keys: &str) {
        self.writer
            .lock()
            .unwrap()
            .write_all(keys.as_bytes())
            .unwrap();
        self.wait_until_idle();
    }

    /// Wait for `wm` to exit and return its exit code and stdout.
    fn finish(mut self) -> (u32, String) {
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() > deadline {
                let _ = self.child.kill();
                panic!("wm did not exit; screen: {}", self.screen_text());
            }
            thread::sleep(Duration::from_millis(20));
        };
        let output = std::fs::read_to_string(self.dir.path().join("output")).unwrap();
        (status.exit_code(), output)
    }

    fn screen_text(&self) -> String {
        String::from_utf8_lossy(&self.screen.lock().unwrap()).into_owned()
    }
}

/// A clean environment: no user config, history or `WM_*` settings leak in.
fn isolate_env(cmd: &mut CommandBuilder, dir: &Path) {
    cmd.env("PATH", std::env::var("PATH").unwrap_or_default());
    cmd.env("TERM", "xterm-256color");
    // Lets `cargo llvm-cov` collect coverage from the wm process too.
    if let Ok(profile) = std::env::var("LLVM_PROFILE_FILE") {
        cmd.env("LLVM_PROFILE_FILE", profile);
    }
    cmd.env("WM_TEST_DIR", dir);
    cmd.env("HOME", dir);
    for var in [
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
        "XDG_CACHE_HOME",
    ] {
        cmd.env(var, dir.join(var));
    }
}

/// Drop CSI escape sequences, leaving the text the terminal would show.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.next() == Some('[') {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn count(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|w| *w == needle)
        .count()
}

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
fn escape_quits_with_1() {
    // The default config binds esc to Quit(1); ctrl-c's plain Quit is 130.
    let wm = Session::start(FRUIT, &[]);
    wm.send(ESC);
    assert_eq!(wm.finish(), (1, String::new()));
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
