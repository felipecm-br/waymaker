//! Harness for the end-to-end tests: runs the real `wm` binary in a pseudo-terminal with an
//! isolated environment, sends keys, and reads back its exit code, stdout and screen.
#![allow(dead_code)] // each test binary uses a different subset

use std::{
    io::{Read, Write},
    path::Path,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use portable_pty::{Child, CommandBuilder, PtySize, native_pty_system};

pub const ESC: &str = "\x1b";
pub const ENTER: &str = "\r";
pub const DOWN: &str = "\x1b[B";
pub const TAB: &str = "\t";
pub const BACKSPACE: &str = "\x7f";
pub const BACKTAB: &str = "\x1b[Z";

/// A running `wm` with its own config and state directories.
pub struct Session {
    child: Box<dyn Child + Send + Sync>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    screen: Arc<Mutex<Vec<u8>>>,
    pub dir: tempfile::TempDir,
    size: (u16, u16),
}

/// How to start a [`Session`]: `Launch::new(items).args(..).size(rows, cols).start()`.
pub struct Launch<'a> {
    items: &'a str,
    args: &'a [&'a str],
    rows: u16,
    cols: u16,
    files: &'a [(&'a str, &'a str)],
    no_input: bool,
    ready: Option<&'a str>,
}

impl<'a> Launch<'a> {
    pub fn new(items: &'a str) -> Self {
        Self {
            items,
            args: &[],
            rows: 24,
            cols: 80,
            files: &[],
            no_input: false,
            ready: None,
        }
    }
    pub fn args(mut self, args: &'a [&'a str]) -> Self {
        self.args = args;
        self
    }
    pub fn size(mut self, rows: u16, cols: u16) -> Self {
        self.rows = rows;
        self.cols = cols;
        self
    }
    /// Files (`path`, `contents`) to create in the working directory; a path ending in `/` is
    /// a directory.
    pub fn files(mut self, files: &'a [(&'a str, &'a str)]) -> Self {
        self.files = files;
        self
    }
    /// Leave stdin on the terminal so `wm` lists the working directory itself.
    pub fn no_input(mut self) -> Self {
        self.no_input = true;
        self
    }
    /// Text that shows the picker is up (default: the last input item).
    pub fn ready(mut self, text: &'a str) -> Self {
        self.ready = Some(text);
        self
    }
    pub fn start(self) -> Session {
        Session::launch(self)
    }
}

impl Session {
    /// Start `wm args...` reading `items` on stdin, with stdout captured to a file.
    pub fn start(items: &str, args: &[&str]) -> Self {
        Launch::new(items).args(args).start()
    }

    fn launch(launch: Launch<'_>) -> Self {
        let Launch {
            items,
            args,
            rows,
            cols,
            files,
            no_input,
            ready,
        } = launch;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("input"), items).unwrap();
        for (path, contents) in files {
            let path = dir.path().join("work").join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            if path.to_string_lossy().ends_with('/') {
                std::fs::create_dir_all(&path).unwrap();
            } else {
                std::fs::write(&path, contents).unwrap();
            }
        }
        std::fs::create_dir_all(dir.path().join("work")).unwrap();

        let pty = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();

        let mut cmd = CommandBuilder::new("/bin/sh");
        // With `no_input`, stdin stays the terminal, so `wm` lists the working directory itself.
        let script = if no_input {
            r#"cd "$WM_TEST_DIR/work" && exec "$0" "$@" > "$WM_TEST_DIR/output""#
        } else {
            r#"cd "$WM_TEST_DIR/work" && exec "$0" "$@" < "$WM_TEST_DIR/input" > "$WM_TEST_DIR/output""#
        };
        cmd.args(["-c", script]);
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
            size: (rows, cols),
        };
        // Keys sent before the picker is up are lost, so wait for the last item.
        let last_item = ready.unwrap_or_else(|| items.lines().last().unwrap_or_default());
        session.wait_for(|text| text.contains(last_item));
        session.wait_until_idle();
        session
    }

    pub fn wait_for(&self, ready: impl Fn(&str) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !ready(&strip_ansi(&self.screen_text())) {
            if Instant::now() > deadline {
                panic!("wm did not draw; screen so far: {}", self.screen_text());
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    /// Wait until `wm` has stopped writing for a moment.
    pub fn wait_until_idle(&self) {
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

    /// Send keys without waiting for `wm` to finish redrawing.
    pub fn send_raw(&self, keys: &str) {
        self.writer
            .lock()
            .unwrap()
            .write_all(keys.as_bytes())
            .unwrap();
    }

    pub fn send(&self, keys: &str) {
        self.writer
            .lock()
            .unwrap()
            .write_all(keys.as_bytes())
            .unwrap();
        self.wait_until_idle();
    }

    /// Wait for `wm` to exit and return its exit code and stdout.
    pub fn finish(mut self) -> (u32, String) {
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

    pub fn screen_text(&self) -> String {
        String::from_utf8_lossy(&self.screen.lock().unwrap()).into_owned()
    }

    /// The current screen as a terminal shows it: the text rows, then every run of cells with
    /// non-default style (`row: cols attrs`). The session's temporary directory is written as
    /// `$DIR`, so snapshots do not depend on where the test runs.
    pub fn styled_screen(&self) -> String {
        let (rows, cols) = self.size;
        let mut parser = vt100::Parser::new(rows.try_into().unwrap(), cols.try_into().unwrap(), 0);
        parser.process(&self.screen.lock().unwrap());
        let screen = parser.screen();
        let dir = self.dir.path().to_string_lossy().into_owned();

        let mut text = String::new();
        let mut styles = String::new();
        for row in 0..rows {
            let mut line = String::new();
            let mut runs: Vec<(u16, u16, String)> = Vec::new();
            for col in 0..cols {
                let cell = screen.cell(row, col).unwrap();
                if !cell.is_wide_continuation() {
                    line.push_str(if cell.has_contents() {
                        cell.contents()
                    } else {
                        " "
                    });
                }
                let style = cell_style(cell);
                match runs.last_mut() {
                    Some((_, end, s)) if *s == style && *end + 1 == col => *end = col,
                    _ => runs.push((col, col, style)),
                }
            }
            text.push_str(&format!("{row:2}|{}\n", line.trim_end()));
            let runs: Vec<String> = runs
                .into_iter()
                .filter(|(_, _, s)| !s.is_empty())
                .map(|(a, b, s)| {
                    if a == b {
                        format!("{a} {s}")
                    } else {
                        format!("{a}-{b} {s}")
                    }
                })
                .collect();
            if !runs.is_empty() {
                styles.push_str(&format!("{row:2}: {}\n", runs.join(" | ")));
            }
        }
        format!("{text}-- styles --\n{styles}").replace(&dir, "$DIR")
    }
}

/// A clean environment: no user config, history or `WM_*` settings leak in.
pub fn isolate_env(cmd: &mut CommandBuilder, dir: &Path) {
    // Presets and previews run `wm` by name: make that the binary under test, not an installed one.
    let bin_dir = Path::new(env!("CARGO_BIN_EXE_wm")).parent().unwrap();
    cmd.env(
        "PATH",
        format!(
            "{}:{}",
            bin_dir.display(),
            std::env::var("PATH").unwrap_or_default()
        ),
    );
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
pub fn strip_ansi(s: &str) -> String {
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

pub fn count(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|w| *w == needle)
        .count()
}

/// A cell's non-default attributes, e.g. `fg=6 bold`; empty for plain text.
fn cell_style(cell: &vt100::Cell) -> String {
    let color = |c: vt100::Color| match c {
        vt100::Color::Default => None,
        vt100::Color::Idx(i) => Some(i.to_string()),
        vt100::Color::Rgb(r, g, b) => Some(format!("#{r:02x}{g:02x}{b:02x}")),
    };
    let mut parts = Vec::new();
    if let Some(c) = color(cell.fgcolor()) {
        parts.push(format!("fg={c}"));
    }
    if let Some(c) = color(cell.bgcolor()) {
        parts.push(format!("bg={c}"));
    }
    for (on, name) in [
        (cell.bold(), "bold"),
        (cell.dim(), "dim"),
        (cell.italic(), "italic"),
        (cell.underline(), "underline"),
        (cell.inverse(), "inverse"),
    ] {
        if on {
            parts.push(name.to_string());
        }
    }
    parts.join(" ")
}
