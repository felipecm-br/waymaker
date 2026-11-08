use ansi_to_tui::IntoText;
use cba::broc::{CommandExt, EnvVars};
use futures::FutureExt;
use log::{debug, error, warn};
use ratatui::text::{Line, Text};
use std::io::BufReader;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::watch::{Receiver, Sender, channel};
use tokio::task::JoinHandle;

use super::AppendOnly;
use crate::config::PreviewerConfig;
use crate::event::EventSender;
use crate::message::Event;
use crate::preview::Preview;

#[derive(Debug, Default, strum_macros::Display, Clone)]
pub enum PreviewMessage {
    Run(String, EnvVars),
    Rerun(String, EnvVars),
    Set(Text<'static>),
    Media(String),
    Markdown(String, Option<usize>),
    Unset,
    #[default]
    Stop,
    Pause,
    Unpause,
}

#[derive(Debug)]
pub struct Previewer {
    /// The reciever for for [`PreviewMessage`]'s.
    rx: Receiver<PreviewMessage>,
    /// storage for preview command output
    lines: AppendOnly<Line<'static>>,
    /// storage for preview string override
    string: Arc<Mutex<Option<Text<'static>>>>,
    /// storage for preview image override
    image: Arc<Mutex<Option<image::DynamicImage>>>,
    /// Version ID for image state to detect changes reliably
    pub image_id: Arc<AtomicU64>,
    /// Flag which is set to true whenever the state changes
    /// and which the viewer can toggle after receiving the current state
    changed: Arc<AtomicBool>,

    /// Rendered line offsets of Mermaid diagram blocks within the current Markdown preview.
    /// Each entry is the 0-based line index in the rendered Text where a diagram starts.
    /// Shared with `Preview` so the render loop can navigate between diagrams.
    pub diagram_offsets: Arc<Mutex<Vec<usize>>>,
    /// Extracted Mermaid diagram source strings for the current Markdown file.
    pub diagram_sources: Arc<Mutex<Vec<String>>>,
    /// Index of the currently displayed diagram.
    pub current_diagram_idx: Arc<std::sync::atomic::AtomicUsize>,
    generation: Arc<AtomicU64>,

    paused: bool,
    /// Maintain a queue of child processes to improve cleanup reliability
    procs: Vec<Child>,
    /// The currently executing child process
    current: Option<(Child, JoinHandle<bool>)>,
    /// The currently executing media/markdown background task
    current_media_task: Option<JoinHandle<()>>,

    pub config: PreviewerConfig,
    last: String,
    /// Event loop controller
    // We only use it to send [`ControlEvent::Event`]
    event_controller_tx: Option<EventSender>,
}

#[inline]
fn try_parse_simple_command(cmd: &str) -> Option<Command> {
    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return None;
    }
    // If command contains any shell metacharacter, let the shell handle it
    if trimmed.chars().any(|c| {
        matches!(
            c,
            '|' | '>'
                | '<'
                | '&'
                | ';'
                | '$'
                | '`'
                | '\n'
                | '*'
                | '?'
                | '('
                | ')'
                | '{'
                | '}'
                | '\''
                | '"'
                | '\\'
        )
    }) {
        return None;
    }
    let mut parts = trimmed.split_whitespace();
    let prog = parts.next()?;
    let mut command = Command::new(prog);
    for arg in parts {
        command.arg(arg);
    }
    Some(command)
}

impl Previewer {
    pub fn new(config: PreviewerConfig) -> (Self, Sender<PreviewMessage>) {
        let (tx, rx) = channel(PreviewMessage::Stop);

        let new = Self {
            rx,
            lines: AppendOnly::new(),
            string: Default::default(),
            image: Default::default(),
            image_id: Arc::new(AtomicU64::new(1)),
            changed: Default::default(),
            diagram_offsets: Default::default(),
            diagram_sources: Default::default(),
            current_diagram_idx: Default::default(),
            generation: Arc::new(AtomicU64::new(0)),
            paused: false,

            procs: Vec::new(),
            current: None,
            current_media_task: None,
            config,
            last: Default::default(),
            event_controller_tx: None,
        };

        (new, tx)
    }

    pub fn view(&self) -> Preview {
        Preview::new(
            self.lines.clone(),
            self.string.clone(),
            self.image.clone(),
            self.image_id.clone(),
            self.changed.clone(),
            self.diagram_offsets.clone(),
            self.diagram_sources.clone(),
            self.current_diagram_idx.clone(),
        )
    }

    pub fn set_string(&self, s: Text<'static>) {
        if let Ok(mut guard) = self.string.lock() {
            *guard = Some(s);
            self.changed.store(true, Ordering::Release);
        }
    }

    pub fn clear_diagrams(&self) {
        if let Ok(mut guard) = self.diagram_sources.lock() {
            guard.clear();
        }
        if let Ok(mut guard) = self.diagram_offsets.lock() {
            guard.clear();
        }
        self.current_diagram_idx.store(0, Ordering::Release);
    }

    pub fn clear_string(&self) {
        if let Ok(mut guard) = self.string.lock() {
            *guard = None;
            self.changed.store(true, Ordering::Release);
        }
        self.clear_diagrams();
    }

    pub fn has_string(&self) -> bool {
        let guard = self.string.lock();
        guard.is_ok_and(|s| s.is_some())
    }

    pub fn set_image(&self, img: image::DynamicImage) {
        if let Ok(mut guard) = self.image.lock() {
            *guard = Some(img);
            self.image_id.fetch_add(1, Ordering::Release);
            self.changed.store(true, Ordering::Release);
        }
    }

    pub fn clear_image(&self) {
        if let Ok(mut guard) = self.image.lock()
            && guard.is_some()
        {
            *guard = None;
            self.image_id.fetch_add(1, Ordering::Release);
            self.changed.store(true, Ordering::Release);
        }
        if crate::utils::mermaid::is_kitty_supported() {
            crate::utils::mermaid::delete_kitty_all();
        }
    }

    pub async fn run(mut self) -> Result<(), Vec<Child>> {
        while self.rx.changed().await.is_ok() {
            if let Some(task) = self.current_media_task.take() {
                task.abort();
            }
            let m = self.rx.borrow_and_update().clone();

            let mut is_debouncable = false;
            let mut force_run = false;
            let mut key = String::new();
            if let PreviewMessage::Run(cmd, _) = &m {
                is_debouncable = true;
                key = cmd.clone();
            } else if let PreviewMessage::Rerun(cmd, _) = &m {
                is_debouncable = true;
                force_run = true;
                key = cmd.clone();
            } else if let PreviewMessage::Media(cmd) = &m {
                is_debouncable = true;
                key = cmd.clone();
            } else if let PreviewMessage::Markdown(cmd, width) = &m {
                is_debouncable = true;
                key = format!("{cmd}:{width:?}");
            }

            if is_debouncable {
                // `always_trigger` re-runs a preview command whose output may differ each time. A
                // file render (Markdown, media) at the same width cannot, and re-rendering it is
                // what makes it ping-pong: the render ends by sending `PreviewChange`, which asks
                // for the same render again, so the picker redrew ~30 times a second while idle.
                let renders_file =
                    matches!(m, PreviewMessage::Media(_) | PreviewMessage::Markdown(..));
                if !force_run && (!self.config.always_trigger || renders_file) && self.last == key {
                    continue;
                }

                if !self.config.delay_clear {
                    self.clear_image();
                    self.clear_string();
                    self.lines.clear();
                    self.changed.store(true, Ordering::Release);
                }

                if !force_run && self.config.debounce_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(self.config.debounce_ms)).await;
                }

                while self.procs.len() >= self.config.max_procs {
                    self.prune_procs();
                    if self.procs.len() < self.config.max_procs {
                        break;
                    }

                    log::error!(
                        "too many zombie processes ({}), delaying preview update ({m:?}).",
                        self.procs.len()
                    );
                    let sleep = self.config.debounce_ms.max(10);
                    tokio::time::sleep(Duration::from_millis(sleep)).await;

                    if self.rx.has_changed().unwrap_or(false) {
                        break;
                    }
                }

                if !force_run && self.rx.has_changed().unwrap_or(false) {
                    continue;
                }
            }

            if !self.procs.is_empty() {
                debug!("procs: {:?}", self.procs);
            }

            {
                match m {
                    PreviewMessage::Pause => {
                        self.paused = true;
                        continue;
                    }
                    PreviewMessage::Unpause => {
                        self.paused = false;
                        continue;
                    }
                    _ if self.paused => {
                        continue;
                    }
                    PreviewMessage::Set(s) => {
                        self.set_string(s.clone());
                        self.last.clear();
                        // don't kill the underlying
                        continue;
                    }
                    PreviewMessage::Unset => {
                        self.dispatch_kill();
                        self.clear_string();
                        self.clear_image();
                        self.clear_diagrams();
                        self.last.clear();
                        continue;
                    }
                    PreviewMessage::Media(ref path) => {
                        self.start_media_preview(path);
                        continue;
                    }
                    PreviewMessage::Markdown(ref path, width) => {
                        self.start_markdown_preview(path, width);
                        continue;
                    }
                    _ => {}
                }
            }

            if !matches!(m, PreviewMessage::Media(_) | PreviewMessage::Markdown(_, _)) {
                self.dispatch_kill();
                self.clear_image();
                self.clear_diagrams();
                if !self.config.delay_clear {
                    self.clear_string();
                }
            }

            match m {
                PreviewMessage::Run(cmd, variables) | PreviewMessage::Rerun(cmd, variables) => {
                    self.run_command_preview(cmd, variables)
                }
                PreviewMessage::Stop => {
                    self.dispatch_kill();
                    self.clear_string();
                    self.clear_image();
                    self.clear_diagrams();
                    self.lines.clear();
                    self.changed.store(true, Ordering::Relaxed);
                    self.last.clear();
                }
                _ => {}
            }

            self.prune_procs();
        }

        let ret = self.cleanup_procs();
        if ret.is_empty() { Ok(()) } else { Err(ret) }
    }

    fn dispatch_kill(&mut self) {
        if let Some(task) = self.current_media_task.take() {
            task.abort();
        }
        if let Some((mut child, old)) = self.current.take() {
            kill_child(&mut child);
            self.procs.push(child);

            // drop future
            let mut old = Box::pin(old);
            match old.as_mut().now_or_never() {
                // unicode error
                Some(Ok(false)) => self.send(Event::Refresh),
                None => {
                    old.abort(); // still works because `AbortHandle` is separate
                }
                _ => {}
            }
        }
    }

    fn send(&self, event: Event) {
        if let Some(ref tx) = self.event_controller_tx {
            let _ = tx.send(event);
        }
    }

    pub fn connect_controller(&mut self, event_controller_tx: EventSender) {
        self.event_controller_tx = Some(event_controller_tx)
    }

    // todo: This would be cleaner with tokio::Child, but does that merit a conversion? I'm not sure if its worth it for the previewer to yield control while waiting for output cuz we are multithreaded anyways
    // also, maybe don't want this delaying exit?
    fn cleanup_procs(mut self) -> Vec<Child> {
        let total_timeout = Duration::from_secs(1);
        let start = Instant::now();

        self.procs.retain_mut(|child| {
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => return false,
                    Ok(None) => {
                        if start.elapsed() >= total_timeout {
                            error!("Child failed to exit in time: {:?}", child);
                            return true;
                        } else {
                            thread::sleep(Duration::from_millis(10));
                        }
                    }
                    Err(e) => {
                        error!("Error waiting on child: {e}");
                        return true;
                    }
                }
            }
        });

        self.procs
    }

    fn prune_procs(&mut self) {
        self.procs.retain_mut(|child| match child.try_wait() {
            Ok(None) => {
                kill_child(child);
                true
            }
            Ok(Some(_)) => false,
            Err(e) => {
                warn!("Error waiting on child: {e}");
                true
            }
        });
    }

    /// Decodes an image, video thumbnail or PDF page in a background task into `image`, unless a
    /// newer preview made it stale.
    #[allow(clippy::ptr_arg)] // the arm binds `ref path: &String`; kept as moved
    fn start_media_preview(&mut self, path: &String) {
        self.dispatch_kill();
        self.clear_diagrams();
        if !self.config.delay_clear {
            self.clear_image();
            self.clear_string();
            self.lines.clear();
        }
        self.last = path.clone();
        let path = path.clone();
        let image_state = self.image.clone();
        let string_state = self.string.clone();
        let image_id = self.image_id.clone();
        let changed = self.changed.clone();
        let event_tx = self.event_controller_tx.clone();
        let current_generation = self.generation.clone();

        let media_size = self.config.media_size;
        let rx = self.rx.clone();
        let task_gen = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let handle = tokio::spawn(async move {
            let is_stale = {
                let rx = rx.clone();
                let current_generation = current_generation.clone();
                move || {
                    rx.has_changed().unwrap_or(false)
                        || current_generation.load(Ordering::Acquire) != task_gen
                }
            };
            if is_stale() {
                return;
            }

            let img_result = if path.to_lowercase().ends_with(".pdf") {
                // PDF support using pdftoppm if available
                let pdf_scale = if media_size > 0 {
                    media_size.to_string()
                } else {
                    "1200".to_string()
                };
                let mut cmd = tokio::process::Command::new("pdftoppm");
                cmd.kill_on_drop(true);
                cmd.args([
                    "-jpeg",
                    "-r",
                    "150",
                    "-scale-to",
                    &pdf_scale,
                    "-f",
                    "1",
                    "-l",
                    "1",
                    &path,
                ]);
                if let Ok(_out) = cmd.output().await {
                    if is_stale() {
                        return;
                    }
                    let p = std::path::PathBuf::from(&path);
                    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("document");
                    let rendered = p.with_file_name(format!("{}-1.jpg", stem));
                    if rendered.exists() {
                        let img = image::open(&rendered).ok();
                        let _ = std::fs::remove_file(&rendered);
                        img
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else if path.to_lowercase().ends_with(".mp4")
                || path.to_lowercase().ends_with(".mkv")
                || path.to_lowercase().ends_with(".webm")
                || path.to_lowercase().ends_with(".mov")
                || path.to_lowercase().ends_with(".avi")
                || path.to_lowercase().ends_with(".flv")
                || path.to_lowercase().ends_with(".m4v")
                || path.to_lowercase().ends_with(".wmv")
            {
                // Video support
                let media_size_str = if media_size > 0 {
                    media_size.to_string()
                } else {
                    "512".to_string()
                };
                let mut cmd = tokio::process::Command::new("ffmpegthumbnailer");
                cmd.kill_on_drop(true);
                cmd.args([
                    "-i",
                    &path,
                    "-s",
                    &media_size_str,
                    "-t",
                    "00:00:01",
                    "-c",
                    "jpeg",
                    "-q",
                    "10",
                    "-o",
                    "-",
                ]);
                if let Ok(out) = cmd.output().await {
                    if is_stale() {
                        return;
                    }
                    // ffmpegthumbnailer sometimes pollutes stdout with debug logs.
                    // We search for the JPEG start of image marker (FF D8 FF) and slice the buffer.
                    let data = &out.stdout;
                    let img_data =
                        if let Some(pos) = data.windows(3).position(|w| w == [0xff, 0xd8, 0xff]) {
                            &data[pos..]
                        } else {
                            data
                        };
                    image::load_from_memory(img_data).ok()
                } else {
                    None
                }
            } else {
                let path_for_task = path.clone();
                let is_stale_task = is_stale.clone();
                tokio::task::spawn_blocking(move || {
                    if is_stale_task() {
                        return None;
                    }
                    let img = image::open(&path_for_task).ok()?;
                    if is_stale_task() {
                        return None;
                    }
                    let max_dim = media_size;
                    if max_dim > 0 && (img.width() > max_dim || img.height() > max_dim) {
                        Some(img.thumbnail(max_dim, max_dim))
                    } else {
                        Some(img)
                    }
                })
                .await
                .ok()
                .flatten()
            };

            if is_stale() {
                return;
            }

            if let Some(img) = img_result {
                if let Ok(mut guard) = image_state.lock() {
                    if is_stale() {
                        return;
                    }
                    *guard = Some(img);
                    image_id.fetch_add(1, Ordering::Release);
                    changed.store(true, Ordering::Release);
                }
                if let Ok(mut str_guard) = string_state.lock() {
                    if is_stale() {
                        return;
                    }
                    *str_guard = None;
                }
                if let Some(ref tx) = event_tx {
                    let _ = tx.send(crate::message::Event::PreviewChange);
                }
            }
        });
        self.current_media_task = Some(handle);
    }

    /// Renders a Markdown file (with its Mermaid diagrams) in a background task; the result lands in
    /// `string` (or `image` for a diagram-only file) unless a newer preview made it stale.
    #[allow(clippy::ptr_arg)] // the arm binds `ref path: &String`; kept as moved
    fn start_markdown_preview(&mut self, path: &String, width: Option<usize>) {
        self.reset_markdown_preview_state(path, width);
        let path = path.clone();
        let string_state = self.string.clone();
        let image_state = self.image.clone();
        let image_id = self.image_id.clone();
        let diagram_offsets_state = self.diagram_offsets.clone();
        let diagram_sources_state = self.diagram_sources.clone();
        let current_diagram_idx_state = self.current_diagram_idx.clone();
        let changed = self.changed.clone();
        let rx = self.rx.clone();
        let event_tx = self.event_controller_tx.clone();
        let current_generation = self.generation.clone();
        let media_enabled = self.config.media;
        let markdown_diagrams = self.config.markdown_diagrams;
        let inline_diagrams = self.config.inline_diagrams;
        let inline_images = self.config.inline_images;
        let diagram_theme = self.config.diagram_theme;
        let diagram_background = self.config.diagram_background;

        let task_gen = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let handle = tokio::task::spawn_blocking(move || {
            let is_stale = || {
                rx.has_changed().unwrap_or(false)
                    || current_generation.load(Ordering::Acquire) != task_gen
            };
            if is_stale() {
                return;
            }

            let p = std::path::Path::new(&path);
            let is_mermaid = p
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|ext| matches!(ext.to_lowercase().as_str(), "mmd" | "mermaid"));

            if is_mermaid {
                let content = std::fs::read_to_string(p).ok();
                if is_stale() {
                    return;
                }
                if let Some(ref c) = content
                    && let Ok(mut src_guard) = diagram_sources_state.lock()
                {
                    if is_stale() {
                        return;
                    }
                    *src_guard = vec![c.clone()];
                }
                if is_stale() {
                    return;
                }
                current_diagram_idx_state.store(0, std::sync::atomic::Ordering::Release);

                if markdown_diagrams
                    && inline_diagrams
                    && crate::utils::mermaid::is_kitty_supported()
                    && let Some(ref c) = content
                    && let Some(diag) =
                        crate::utils::mermaid::render_mermaid_to_unicode_placeholders_with_options(
                            c,
                            width,
                            diagram_theme,
                            diagram_background,
                        )
                {
                    if is_stale() {
                        return;
                    }
                    crate::utils::mermaid::transmit_kitty_image_idempotent(
                        diag.image_id,
                        &diag.transmission,
                    );
                    let mut lines = Vec::with_capacity(diag.lines.len() + 2);
                    lines.push(Line::default());
                    lines.extend(diag.lines);
                    lines.push(Line::default());
                    let rendered_text = Text::from(lines);
                    if is_stale() {
                        return;
                    }
                    finish_mermaid_text_preview(
                        rendered_text,
                        &is_stale,
                        &string_state,
                        &image_state,
                        &image_id,
                        &changed,
                        event_tx,
                    );
                    return;
                }

                if media_enabled && markdown_diagrams {
                    // Try graphics-based rendering fallback (Kitty protocol)
                    let img = crate::utils::mermaid::render_mermaid_file_to_image_with_options(
                        p,
                        2.0,
                        diagram_theme,
                        diagram_background, // Scale 2.0 for high-DPI rendering
                    );

                    if is_stale() {
                        return;
                    }

                    if let Some(rendered_img) = img {
                        finish_mermaid_image_preview(
                            rendered_img,
                            &is_stale,
                            &image_state,
                            &image_id,
                            &changed,
                            &string_state,
                            event_tx,
                        );
                        return;
                    }
                    // Fall through to text rendering if image failed
                }

                let opts = crate::utils::mermaid::MermaidOptions {
                    max_width: width,
                    show_box: true,
                    title: Some("Mermaid Diagram".to_string()),
                    theme: diagram_theme,
                    background: diagram_background,
                    ..Default::default()
                };
                let rendered_text = match content {
                    Some(ref c) => crate::utils::mermaid::render_mermaid(c, &opts),
                    None => {
                        crate::utils::mermaid::render_mermaid_file(p, &opts).unwrap_or_else(|err| {
                            Text::from(format!("Error reading Mermaid file: {err}"))
                        })
                    }
                };

                if is_stale() {
                    return;
                }
                finish_mermaid_text_preview(
                    rendered_text,
                    &is_stale,
                    &string_state,
                    &image_state,
                    &image_id,
                    &changed,
                    event_tx,
                );
            } else {
                // Markdown file — render with diagram offset tracking
                let opts = crate::utils::markdown::MarkdownOptions {
                    max_width: width,
                    base_path: Some(p.to_path_buf()),
                    render_mermaid: markdown_diagrams,
                    inline_diagrams,
                    inline_images,
                    diagram_theme,
                    diagram_background,
                    ..Default::default()
                };
                let (rendered_text, offsets) =
                    crate::utils::markdown::render_markdown_file_with_diagram_offsets(p, &opts)
                        .unwrap_or_else(|err| {
                            (
                                Text::from(format!("Error reading Markdown file: {err}")),
                                vec![],
                            )
                        });

                preview_markdown_file(
                    p,
                    media_enabled,
                    diagram_theme,
                    diagram_background,
                    rendered_text,
                    offsets,
                    &is_stale,
                    &string_state,
                    &image_state,
                    &image_id,
                    &diagram_offsets_state,
                    &diagram_sources_state,
                    &current_diagram_idx_state,
                    &changed,
                    event_tx,
                );
            }
        });
        self.current_media_task = Some(handle);
    }

    /// Runs a preview command and streams its output into `lines` from a background task
    /// (re-rendering ANSI and Mermaid output), unless a newer preview made it stale.
    fn run_command_preview(&mut self, cmd: String, variables: EnvVars) {
        self.last = cmd.clone();
        let mut cmd_builder = self.make_preview_command(&cmd);

        cmd_builder
            .envs(variables.iter().cloned())
            .stdout(Stdio::piped())
            .stdin(Stdio::null())
            .stderr(Stdio::null());

        #[cfg(not(target_os = "windows"))]
        cmd_builder.detach();

        if !self.config.delay_clear {
            self.lines.clear();
            self.changed.store(true, Ordering::Relaxed);
        }

        // we need the child handle
        if let Some(mut child) = cmd_builder._spawn() {
            let task_gen = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
            if let Some(stdout) = child.stdout.take() {
                let lines = self.lines.clone();
                let mut guard = self.lines.read();
                let changed = self.changed.clone();
                let cmd_str = cmd.clone();
                let string_state = self.string.clone();
                let image_state = self.image.clone();
                let image_id = self.image_id.clone();
                let diagram_sources_state = self.diagram_sources.clone();
                let diagram_offsets_state = self.diagram_offsets.clone();
                let current_diagram_idx_state = self.current_diagram_idx.clone();
                let current_generation = self.generation.clone();

                let delay_clear = self.config.delay_clear;
                let try_lossy = self.config.try_lossy;

                // false => needs refresh (i.e. invalid utf-8)
                let handle = tokio::spawn(async move {
                    // Blocking reads on a thread of their own, so this task can notice a quiet
                    // command: see the timeout below.
                    let (chunk_tx, mut chunk_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
                    spawn_stdout_reader_thread(stdout, chunk_tx);
                    let mut leftover = Vec::new();
                    let mut staged = Vec::new();
                    let mut has_swapped = false;
                    // What the new output replaces when it is swapped in: the previous text, image
                    // and diagrams.
                    let clear_other_views = || {
                        reset_other_views(
                            &string_state,
                            &image_state,
                            &image_id,
                            &diagram_sources_state,
                            &diagram_offsets_state,
                            &current_diagram_idx_state,
                        );
                    };

                    loop {
                        let chunk =
                            match tokio::time::timeout(STAGED_SWAP_AFTER, chunk_rx.recv()).await {
                                Ok(Some(chunk)) => chunk,
                                Ok(None) => break, // end of output
                                Err(_) => {
                                    if current_generation.load(Ordering::Acquire) != task_gen {
                                        break; // a newer preview took over
                                    }
                                    // The command went quiet with output still staged (delay_clear):
                                    // show it now rather than when it exits or reaches 500 lines.
                                    if delay_clear && !has_swapped && !staged.is_empty() {
                                        if current_generation.load(Ordering::Acquire) != task_gen {
                                            return true;
                                        }
                                        lines.replace_all(std::mem::take(&mut staged));
                                        has_swapped = true;
                                        guard = lines.read();
                                        clear_other_views();
                                        changed.store(true, Ordering::Release);
                                    }
                                    continue;
                                }
                            };
                        if current_generation.load(Ordering::Acquire) != task_gen {
                            break;
                        }

                        leftover.extend_from_slice(&chunk);
                        clean_invalid_utf8_lines(&mut leftover, try_lossy, false);

                        let valid_up_to = match std::str::from_utf8(&leftover) {
                            Ok(_) => leftover.len(),
                            Err(e) => e.valid_up_to(),
                        };

                        let split_at = leftover[..valid_up_to]
                            .iter()
                            .rposition(|&b| b == b'\n' || b == b'\r')
                            .map(|pos| pos + 1)
                            .unwrap_or(valid_up_to);

                        let (valid_bytes, rest) = leftover.split_at(split_at);

                        match valid_bytes.into_text() {
                            Ok(text) => {
                                let has_text = !text.lines.is_empty();
                                if delay_clear && !has_swapped {
                                    staged.extend(text.lines);
                                    if staged.len() >= 500 {
                                        if current_generation.load(Ordering::Acquire) != task_gen {
                                            return true;
                                        }
                                        lines.replace_all(std::mem::take(&mut staged));
                                        has_swapped = true;
                                        guard = lines.read();
                                        clear_other_views();
                                        changed.store(true, Ordering::Release);
                                    }
                                } else {
                                    for line in text {
                                        if current_generation.load(Ordering::Acquire) != task_gen
                                            || lines.is_expired(&guard)
                                        {
                                            return true;
                                        }
                                        guard.push(line);
                                    }
                                    if has_text {
                                        changed.store(true, Ordering::Release);
                                    }
                                }
                            }
                            Err(e) => {
                                if try_lossy {
                                    let mut has_text = false;
                                    for bytes in valid_bytes.split(|b| *b == b'\n') {
                                        let line =
                                            Line::from(String::from_utf8_lossy(bytes).into_owned());
                                        if delay_clear && !has_swapped {
                                            staged.push(line);
                                        } else {
                                            if current_generation.load(Ordering::Acquire)
                                                != task_gen
                                                || lines.is_expired(&guard)
                                            {
                                                return true;
                                            }
                                            guard.push(line);
                                            has_text = true;
                                        }
                                    }
                                    if delay_clear && !has_swapped && staged.len() >= 500 {
                                        if current_generation.load(Ordering::Acquire) != task_gen {
                                            return true;
                                        }
                                        lines.replace_all(std::mem::take(&mut staged));
                                        has_swapped = true;
                                        guard = lines.read();
                                        clear_other_views();
                                        changed.store(true, Ordering::Release);
                                    } else if has_text && has_swapped {
                                        changed.store(true, Ordering::Release);
                                    }
                                } else {
                                    error!("Error displaying {cmd_str}: {:?}", e);
                                    return false;
                                }
                            }
                        }

                        leftover = rest.to_vec();
                    }

                    finish_command_preview_stream(
                        leftover,
                        try_lossy,
                        delay_clear,
                        has_swapped,
                        staged,
                        &current_generation,
                        task_gen,
                        &lines,
                        guard,
                        &changed,
                        &cmd_str,
                        &clear_other_views,
                    )
                });
                self.current = Some((child, handle))
            } else {
                error!("Failed to get stdout of preview command: {cmd}")
            }
        }
    }

    fn reset_markdown_preview_state(&mut self, path: &str, width: Option<usize>) {
        if !self.config.delay_clear {
            self.clear_image();
            self.clear_string();
            self.lines.clear();
        }
        self.dispatch_kill();
        // Clear any stale diagram offsets from previous preview
        if let Ok(mut offsets) = self.diagram_offsets.lock() {
            offsets.clear();
        }
        if let Ok(mut sources) = self.diagram_sources.lock() {
            sources.clear();
        }
        self.current_diagram_idx
            .store(0, std::sync::atomic::Ordering::Release);
        self.last = format!("{path}:{width:?}");
    }

    fn make_preview_command(&self, cmd: &str) -> Command {
        if let Some(s) = &self.config.shell
            && !s.is_empty()
        {
            let mut iter = s.iter();
            let mut program = Command::new(iter.next().unwrap());
            program.args(iter).arg(cmd);
            program
        } else if let Some(direct) = try_parse_simple_command(cmd) {
            direct
        } else {
            Command::from_script(cmd)
        }
    }
}

fn finish_mermaid_image_preview(
    rendered_img: image::DynamicImage,
    is_stale: &impl Fn() -> bool,
    image_state: &std::sync::Mutex<Option<image::DynamicImage>>,
    image_id: &AtomicU64,
    changed: &std::sync::atomic::AtomicBool,
    string_state: &std::sync::Mutex<Option<Text<'static>>>,
    event_tx: Option<EventSender>,
) {
    if let Ok(mut guard) = image_state.lock() {
        if is_stale() {
            return;
        }
        *guard = Some(rendered_img);
        image_id.fetch_add(1, Ordering::Release);
        changed.store(true, Ordering::Release);
    }
    if let Ok(mut str_guard) = string_state.lock() {
        if is_stale() {
            return;
        }
        *str_guard = None;
    }
    if let Some(ref tx) = event_tx
        && !is_stale()
    {
        let _ = tx.send(crate::message::Event::PreviewChange);
    }
}

fn finish_mermaid_text_preview(
    rendered_text: Text<'static>,
    is_stale: &impl Fn() -> bool,
    string_state: &std::sync::Mutex<Option<Text<'static>>>,
    image_state: &std::sync::Mutex<Option<image::DynamicImage>>,
    image_id: &AtomicU64,
    changed: &std::sync::atomic::AtomicBool,
    event_tx: Option<EventSender>,
) {
    if let Ok(mut guard) = string_state.lock() {
        if is_stale() {
            return;
        }
        *guard = Some(rendered_text);
        changed.store(true, Ordering::Release);
    }
    if let Ok(mut img_guard) = image_state.lock() {
        if is_stale() {
            return;
        }
        *img_guard = None;
        image_id.fetch_add(1, Ordering::Release);
    }
    if let Some(ref tx) = event_tx
        && !is_stale()
    {
        let _ = tx.send(crate::message::Event::PreviewChange);
    }
}

#[allow(clippy::too_many_arguments)]
fn preview_markdown_file(
    p: &std::path::Path,
    media_enabled: bool,
    diagram_theme: crate::config::DiagramTheme,
    diagram_background: crate::config::DiagramBackground,
    rendered_text: Text<'static>,
    offsets: Vec<usize>,
    is_stale: &impl Fn() -> bool,
    string_state: &std::sync::Mutex<Option<Text<'static>>>,
    image_state: &std::sync::Mutex<Option<image::DynamicImage>>,
    image_id: &AtomicU64,
    diagram_offsets_state: &std::sync::Mutex<Vec<usize>>,
    diagram_sources_state: &std::sync::Mutex<Vec<String>>,
    current_diagram_idx_state: &std::sync::atomic::AtomicUsize,
    changed: &std::sync::atomic::AtomicBool,
    event_tx: Option<EventSender>,
) {
    if is_stale() {
        return;
    }

    // If media is enabled, extract and prepare diagram image
    let mut diag_img = None;
    let mut sources = Vec::new();
    if media_enabled && let Ok(content) = std::fs::read_to_string(p) {
        let diagrams = crate::utils::markdown::extract_mermaid_blocks(&content);
        sources = diagrams.into_iter().map(|(_, s)| s).collect();
        if let Some(first_diag) = sources.first() {
            diag_img = crate::utils::mermaid::render_mermaid_to_image_with_options(
                first_diag,
                2.0,
                diagram_theme,
                diagram_background,
            );
        }
    }

    if is_stale() {
        return;
    }

    if let Ok(mut img_guard) = image_state.lock() {
        if is_stale() {
            return;
        }
        *img_guard = diag_img;
        image_id.fetch_add(1, Ordering::Release);
    }
    if let Ok(mut src_guard) = diagram_sources_state.lock() {
        if is_stale() {
            return;
        }
        *src_guard = sources;
    }
    if is_stale() {
        return;
    }
    current_diagram_idx_state.store(0, std::sync::atomic::Ordering::Release);

    // Store diagram offsets for navigation
    if let Ok(mut diag_guard) = diagram_offsets_state.lock() {
        if is_stale() {
            return;
        }
        *diag_guard = offsets;
    }
    if let Ok(mut guard) = string_state.lock() {
        if is_stale() {
            return;
        }
        *guard = Some(rendered_text);
        changed.store(true, Ordering::Release);
    }

    if let Some(ref tx) = event_tx
        && !is_stale()
    {
        let _ = tx.send(crate::message::Event::PreviewChange);
    }
}

fn spawn_stdout_reader_thread(
    stdout: std::process::ChildStdout,
    chunk_tx: tokio::sync::mpsc::Sender<Vec<u8>>,
) {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut buf = [0u8; 8192];
        while let Ok(n) = std::io::Read::read(&mut reader, &mut buf) {
            if n == 0 || chunk_tx.blocking_send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });
}

fn reset_other_views(
    string_state: &std::sync::Mutex<Option<Text<'static>>>,
    image_state: &std::sync::Mutex<Option<image::DynamicImage>>,
    image_id: &AtomicU64,
    diagram_sources_state: &std::sync::Mutex<Vec<String>>,
    diagram_offsets_state: &std::sync::Mutex<Vec<usize>>,
    current_diagram_idx_state: &std::sync::atomic::AtomicUsize,
) {
    if let Ok(mut s) = string_state.lock() {
        *s = None;
    }
    if let Ok(mut img) = image_state.lock()
        && img.is_some()
    {
        *img = None;
        image_id.fetch_add(1, Ordering::Release);
    }
    if let Ok(mut d) = diagram_sources_state.lock() {
        d.clear();
    }
    if let Ok(mut d) = diagram_offsets_state.lock() {
        d.clear();
    }
    current_diagram_idx_state.store(0, Ordering::Release);
}

#[allow(clippy::too_many_arguments)]
fn finish_command_preview_stream(
    mut leftover: Vec<u8>,
    try_lossy: bool,
    delay_clear: bool,
    has_swapped: bool,
    mut staged: Vec<Line<'static>>,
    current_generation: &AtomicU64,
    task_gen: u64,
    lines: &AppendOnly<Line<'static>>,
    guard: arc_swap::Guard<Arc<boxcar::Vec<Line<'static>>>>,
    changed: &std::sync::atomic::AtomicBool,
    cmd_str: &str,
    clear_other_views: &impl Fn(),
) -> bool {
    if current_generation.load(Ordering::Acquire) != task_gen {
        return true;
    }

    clean_invalid_utf8_lines(&mut leftover, try_lossy, true);
    if !leftover.is_empty() {
        match leftover.into_text() {
            Ok(text) => {
                if delay_clear && !has_swapped {
                    staged.extend(text.lines);
                } else {
                    let has_text = !text.lines.is_empty();
                    for line in text {
                        if current_generation.load(Ordering::Acquire) != task_gen
                            || lines.is_expired(&guard)
                        {
                            return true;
                        }
                        guard.push(line);
                    }
                    if has_text {
                        changed.store(true, Ordering::Release);
                    }
                }
            }
            Err(e) => {
                if try_lossy {
                    let mut has_text = false;
                    for bytes in leftover.split(|b| *b == b'\n') {
                        let line = Line::from(String::from_utf8_lossy(bytes).into_owned());
                        if delay_clear && !has_swapped {
                            staged.push(line);
                        } else {
                            if lines.is_expired(&guard) {
                                return true;
                            }
                            guard.push(line);
                            has_text = true;
                        }
                    }
                    if has_text && has_swapped {
                        changed.store(true, Ordering::Release);
                    }
                } else {
                    error!("Error displaying {cmd_str}: {:?}", e);
                    return false;
                }
            }
        }
    }

    if delay_clear && !has_swapped {
        if current_generation.load(Ordering::Acquire) != task_gen {
            return true;
        }
        lines.replace_all(staged);
        clear_other_views();
        changed.store(true, Ordering::Release);
    }

    changed.store(true, Ordering::Release);
    true
}

fn kill_child(child: &mut Child) {
    #[cfg(unix)]
    {
        use nix::sys::signal::{Signal, kill};
        use nix::unistd::Pid;

        let pgid = Pid::from_raw(-(child.id() as i32));
        let _ = kill(pgid, Signal::SIGKILL);
    }
    let _ = child.kill();
}

//     /// Try to get a key; if found, move it to the top.
//     /// If not found, replace the oldest, clear its vec, set new key.
//     pub fn try_get(&mut self, key: &str) -> bool {
//         let n = self.entries.len();

//             // shift and insert at front
//             self.order.copy_within(0..order_idx, 1);
//             self.order[0] = idx;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_try_parse_simple_command() {
        assert!(try_parse_simple_command("bat --color=always file.rs").is_some());
        assert!(try_parse_simple_command("cat /tmp/test.txt").is_some());
        assert!(try_parse_simple_command("ls -la src/").is_some());
        assert!(try_parse_simple_command("   ").is_none());

        // Metacharacters should fall back to shell (return None)
        assert!(try_parse_simple_command("bat file.rs | grep fn").is_none());
        assert!(try_parse_simple_command("echo hello > out.txt").is_none());
        assert!(try_parse_simple_command("cat file && echo ok").is_none());
        assert!(try_parse_simple_command("echo $VAR").is_none());
        assert!(try_parse_simple_command("cat `which bat`").is_none());
        assert!(try_parse_simple_command("echo 'hello world'").is_none());
        assert!(try_parse_simple_command("echo \"hello world\"").is_none());
    }

    #[tokio::test]
    #[cfg(feature = "diagram-images")]
    async fn test_previewer_standalone_mermaid_with_inline_and_media() {
        let mut previewer_cfg = crate::config::PreviewerConfig::default();
        previewer_cfg.media = true;
        previewer_cfg.markdown_diagrams = true;
        previewer_cfg.inline_diagrams = true;

        let (previewer, tx) = Previewer::new(previewer_cfg);
        let string_state = previewer.string.clone();
        let sources_state = previewer.diagram_sources.clone();

        let handle = tokio::spawn(previewer.run());

        // Write a temporary mermaid file
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("test_diag_{}.mmd", std::process::id()));
        std::fs::write(&file_path, "graph TD\n    A[Client] --> B[Server]").unwrap();

        let _env_lock = crate::utils::lock_test_env_async().await;
        unsafe {
            std::env::set_var("WM_INLINE_DIAGRAMS", "1");
            std::env::set_var("MM_INLINE_DIAGRAMS", "1");
        }

        // Send PreviewMessage::Markdown for the .mmd file
        let _ = tx.send(PreviewMessage::Markdown(
            file_path.to_string_lossy().to_string(),
            Some(80),
        ));

        // Poll string state until rendered (timeout 5 seconds)
        let start = std::time::Instant::now();
        let mut got_placeholders = false;
        while start.elapsed() < std::time::Duration::from_secs(5) {
            if let Ok(guard) = string_state.lock()
                && let Some(ref text) = *guard
            {
                let full_str: String = text
                    .lines
                    .iter()
                    .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
                    .collect();
                if full_str.contains(crate::utils::mermaid::PLACEHOLDER) {
                    got_placeholders = true;
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }

        handle.abort();
        let _ = std::fs::remove_file(&file_path);
        unsafe {
            std::env::remove_var("WM_INLINE_DIAGRAMS");
            std::env::remove_var("MM_INLINE_DIAGRAMS");
        }

        assert!(
            got_placeholders,
            "Standalone .mmd preview with media=true and inline_diagrams=true must render inline unicode placeholders"
        );
        let sources = sources_state.lock().unwrap();
        assert_eq!(
            sources.len(),
            1,
            "diagram_sources must be populated for modal diagram toggle"
        );
    }

    #[tokio::test]
    #[cfg(feature = "diagram-images")]
    async fn test_preview_transition_from_markdown_diagram_to_code_file() {
        let _env_lock = crate::utils::lock_test_env_async().await;
        let mut previewer_cfg = crate::config::PreviewerConfig::default();
        previewer_cfg.media = true;
        previewer_cfg.markdown_diagrams = true;
        previewer_cfg.delay_clear = true;

        previewer_cfg.debounce_ms = 0;

        let (previewer, tx) = Previewer::new(previewer_cfg);
        let view = previewer.view();
        let sources_state = previewer.diagram_sources.clone();
        let image_state = previewer.image.clone();

        let handle = tokio::spawn(previewer.run());

        // 1. Create a markdown file with mermaid diagram
        let temp_dir = std::env::temp_dir();
        let md_path = temp_dir.join(format!("test_stuck_{}.md", std::process::id()));
        std::fs::write(
            &md_path,
            "# Doc\n\n```mermaid\ngraph LR\n    A[Start] --> B[End]\n```\n",
        )
        .unwrap();

        let preview_cfg = crate::config::PreviewConfig {
            media: crate::config::PreviewMediaConfig {
                active: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut ui = crate::ui::PreviewUI::new(view.clone(), preview_cfg, [80, 24]);
        ui.set_title(Some("test_stuck.md".to_string()));

        // Send PreviewMessage::Markdown
        let _ = tx.send(PreviewMessage::Markdown(
            md_path.to_string_lossy().to_string(),
            Some(80),
        ));

        // Wait until diagram sources or image is populated (timeout 5s)
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_secs(5) {
            if sources_state.lock().is_ok_and(|guard| !guard.is_empty()) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }

        assert!(
            ui.has_diagram(),
            "Diagram mode must be available for markdown with diagram"
        );
        assert!(
            image_state.lock().unwrap().is_some(),
            "Diagram image must be generated in image_state when media=true"
        );

        // PreviewUI gets the image state (as would happen when rendering preview)
        let state = ui.get_image_state();
        assert!(
            state.is_some(),
            "State must be available for markdown diagram"
        );

        // 2. Now user moves cursor to a code file: title changes and PreviewMessage::Run is dispatched
        ui.set_title(Some("main.rs".to_string()));
        assert!(
            ui.image_state.is_none(),
            "Changing file selection (set_title) must immediately clear cached image_state"
        );

        let _ = tx.send(PreviewMessage::Run(
            "echo 'fn main() { println!(\"hello\"); }'".to_string(),
            vec![].into(),
        ));

        // Wait until lines are populated from command (timeout 5s)
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_secs(5) {
            if view.lines_len() > 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }

        handle.abort();
        let _ = std::fs::remove_file(&md_path);

        // 3. Verify ALL image and diagram states are completely cleared
        assert_eq!(
            view.diagram_sources.lock().unwrap().len(),
            0,
            "diagram_sources must be cleared when switching to a code file"
        );
        assert!(
            !ui.has_diagram(),
            "ui.has_diagram must be false on code file"
        );
        assert!(
            view.image.lock().unwrap().is_none(),
            "view.image must be None on code file"
        );
        assert!(
            ui.get_image_state().is_none(),
            "ui.get_image_state() must return None on code file, never frozen previous image!"
        );
        assert!(
            !ui.has_markdown(),
            "ui.has_markdown must be false on code file"
        );
    }

    #[tokio::test]
    async fn test_media_preview_cooperative_cancellation() {
        let _env_lock = crate::utils::lock_test_env_async().await;
        let mut previewer_cfg = crate::config::PreviewerConfig::default();
        previewer_cfg.media = true;
        previewer_cfg.debounce_ms = 0;

        let (previewer, tx) = Previewer::new(previewer_cfg);
        let _view = previewer.view();
        let image_state = previewer.image.clone();

        let handle = tokio::spawn(previewer.run());

        // Create temporary test images of distinct dimensions
        let temp_dir = std::env::temp_dir();
        let p1 = temp_dir.join(format!("test_canc_1_{}.png", std::process::id()));
        let p2 = temp_dir.join(format!("test_canc_2_{}.png", std::process::id()));
        let p3 = temp_dir.join(format!("test_canc_3_{}.png", std::process::id()));

        image::RgbImage::new(10, 10).save(&p1).unwrap();
        image::RgbImage::new(20, 20).save(&p2).unwrap();
        image::RgbImage::new(30, 30).save(&p3).unwrap();

        // 1. Send p1, p2, p3 in rapid succession simulating rapid keyboard scroll
        let _ = tx.send(PreviewMessage::Media(p1.to_string_lossy().to_string()));
        let _ = tx.send(PreviewMessage::Media(p2.to_string_lossy().to_string()));
        let _ = tx.send(PreviewMessage::Media(p3.to_string_lossy().to_string()));

        // Poll until image is populated (timeout 5s)
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_secs(5) {
            if let Ok(guard) = image_state.lock()
                && let Some(ref img) = *guard
                && img.width() == 30
                && img.height() == 30
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }

        // Verify final image corresponds to p3 (width 30), not stale p1 or p2
        {
            let guard = image_state.lock().unwrap();
            let img = guard.as_ref().expect("Image should be loaded for p3");
            assert_eq!(img.width(), 30);
            assert_eq!(img.height(), 30);
        }

        // 2. Now send Media followed immediately by Unset: should cooperatively cancel and clear
        let _ = tx.send(PreviewMessage::Media(p1.to_string_lossy().to_string()));
        let _ = tx.send(PreviewMessage::Unset);

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        {
            let guard = image_state.lock().unwrap();
            assert!(
                guard.is_none(),
                "Unset must cancel in-flight media and clear image_state"
            );
        }

        handle.abort();
        let _ = std::fs::remove_file(&p1);
        let _ = std::fs::remove_file(&p2);
        let _ = std::fs::remove_file(&p3);
    }

    #[tokio::test]
    async fn test_previewer_run_command() {
        let mut previewer_cfg = crate::config::PreviewerConfig::default();
        previewer_cfg.delay_clear = true;
        previewer_cfg.debounce_ms = 20;

        let (previewer, tx) = Previewer::new(previewer_cfg);
        let lines = previewer.lines.clone();

        let handle = tokio::spawn(previewer.run());

        let _ = tx.send(PreviewMessage::Run(
            "echo hello world".to_string(),
            vec![].into(),
        ));
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let _ = tx.send(PreviewMessage::Run(
            "echo hello world".to_string(),
            vec![].into(),
        ));

        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_secs(2) {
            if !lines.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        handle.abort();
        assert!(
            !lines.is_empty(),
            "Lines should be populated with 'hello world'"
        );
    }
}

/// With `delay_clear`, a command preview's output is staged (the previous preview stays up)
/// until it has 500 lines or ends; a command that goes quiet this long with output staged has
/// it shown anyway, so slow and streaming commands are not blank while they run.
const STAGED_SWAP_AFTER: std::time::Duration = std::time::Duration::from_millis(200);

/// Cleans invalid UTF-8 out of the complete lines of `buf` (all of it at `eof`), so the
/// decoder never waits on bytes that can never become valid: each invalid sequence becomes
/// U+FFFD with `lossy`, and is dropped otherwise. Bytes after the last newline are left alone
/// until `eof`, since a character there may be cut by the read and complete in the next one.
fn clean_invalid_utf8_lines(buf: &mut Vec<u8>, lossy: bool, eof: bool) {
    if std::str::from_utf8(buf).is_ok() {
        return;
    }
    let end = if eof {
        buf.len()
    } else {
        match buf.iter().rposition(|&b| b == b'\n') {
            Some(nl) => nl + 1,
            None => return,
        }
    };
    if std::str::from_utf8(&buf[..end]).is_ok() {
        return;
    }
    let mut clean = Vec::with_capacity(buf.len());
    for chunk in buf[..end].utf8_chunks() {
        clean.extend_from_slice(chunk.valid().as_bytes());
        if lossy && !chunk.invalid().is_empty() {
            clean.extend_from_slice("\u{FFFD}".as_bytes());
        }
    }
    clean.extend_from_slice(&buf[end..]);
    *buf = clean;
}

#[cfg(test)]
mod clean_utf8_tests {
    use super::clean_invalid_utf8_lines;

    #[test]
    fn complete_lines_are_cleaned_and_a_cut_tail_is_kept() {
        let mut lossy = b"ok\nbad \xff byte\n\xe2\x82".to_vec(); // ends in a cut euro sign
        clean_invalid_utf8_lines(&mut lossy, true, false);
        assert_eq!(
            lossy,
            "ok\nbad \u{FFFD} byte\n"
                .bytes()
                .chain([0xe2, 0x82])
                .collect::<Vec<_>>()
        );

        let mut strict = b"bad \xff byte\n".to_vec();
        clean_invalid_utf8_lines(&mut strict, false, false);
        assert_eq!(strict, b"bad  byte\n");

        let mut no_newline = b"bad \xff".to_vec(); // may still be mid-line: wait
        clean_invalid_utf8_lines(&mut no_newline, true, false);
        assert_eq!(no_newline, b"bad \xff");
        clean_invalid_utf8_lines(&mut no_newline, true, true); // at EOF: clean
        assert_eq!(no_newline, "bad \u{FFFD}".as_bytes());
    }
}
