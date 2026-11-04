//! Interrupt and event handlers registered by [`start`](super::start). Each returns the closure
//! `start` registers; what the closure captures is passed in explicitly.

use super::*;
use crate::config::PathRule;
use std::path::PathBuf;
use waymaker::{ConfigMMInnerItem, ConfigMMItem, config::WalkerConfig};

pub(super) type CliFormatter = waymaker::AttachmentFormatter<ConfigMMItem, ConfigMMInnerItem>;
/// Item lists of recently visited directories, so going back to one is instant.
pub(super) type SpecCache = Arc<Mutex<HashMap<PathBuf, Vec<String>>>>;
pub(super) type CliState<'a, 'b> = MMState<'a, 'b, ConfigMMItem, ConfigMMInnerItem>;

/// `ChDir(template)`: changes the working directory to the formatted path (a file means its
/// parent), selects the directory just left when going up, remembers each directory's query in
/// navigation mode, and caches the old directory's items in `spec_cache`.
pub(super) fn chdir_handler(
    formatter: CliFormatter,
    spec_cache: SpecCache,
    render_tx: RenderSender<MMAction>,
) -> impl FnMut(&mut CliState<'_, '_>) + 'static {
    let mut history: std::collections::HashMap<std::path::PathBuf, String> =
        std::collections::HashMap::new();
    move |state| {
        state.picker_ui.worker.nucleo.tick(15);
        let template = state.payload().clone();
        if template.is_empty() {
            return;
        }
        let mut path = use_formatter(&formatter, state, &template, None);
        if path.is_empty() {
            return;
        }

        if (path.starts_with("~/") || path == "~")
            && let Ok(home) = std::env::var("HOME")
        {
            if path == "~" {
                path = home;
            } else {
                path = format!("{home}/{}", &path[2..]);
            }
        }

        let target_path = Path::new(&path);
        let target_dir = if target_path.is_file() {
            target_path.parent().unwrap_or(target_path).to_path_buf()
        } else if target_path.is_relative() {
            let current_dir = std::env::current_dir().unwrap_or_default();
            let p = current_dir.join(target_path);
            if p.exists() {
                p
            } else {
                target_path.to_path_buf()
            }
        } else {
            target_path.to_path_buf()
        };

        let mut target_to_select = TARGET_ITEM.lock().unwrap().clone();
        if target_to_select.is_none() {
            if target_path == Path::new("..") || path == ".." {
                if let Ok(cwd) = std::env::current_dir()
                    && let Some(name) = cwd.file_name()
                {
                    target_to_select = Some(name.to_string_lossy().to_string());
                }
            } else if let Ok(cwd) = std::env::current_dir()
                && cwd.parent() == Some(&target_dir)
                && let Some(name) = cwd.file_name()
            {
                target_to_select = Some(name.to_string_lossy().to_string());
            }
        }

        let mut old_cwd = None;
        if state.ui.config.nav.active
            && let Ok(cwd) = std::env::current_dir()
        {
            history.insert(cwd.clone(), state.picker_ui.query.input.clone());

            // Save old_cwd items in speculative cache so returning to parent is instant
            let items: Vec<String> = state
                .picker_ui
                .worker
                .raw_results()
                .map(|item| state.picker_ui.worker.columns[0].raw(item).into_owned())
                .collect();
            if state.picker_ui.results.mode_index == 0
                && !items.is_empty()
                && let Ok(mut c) = spec_cache.lock()
            {
                if c.len() >= 64 {
                    c.clear();
                }
                c.insert(cwd.clone(), items);
            }

            old_cwd = Some(cwd);
        }

        log::debug!("ChDir: {}", target_dir.display());
        if let Err(e) = std::env::set_current_dir(&target_dir) {
            log::warn!("ChDir({}) failed: {e}", target_dir.display());
        } else {
            CHDIR_NEW_DIR.store(true, std::sync::atomic::Ordering::SeqCst);
            if let Some(t) = target_to_select {
                *TARGET_ITEM.lock().unwrap() = Some(t.clone());
            } else {
                TARGET_ITEM.lock().unwrap().take();
                // Entering a child directory (or new directory with no specific target):
                // Reset cursor to 0 and clear any previous reload item.
                state.picker_ui.results.cursor_jump(0);
                *PREV_RELOAD_ITEM.lock().unwrap() = None;
            }

            if let Ok(new_cwd) = std::env::current_dir() {
                let store = waymaker::frecency::FrecencyStore::open();
                let _ = store.add(&new_cwd.to_string_lossy());
                if state.ui.config.nav.active {
                    state.focus = waymaker::render::Focus::Results;
                    if state.picker_ui.query.mode_index() != 0 {
                        let _ = render_tx.send(waymaker::message::RenderCommand::Action(
                            waymaker::action::Action::Custom(crate::action::MMAction::ReloadNext(
                                Some(0),
                            )),
                        ));
                    }
                    let _ = render_tx.send(waymaker::message::RenderCommand::Action(
                        waymaker::action::Action::FocusNav,
                    ));
                    let is_parent = old_cwd
                        .as_ref()
                        .is_some_and(|old| old.starts_with(&new_cwd) && old != &new_cwd);
                    if is_parent {
                        if let Some(saved) = history.remove(&new_cwd) {
                            state.picker_ui.query.set(Some(saved), 0);
                        } else {
                            state.picker_ui.query.set(Some(String::new()), 0);
                        }
                    } else {
                        state.picker_ui.query.set(Some(String::new()), 0);
                    }
                }
            }
        }
    }
}

/// `Synced`/`Resynced`: once the matcher has the item named by `TARGET_ITEM` (the directory just
/// left, or a reload's previous item), moves the cursor to it; gives up and goes to the top when
/// the input has finished without it.
pub(super) fn synced_handler(
    render_tx: RenderSender<MMAction>,
) -> impl Fn(&mut CliState<'_, '_>, &Event) + 'static {
    move |state, _| {
        let target_opt = TARGET_ITEM.lock().unwrap().clone();

        if let Some(target) = target_opt {
            let count = state.picker_ui.worker.counts().0;
            if count == 0 {
                return;
            }
            let is_local = state.picker_ui.worker.mode_index == 0;
            let col0 = &state.picker_ui.worker.columns[0];
            let found_idx = state.picker_ui.worker.find_item_index(|raw| {
                let val = col0.raw(raw);
                is_target_item_match(&val, &target, is_local)
            });
            if let Some(i) = found_idx {
                state.picker_ui.results.cursor_jump(i as u32);
                let _ = render_tx.send(waymaker::message::RenderCommand::Action(
                    waymaker::action::Action::Pos(i as i32),
                ));
                state.needs_redraw = true;
                TARGET_ITEM.lock().unwrap().take();
            } else if !state.picker_ui.results.status.running {
                TARGET_ITEM.lock().unwrap().take();
                state.picker_ui.results.cursor_jump(0);
                let _ = render_tx.send(waymaker::message::RenderCommand::Action(
                    waymaker::action::Action::Pos(0),
                ));
                state.needs_redraw = true;
            }
        }
    }
}

/// The start command for `dir`: the base command, replaced by the command of the last matching
/// path rule (its preset's `[start.command]`, then its own override).
pub(super) fn active_command(
    rules: &[PathRule],
    default_cmd: &str,
    dir: &std::path::Path,
) -> String {
    let mut active_cmd = default_cmd.to_string();
    for rule in rules {
        if rule.path.matches(dir) {
            if let Some(ref p) = rule.preset {
                let mut preset_path = p.clone();
                if preset_path.is_relative() && preset_path.extension().is_none() {
                    let main_p = presets_path().join(&preset_path).join("main.toml");
                    preset_path = if !main_p.exists() {
                        presets_path().join(preset_path.with_extension("toml"))
                    } else {
                        main_p
                    };
                }
                if let Ok(preset_config) =
                    load_type::<PartialConfig, _>(&preset_path, |s| toml::from_str(s))
                    && let Some(cmd_setting) = preset_config.start.command.default.as_ref()
                    && !cmd_setting.command.is_empty()
                {
                    active_cmd = cmd_setting.command.clone();
                }
            }
            if let Some(ref cmd_setting) = rule.override_config.start.command.default
                && !cmd_setting.command.is_empty()
            {
                active_cmd = cmd_setting.command.clone();
            }
        }
    }
    active_cmd
}

/// `CursorChange`: when the cursor is on a directory that is not cached yet, lists it in the
/// background (with the walker or that directory's start command) into `spec_cache`, so
/// entering it shows its items at once.
pub(super) fn cursor_prefetch_handler(
    formatter: CliFormatter,
    spec_cache: SpecCache,
    get_cmd: impl Fn(&std::path::Path) -> String + 'static,
    walker_config: WalkerConfig,
    separator: char,
) -> impl Fn(&mut CliState<'_, '_>, &Event) + 'static {
    move |state, _| {
        let val = use_formatter(&formatter, state, "{=}", None);
        if val.is_empty() {
            return;
        }
        let current_dir = std::env::current_dir().unwrap_or_default();
        let target_path = current_dir.join(val.trim_end_matches('/'));
        if target_path.is_dir() {
            let is_cached = spec_cache
                .lock()
                .map(|c| c.contains_key(&target_path))
                .unwrap_or(false);
            if !is_cached {
                let cache_ref = spec_cache.clone();
                let cmd_to_run = get_cmd(&target_path);
                let target_dir = target_path.clone();
                let env_vars = state.make_env_vars();
                let walker_opts = walker_config.to_options(&target_dir);
                tokio::task::spawn_blocking(move || {
                    let lines: Option<Vec<String>> = if is_default_file_walker_command(&cmd_to_run)
                    {
                        let walker = waymaker::walker::AsyncWalker::new(walker_opts);
                        Some(walker.collect_sync())
                    } else if let Some(out) = Command::from_script(&cmd_to_run)
                        .current_dir(&target_dir)
                        .envs(env_vars)
                        .stdin(Stdio::null())
                        .args(&*COMMAND_ARGS.lock().unwrap())
                        .output()
                        ._elog()
                    {
                        let text = String::from_utf8_lossy(&out.stdout);
                        let mut l: Vec<String> =
                            text.split(separator).map(|s| s.to_string()).collect();
                        if l.last().is_some_and(|line| line.is_empty()) {
                            l.pop();
                        }
                        Some(l)
                    } else {
                        None
                    };

                    if let Some(lines) = lines
                        && let Ok(mut c) = cache_ref.lock()
                    {
                        if c.len() >= 64 {
                            c.clear();
                        }
                        c.insert(target_dir, lines);
                    }
                });
            }
        }
    }
}

/// What the `Reload` handler needs from [`start`](super::start); the field names are the
/// variable names the handler body uses.
pub(super) struct ReloadDeps<F> {
    pub reload_formatter: CliFormatter,
    pub reload_render_tx: RenderSender<MMAction>,
    /// The start command for a directory; see [`active_command`].
    pub get_active_cmd: F,
    pub spec_cache_reload: SpecCache,
    pub walker_config_reload: WalkerConfig,
    pub splitter: waymaker::nucleo::injector::SplitterFn<
        Either<Arc<str>, Box<waymaker::nucleo::Text<'static>>>,
    >,
    pub preprocess: waymaker::nucleo::injector::PreprocessOptions,
    pub group_prefix: Option<String>,
    pub separator: Option<char>,
    pub input_separator: Option<char>,
}

/// `Reload`: restarts the item list for the current directory and mode (start command, the
/// walker, its cache, frecency, pins), keeping the cursor on the previous item where it can.
pub(super) fn reload_handler<F>(deps: ReloadDeps<F>) -> impl FnMut(&mut CliState<'_, '_>) + 'static
where
    F: Fn(&std::path::Path) -> String + 'static,
{
    let ReloadDeps {
        reload_formatter,
        reload_render_tx,
        get_active_cmd,
        spec_cache_reload,
        walker_config_reload,
        splitter,
        preprocess,
        group_prefix,
        separator,
        input_separator,
    } = deps;
    move |state| {
        let current_dir = std::env::current_dir().unwrap_or_default();
        let just_chdir = CHDIR_NEW_DIR.swap(false, std::sync::atomic::Ordering::SeqCst);
        if just_chdir {
            *crate::start::PREV_RELOAD_ITEM.lock().unwrap() = None;
            state.picker_ui.results.cursor_jump(0);
        } else {
            let prev_idx = state.picker_ui.results.current_index();
            let prev_item = state
                .picker_ui
                .worker
                .get_nth(prev_idx)
                .map(|raw| state.picker_ui.worker.columns[0].raw(raw).to_string());
            *crate::start::PREV_RELOAD_ITEM.lock().unwrap() =
                Some((prev_item, prev_idx, current_dir.clone()));
        }

        let cmd = if !state.payload().is_empty() {
            use_formatter(&reload_formatter, state, state.payload(), None)
        } else {
            get_active_cmd(&current_dir)
        };

        let is_walker = is_default_file_walker_command(&cmd);
        let is_bookmarks = (is_walker && state.picker_ui.results.mode_index == 2)
            || cmd.contains("--bookmarks")
            || cmd.contains("--pins");
        let is_dirs = !is_bookmarks
            && ((is_walker && state.picker_ui.results.mode_index == 1)
                || cmd.contains("--dirs")
                || cmd.starts_with("mm list -d")
                || cmd.starts_with("wm list -d")
                || cmd.contains("frecency"));

        if is_bookmarks {
            state.picker_ui.worker.set_mode_index(2);
            state.picker_ui.worker.restart(false);
            state.reloading = true;

            let injector = state.injector();
            let injector = IndexedInjector::new_globally_indexed(injector);
            let injector = SegmentedInjector::new(injector, splitter.clone());
            let injector = AnsiInjector::new(injector, preprocess);

            let mut push_fn = inject_line(
                state.picker_ui.header.config.header_lines,
                reload_render_tx.clone(),
                injector,
                group_prefix.clone(),
            );

            state.picker_ui.selector.clear();
            let store = waymaker::frecency::FrecencyStore::open();
            let pins = store.list_pins();
            for pin in pins {
                let _ = push_fn(pin.into());
            }

            let _ = reload_render_tx.send(waymaker::message::RenderCommand::Action(
                waymaker::action::Action::Custom(crate::action::MMAction::ReloadReady(vec![])),
            ));
        } else if is_dirs {
            state.picker_ui.worker.set_mode_index(1);
            state.picker_ui.worker.restart(false);
            state.reloading = true;

            let injector = state.injector();
            let injector = IndexedInjector::new_globally_indexed(injector);
            let injector = SegmentedInjector::new(injector, splitter.clone());
            let injector = AnsiInjector::new(injector, preprocess);

            let mut push_fn = inject_line(
                state.picker_ui.header.config.header_lines,
                reload_render_tx.clone(),
                injector,
                group_prefix.clone(),
            );

            state.picker_ui.selector.clear();
            let store = waymaker::frecency::FrecencyStore::open();
            if let Ok(cwd) = std::env::current_dir() {
                let _ = store.add(&cwd.to_string_lossy());
            }
            let pinned_paths = store.list_pins();
            let pins_set: std::collections::HashSet<String> =
                pinned_paths.iter().cloned().collect();

            for path in pinned_paths {
                if std::path::Path::new(&path).is_dir() {
                    let _ = push_fn(path.into());
                }
            }

            let snapshot = store.get_snapshot();
            let mut items: Vec<(String, u32, usize)> = Vec::new();
            for (path, score) in snapshot.scores {
                if pins_set.contains(&path) {
                    continue;
                }
                let p = std::path::Path::new(&path);
                if p.is_dir() {
                    let depth = p.components().count();
                    items.push((path, score, depth));
                }
            }
            items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.2.cmp(&b.2)));
            for (path, _, _) in items {
                let _ = push_fn(path.into());
            }

            let _ = reload_render_tx.send(waymaker::message::RenderCommand::Action(
                waymaker::action::Action::Custom(crate::action::MMAction::ReloadReady(vec![])),
            ));
        } else if is_default_file_walker_command(&cmd) {
            state.picker_ui.worker.set_mode_index(0);
            let cwd_str = current_dir.to_string_lossy().to_string();

            if state.payload().is_empty()
                && let Ok(mut cache) = spec_cache_reload.lock()
                && let Some(lines) = cache.remove(&current_dir)
            {
                debug!("Speculative Cache HIT for {current_dir:?}");
                state.picker_ui.worker.restart(false);
                state.reloading = true;

                let injector = state.injector();
                let injector = IndexedInjector::new_globally_indexed(injector);
                let injector = SegmentedInjector::new(injector, splitter.clone());
                let injector = AnsiInjector::new(injector, preprocess);

                let mut push_fn = inject_line(
                    state.picker_ui.header.config.header_lines,
                    reload_render_tx.clone(),
                    injector,
                    group_prefix.clone(),
                );

                state.picker_ui.selector.clear();
                for line in lines {
                    let _ = push_fn(line.into());
                }

                let _ = reload_render_tx.send(waymaker::message::RenderCommand::Action(
                    waymaker::action::Action::Custom(crate::action::MMAction::ReloadReady(vec![])),
                ));
                return;
            }

            let ignored_set: std::collections::HashSet<String> = walker_config_reload
                .effective_ignore()
                .into_iter()
                .collect();
            let is_ignored_item = |item: &str| -> bool {
                let s = item.strip_prefix("./").unwrap_or(item);
                let s = s.strip_prefix('/').unwrap_or(s);
                let first = s.split(['/', '\\']).next().unwrap_or("");
                let clean_first = first.trim_end_matches(['/', '\\']);
                !clean_first.is_empty() && ignored_set.contains(clean_first)
            };

            let cache_store = waymaker::cache::DirCacheStore::open();

            if let Some(cached_rec) = cache_store.get_valid(&cwd_str)
                && !cached_rec.items.is_empty()
            {
                debug!("DirCacheStore HIT for {cwd_str}");
                state.picker_ui.worker.restart(false);
                state.reloading = true;

                let injector = state.injector();
                let injector = IndexedInjector::new_globally_indexed(injector);
                let injector = SegmentedInjector::new(injector, splitter.clone());
                let injector = AnsiInjector::new(injector, preprocess);

                let mut push_fn = inject_line(
                    state.picker_ui.header.config.header_lines,
                    reload_render_tx.clone(),
                    injector,
                    group_prefix.clone(),
                );

                state.picker_ui.selector.clear();
                for item in cached_rec.items {
                    if is_ignored_item(&item) {
                        continue;
                    }
                    let _ = push_fn(item.into());
                }

                let _ = reload_render_tx.send(waymaker::message::RenderCommand::Action(
                    waymaker::action::Action::Custom(crate::action::MMAction::ReloadReady(vec![])),
                ));

                // Background async walk to refresh disk cache if needed
                let cwd_clone = cwd_str.clone();
                let walker_opts = walker_config_reload.to_options(&cwd_clone);
                tokio::spawn(async move {
                    let (collect_tx, collect_rx) = std::sync::mpsc::channel();
                    let walker = waymaker::walker::AsyncWalker::new(walker_opts);
                    let handle = walker.spawn_walk(move |line| {
                        let _ = collect_tx.send(line);
                        Ok(())
                    });
                    let _ = handle.await;

                    let mut fresh_items: Vec<String> = collect_rx.into_iter().collect();
                    if !fresh_items.is_empty() {
                        fresh_items.sort_by_key(|item| {
                            let slashes = item.bytes().filter(|&b| b == b'/' || b == b'\\').count();
                            (slashes, item.clone())
                        });
                        let cache_store = waymaker::cache::DirCacheStore::open();
                        let _ = cache_store.put(&cwd_clone, fresh_items);
                    }
                });
                return;
            }

            state.picker_ui.worker.restart(false);
            state.reloading = true;

            if let Ok(mut c) = spec_cache_reload.lock() {
                c.clear();
            }

            let injector = state.injector();
            let injector = IndexedInjector::new_globally_indexed(injector);
            let injector = SegmentedInjector::new(injector, splitter.clone());
            let injector = AnsiInjector::new(injector, preprocess);

            let mut push_fn = inject_line(
                state.picker_ui.header.config.header_lines,
                reload_render_tx.clone(),
                injector,
                group_prefix.clone(),
            );

            state.picker_ui.selector.clear();
            let reload_render_tx = reload_render_tx.clone();
            let reload_render_tx_ready = reload_render_tx.clone();
            let walker_opts = walker_config_reload.to_options(".");
            tokio::task::spawn_blocking(move || {
                let (collect_tx, collect_rx) = std::sync::mpsc::channel();
                let walker = waymaker::walker::AsyncWalker::new(walker_opts);
                let mut first = true;
                let handle = walker.spawn_walk(move |line| {
                    let _ = collect_tx.send(line.clone());
                    let res = push_fn(line.into());
                    if first {
                        first = false;
                        let _ =
                            reload_render_tx_ready.send(waymaker::message::RenderCommand::Action(
                                waymaker::action::Action::Custom(
                                    crate::action::MMAction::ReloadReady(vec![]),
                                ),
                            ));
                    }
                    res
                });
                let _ = tokio::runtime::Handle::current().block_on(handle);

                let mut fresh_items: Vec<String> = collect_rx.into_iter().collect();
                if !fresh_items.is_empty() {
                    fresh_items.sort_by_key(|item| {
                        let slashes = item.bytes().filter(|&b| b == b'/' || b == b'\\').count();
                        (slashes, item.clone())
                    });
                    let cwd_str = std::env::current_dir()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let cache_store = waymaker::cache::DirCacheStore::open();
                    let _ = cache_store.put(&cwd_str, fresh_items);
                }

                if first {
                    let _ = reload_render_tx.send(waymaker::message::RenderCommand::Action(
                        waymaker::action::Action::Custom(crate::action::MMAction::ReloadReady(
                            vec![],
                        )),
                    ));
                }
            });
        } else if !cmd.is_empty() {
            state.picker_ui.worker.restart(false);
            state.reloading = true;

            let injector = state.injector();
            let injector = IndexedInjector::new_globally_indexed(injector);
            let injector = SegmentedInjector::new(injector, splitter.clone());
            let injector = AnsiInjector::new(injector, preprocess);

            let push_fn = inject_line(
                state.picker_ui.header.config.header_lines,
                reload_render_tx.clone(),
                injector,
                group_prefix.clone(),
            );

            let vars = state.make_env_vars();
            debug!("Reloading: {cmd} in {current_dir:?}");
            state.picker_ui.selector.clear();

            let separator = separator.or(input_separator).unwrap_or('\n');
            let reload_render_tx = reload_render_tx.clone();
            let cmd = cmd.clone();
            let current_dir = current_dir.clone();
            tokio::task::spawn_blocking(move || {
                if let Some(out) = Command::from_script(&cmd)
                    .current_dir(&current_dir)
                    .envs(vars)
                    .stdin(Stdio::null())
                    .args(&*COMMAND_ARGS.lock().unwrap())
                    .output()
                    ._elog()
                {
                    let text = String::from_utf8_lossy(&out.stdout);
                    let mut lines: Vec<&str> = text.split(separator).collect();
                    if lines.last() == Some(&"") {
                        lines.pop();
                    }
                    let mut push_fn = push_fn;
                    for line in lines {
                        let _ = push_fn(line.into());
                    }
                }

                let _ = reload_render_tx.send(waymaker::message::RenderCommand::Action(
                    waymaker::action::Action::Custom(crate::action::MMAction::ReloadReady(vec![])),
                ));
            });
        }
    }
}
