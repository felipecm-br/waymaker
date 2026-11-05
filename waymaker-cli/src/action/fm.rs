use super::*;

pub(super) fn handle_fm(a: MMAction, state: &mut MMState<'_, '_>, ctx: &mut ActionContext) {
    let ActionContext {
        render_tx,
        clipboard,
        fm_notify,
        undo_stack,
        redo_stack,
        fm_action,
        ..
    } = ctx;
    match a {
        MMAction::FmSetYankPaths(raw) => {
            state.picker_ui.results.yank_paths = raw
                .split('\n')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            state.needs_redraw = true;
        }
        MMAction::FmRemoveYankPaths(raw) => {
            let cwd = std::env::current_dir().unwrap_or_default();
            let to_remove: Vec<String> = raw
                .split('\n')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            state.picker_ui.results.yank_paths.retain(|item| {
                !to_remove.iter().any(|rem| {
                    let rem_trimmed = rem.trim_end_matches('/').trim_end_matches('\\');
                    let item_trimmed = item.trim_end_matches('/').trim_end_matches('\\');
                    if rem_trimmed == item_trimmed {
                        return true;
                    }
                    let rem_abs = if std::path::Path::new(rem_trimmed).is_absolute() {
                        rem_trimmed.to_string()
                    } else {
                        cwd.join(rem_trimmed).to_string_lossy().to_string()
                    };
                    let item_abs = if std::path::Path::new(item_trimmed).is_absolute() {
                        item_trimmed.to_string()
                    } else {
                        cwd.join(item_trimmed).to_string_lossy().to_string()
                    };
                    rem_abs == item_abs
                })
            });
            state.needs_redraw = true;
        }
        MMAction::FmRemoveCutPaths(raw) => {
            let cwd = std::env::current_dir().unwrap_or_default();
            let to_remove: Vec<String> = raw
                .split('\n')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            state.picker_ui.results.cut_paths.retain(|item| {
                !to_remove.iter().any(|rem| {
                    let rem_trimmed = rem.trim_end_matches('/').trim_end_matches('\\');
                    let item_trimmed = item.trim_end_matches('/').trim_end_matches('\\');
                    if rem_trimmed == item_trimmed {
                        return true;
                    }
                    let rem_abs = if std::path::Path::new(rem_trimmed).is_absolute() {
                        rem_trimmed.to_string()
                    } else {
                        cwd.join(rem_trimmed).to_string_lossy().to_string()
                    };
                    let item_abs = if std::path::Path::new(item_trimmed).is_absolute() {
                        item_trimmed.to_string()
                    } else {
                        cwd.join(item_trimmed).to_string_lossy().to_string()
                    };
                    rem_abs == item_abs
                })
            });
            state.needs_redraw = true;
        }
        MMAction::FmSetCutPaths(raw) => {
            state.picker_ui.results.cut_paths = raw
                .split('\n')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            state.needs_redraw = true;
        }
        MMAction::FmSetPinPaths(raw) => {
            state.picker_ui.results.pin_paths = raw
                .split('\n')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            state.needs_redraw = true;
        }
        MMAction::FmSetFlashCopy(raw) => {
            state
                .picker_ui
                .results
                .set_flash_target(raw, waymaker::ui::results::FlashOp::Copy);
            state.needs_redraw = true;
        }
        MMAction::FmSetFlashCut(raw) => {
            state
                .picker_ui
                .results
                .set_flash_target(raw, waymaker::ui::results::FlashOp::Cut);
            state.needs_redraw = true;
        }
        MMAction::FmCreateStart => {
            *fm_action = Some(FmActionMode::Create);
            show_action_box(state, " ", "");
        }
        MMAction::FmDeleteStart => {
            let paths = fm_current_items(state);
            if !paths.is_empty() {
                let label = if paths.len() > 1 {
                    format!(
                        "{{red:}} Delete {} selected items? (Enter/Esc)",
                        paths.len()
                    )
                } else {
                    format!("{{red:}} {}? (Enter/Esc)", paths[0])
                };
                *fm_action = Some(FmActionMode::Delete { paths });
                show_styled_action_box(state, &label, "");
            }
        }
        MMAction::FmRenameStart => {
            let mut items = fm_current_items(state);
            if !items.is_empty() {
                let from = items.remove(0);
                *fm_action = Some(FmActionMode::Rename {
                    from: from.clone(),
                    remaining: items,
                });
                show_action_box(state, "󰑕 ", from.trim_end_matches('/'));
            }
        }
        MMAction::FmUnzipStart => {
            if let Some(src) = fm_current_items(state).into_iter().next() {
                *fm_action = Some(FmActionMode::Unzip { src: src.clone() });
                show_action_box(state, "󰋺 ", crate::fm::archive_stem(&src));
            }
        }
        MMAction::FmZipStart => {
            let paths = fm_current_items(state);
            if !paths.is_empty() {
                let default_name = if paths.len() == 1 {
                    format!("{}.zip", crate::fm::archive_stem(&paths[0]))
                } else {
                    "archive.zip".to_string()
                };
                *fm_action = Some(FmActionMode::Zip { paths });
                show_action_box(state, "󰋪 ", &default_name);
            }
        }
        MMAction::FmYank => fm_yank(state, render_tx, clipboard, fm_notify),
        MMAction::FmUnyank => {
            let items = fm_current_items(state);
            if !items.is_empty() {
                // If it's in the clipboard, do we remove it?
                // For simplicity, we just remove it from yank_paths visually.
                // Or we can remove it from the clipboard if the clipboard exactly matches?
                // The primary goal is visual unyanking.
                let _ = render_tx.send(RenderCommand::Action(Action::Custom(
                    MMAction::FmRemoveYankPaths(items.join("\n")),
                )));
                let _ = render_tx.send(RenderCommand::Action(Action::Custom(
                    MMAction::FmRemoveCutPaths(items.join("\n")),
                )));
                if *fm_notify {
                    let msg = fm_notify_msg("Un-yanked", &items, "yellow");
                    show_styled_info_box(state, &msg);
                }
            }
        }
        MMAction::FmUncut => {
            let items = fm_current_items(state);
            if !items.is_empty() {
                let _ = render_tx.send(RenderCommand::Action(Action::Custom(
                    MMAction::FmRemoveYankPaths(items.join("\n")),
                )));
                let _ = render_tx.send(RenderCommand::Action(Action::Custom(
                    MMAction::FmRemoveCutPaths(items.join("\n")),
                )));
                if *fm_notify {
                    let msg = fm_notify_msg("Un-cut", &items, "red");
                    show_styled_info_box(state, &msg);
                }
            }
        }
        MMAction::FmCut => fm_cut(state, render_tx, clipboard, fm_notify),
        MMAction::FmPaste => fm_paste(state, render_tx, clipboard, fm_notify, undo_stack),
        MMAction::FmPasteInto => fm_paste_into(state, render_tx, clipboard, undo_stack),
        MMAction::FmUndo => fm_undo(state, render_tx, clipboard, undo_stack, redo_stack),
        MMAction::FmRedo => fm_redo(state, render_tx, clipboard, undo_stack, redo_stack),
        MMAction::FmDragDrop => {
            let paths = fm_current_items(state);
            if !paths.is_empty() {
                let cwd = std::env::current_dir().unwrap_or_default();
                let mut cmd = std::process::Command::new("ripdrag");
                cmd.args(&paths);
                cmd.current_dir(cwd);
                if let Err(e) = cmd.spawn() {
                    log::error!("fm dragdrop: {e}");
                }
            }
        }
        MMAction::FmTogglePin => fm_toggle_pin(state, render_tx),
        _ => unreachable!("handle_fm got a non-fm action"),
    }
}

pub(super) fn show_action_box(state: &mut MMState<'_, '_>, prompt: &str, initial: &str) {
    state.picker_ui.action_visible = true;
    waymaker::ACTION_BOX_ACTIVE.store(true, std::sync::atomic::Ordering::Relaxed);
    state.picker_ui.action.set(Some(initial.to_string()), 0);
    state
        .picker_ui
        .action
        .set_prompt_line(StatusUI::parse_template_to_status_line(prompt));
}

pub(super) fn show_styled_action_box(state: &mut MMState<'_, '_>, prompt: &str, initial: &str) {
    state.picker_ui.action_visible = true;
    waymaker::ACTION_BOX_ACTIVE.store(true, std::sync::atomic::Ordering::Relaxed);
    state.picker_ui.action.set(Some(initial.to_string()), 0);
    state
        .picker_ui
        .action
        .set_prompt_line(StatusUI::parse_template_to_status_line(prompt));
}

pub(super) fn show_styled_info_box(state: &mut MMState<'_, '_>, message: &str) {
    state.picker_ui.action_visible = true;
    waymaker::ACTION_BOX_ACTIVE.store(false, std::sync::atomic::Ordering::Relaxed);
    state.picker_ui.action.set(Some(String::new()), 0);
    state
        .picker_ui
        .action
        .set_prompt_line(StatusUI::parse_template_to_status_line(message));
    state.needs_redraw = true;
}

pub(crate) fn abbreviate_target_dir_display(dest_display_name: &str) -> String {
    let clean = dest_display_name
        .trim()
        .trim_start_matches("./")
        .trim_end_matches('/')
        .trim_end_matches('\\');

    let p = std::path::Path::new(clean);
    let segments: Vec<&str> = p
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .collect();

    if segments.len() > 3 {
        let n = segments.len();
        format!(
            ".../{}/{}/{}",
            segments[n - 3],
            segments[n - 2],
            segments[n - 1]
        )
    } else if p.is_absolute() {
        format!("{clean}/")
    } else {
        format!("./{clean}/")
    }
}

pub(super) fn close_action_box(state: &mut MMState<'_, '_>, fm_action: &mut Option<FmActionMode>) {
    state.picker_ui.action_visible = false;
    waymaker::ACTION_BOX_ACTIVE.store(false, std::sync::atomic::Ordering::Relaxed);
    state.picker_ui.action.set(Some(String::new()), 0);
    state.picker_ui.action.set_prompt(None);
    *fm_action = None;
}

pub(super) fn commit_fm_action(
    state: &mut MMState<'_, '_>,
    render_tx: &waymaker::event::RenderSender<MMAction>,
    undo_stack: &crate::fm::UndoStack,
    _redo_stack: &crate::fm::UndoStack,
    fm_action: &mut Option<FmActionMode>,
    fm_notify: bool,
) {
    let input = state.picker_ui.action.input.trim().to_string();
    let Some(mode) = fm_action.clone() else {
        close_action_box(state, fm_action);
        return;
    };

    match mode {
        FmActionMode::Create => {
            if !input.is_empty() {
                let result = if input.ends_with('/') {
                    std::fs::create_dir_all(&input)
                } else {
                    if let Some(parent) = std::path::Path::new(&input).parent()
                        && !parent.as_os_str().is_empty()
                    {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    std::fs::File::create(&input).map(|_| ())
                };
                close_action_box(state, fm_action);
                if let Err(e) = result {
                    error!("fm create '{input}': {e}");
                } else {
                    if let Ok(mut stack) = undo_stack.lock() {
                        stack.push(crate::fm::UndoAction::CreatedFile {
                            path: PathBuf::from(&input),
                        });
                    }
                    if fm_notify {
                        let msg = format!("{{green:Created:}} {}", input);
                        show_styled_info_box(state, &msg);
                    }
                }
            } else {
                close_action_box(state, fm_action);
            }
        }
        FmActionMode::Delete { paths } => {
            let mut deleted_items = Vec::new();
            let mut deleted_names = Vec::new();
            for path in &paths {
                let path_buf = PathBuf::from(path);
                match crate::fm::move_to_trash(&path_buf) {
                    Ok(backup) => {
                        deleted_names.push(path.clone());
                        deleted_items.push((path_buf, backup));
                    }
                    Err(e) => error!("fm delete '{}': {e}", path),
                }
            }
            if !deleted_items.is_empty()
                && let Ok(mut stack) = undo_stack.lock()
            {
                stack.push(crate::fm::UndoAction::DeletedFiles {
                    items: deleted_items,
                });
            }
            close_action_box(state, fm_action);
            if fm_notify && !deleted_names.is_empty() {
                let msg = fm_notify_msg("Deleted", &deleted_names, "red");
                show_styled_info_box(state, &msg);
            }
        }
        FmActionMode::Rename { from, remaining } => {
            if !input.is_empty() && input != from {
                if let Err(e) =
                    crate::fm::move_path(std::path::Path::new(&from), std::path::Path::new(&input))
                {
                    error!("fm rename '{}' -> '{input}': {e}", from);
                } else if let Ok(mut stack) = undo_stack.lock() {
                    stack.push(crate::fm::UndoAction::Renamed {
                        from: PathBuf::from(&from),
                        to: PathBuf::from(&input),
                    });
                }
            }

            if !remaining.is_empty() {
                let mut remaining_mut = remaining.clone();
                let next_from = remaining_mut.remove(0);
                *fm_action = Some(FmActionMode::Rename {
                    from: next_from.clone(),
                    remaining: remaining_mut,
                });
                state
                    .picker_ui
                    .action
                    .set(Some(next_from.trim_end_matches('/').to_string()), 0);
                state.picker_ui.action_visible = true;
                waymaker::ACTION_BOX_ACTIVE.store(true, std::sync::atomic::Ordering::Relaxed);
                let _ = render_tx.send(RenderCommand::Action(Action::Reload(String::new())));
                return;
            }

            close_action_box(state, fm_action);
            if fm_notify && !input.is_empty() && input != from {
                let msg = format!("{{cyan:Renamed:}} {} -> {}", from, input);
                show_styled_info_box(state, &msg);
            }
        }
        FmActionMode::Unzip { src } => {
            if !input.is_empty() {
                let res = if let Err(e) = std::fs::create_dir_all(&input) {
                    Err(e)
                } else {
                    crate::fm::extract_archive(&src, &input)
                };
                close_action_box(state, fm_action);
                match res {
                    Ok(_) => {
                        if fm_notify {
                            let msg = format!("{{green:Extracted:}} {} -> {}", src, input);
                            show_styled_info_box(state, &msg);
                        }
                    }
                    Err(e) => error!("fm unzip '{src}' -> '{input}': {e}"),
                }
            } else {
                close_action_box(state, fm_action);
            }
        }
        FmActionMode::Zip { paths } => {
            if !input.is_empty() {
                let res = crate::fm::create_archive(&input, &paths);
                close_action_box(state, fm_action);
                match res {
                    Ok(_) => {
                        if fm_notify {
                            let msg = format!("{{green:Compressed:}} {}", input);
                            show_styled_info_box(state, &msg);
                        }
                    }
                    Err(e) => error!("fm zip '{input}': {e}"),
                }
            } else {
                close_action_box(state, fm_action);
            }
        }
        FmActionMode::Custom { command } => {
            if !command.is_empty() {
                let final_cmd = command.replace("{input}", &input);
                let vars = state.make_env_vars();
                if let Some(mut child) = Command::from_script(&final_cmd)
                    .envs(vars)
                    .stdin(crate::register::maybe_tty_in())
                    .stdout(crate::register::maybe_tty_out())
                    .stderr(crate::register::maybe_tty_out())
                    ._spawn()
                {
                    let _ = child.wait();
                }
            }
            close_action_box(state, fm_action);
        }
    }

    let _ = render_tx.send(RenderCommand::Action(Action::Reload(String::new())));
}

pub(super) fn fm_current_items(state: &MMState<'_, '_>) -> Vec<String> {
    state.map_selected_to_vec(|_, x| x.to_cow().to_string())
}

pub(super) fn fm_notify_msg(verb: &str, names: &[String], style: &str) -> String {
    let count = names.len();
    if count == 0 {
        return String::new();
    }
    let trimmed = style.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') && trimmed.contains(':') {
        let inner = &trimmed[1..trimmed.len() - 1];
        if let Some((color, icon)) = inner.split_once(':') {
            return match count {
                1 => format!("{{{color}:{icon} {verb}: {}}}", names[0]),
                n => format!("{{{color}:{icon} {verb}: {n} items ({})}}", names[0]),
            };
        }
    }
    let color_name = trimmed.trim_matches('{').trim_matches('}');
    match count {
        1 => format!("{{{color_name}:{verb}: {}}}", names[0]),
        n => format!("{{{color_name}:{verb}: {n} items ({})}}", names[0]),
    }
}

/// `FmTogglePin`: bookmarks or un-bookmarks the focused path.
fn fm_toggle_pin(
    state: &mut MMState<'_, '_>,
    render_tx: &mut waymaker::event::RenderSender<MMAction>,
) {
    let mut paths = fm_current_items(state);
    if paths.is_empty()
        && let Ok(cwd) = std::env::current_dir()
    {
        paths.push(cwd.to_string_lossy().to_string());
    }
    if !paths.is_empty() {
        let store = waymaker::frecency::FrecencyStore::open();
        let mut last_state = false;
        for p in &paths {
            if let Ok(pinned) = store.toggle_pin(p) {
                last_state = pinned;
                let key_str = waymaker::frecency::normalize_path(p);
                if pinned {
                    state.picker_ui.results.pin_paths.insert(key_str);
                } else {
                    state.picker_ui.results.pin_paths.remove(&key_str);
                    state.picker_ui.results.pin_paths.remove(p);
                }
            }
        }
        let verb = if last_state {
            "Bookmarked"
        } else {
            "Unbookmarked"
        };
        let is_dir = paths
            .first()
            .map(|p| std::path::Path::new(p).is_dir())
            .unwrap_or(false);
        let icon_str = if is_dir {
            "{yellow:󰮟}"
        } else {
            "{yellow:󱀻}"
        };
        let color = if last_state { icon_str } else { "{darkgray}" };
        let msg = fm_notify_msg(verb, &paths, color);
        show_styled_info_box(state, &msg);
        let _ = render_tx.send(RenderCommand::Action(Action::Redraw));
        let _ = render_tx.send(RenderCommand::Refresh);
    }
}

/// `FmRedo`: re-applies the last undone file operation.
fn fm_redo(
    state: &mut MMState<'_, '_>,
    render_tx: &mut waymaker::event::RenderSender<MMAction>,
    clipboard: &mut crate::fm::Clipboard,
    undo_stack: &mut crate::fm::UndoStack,
    redo_stack: &mut crate::fm::UndoStack,
) {
    let action = redo_stack.lock().ok().and_then(|mut s| s.pop());
    if let Some(action) = action {
        if let Err(e) = crate::fm::apply_redo(&action) {
            error!("fm redo: {e}");
        }

        if let crate::fm::UndoAction::Paste {
            ref items,
            ref target_dir,
            ..
        } = action
        {
            if let Ok(mut cb) = clipboard.lock() {
                *cb = None;
            }
            state.picker_ui.results.yank_paths.clear();
            state.picker_ui.results.cut_paths.clear();
            let _ = render_tx.send(RenderCommand::Action(Action::Custom(
                MMAction::FmSetYankPaths(String::new()),
            )));
            let _ = render_tx.send(RenderCommand::Action(Action::Custom(
                MMAction::FmSetCutPaths(String::new()),
            )));
            if let Some(target) = target_dir {
                if let Some((_, dest)) = items.first()
                    && let Some(item_name) = dest.file_name()
                {
                    let name = item_name.to_string_lossy().to_string();
                    *crate::start::TARGET_ITEM.lock().unwrap() = Some(name.clone());
                }
                let _ = render_tx.send(RenderCommand::Action(Action::ChDir(
                    target.to_string_lossy().to_string(),
                )));
            }
        }

        let msg = match &action {
            crate::fm::UndoAction::Paste { op, items, .. } => {
                let count = items.len();
                let item_word = if count == 1 { "item" } else { "items" };
                let op_word = match op {
                    crate::fm::ClipOp::Copy => "pasted",
                    crate::fm::ClipOp::Cut => "moved",
                };
                format!("{{cyan:󰑖 Redone {op_word} of {count} {item_word}}}")
            }
            _ => "{cyan:󰑖 Redone last action}".to_string(),
        };

        if let Ok(mut us) = undo_stack.lock() {
            us.push(action);
        }

        show_styled_info_box(state, &msg);
        let _ = render_tx.send(RenderCommand::Action(Action::Reload(String::new())));
    }
}

/// `FmUndo`: reverts the last file operation (paste, move, delete, rename).
fn fm_undo(
    state: &mut MMState<'_, '_>,
    render_tx: &mut waymaker::event::RenderSender<MMAction>,
    clipboard: &mut crate::fm::Clipboard,
    undo_stack: &mut crate::fm::UndoStack,
    redo_stack: &mut crate::fm::UndoStack,
) {
    let action = undo_stack.lock().ok().and_then(|mut s| s.pop());
    if let Some(action) = action {
        if let Err(e) = crate::fm::apply_undo(&action) {
            error!("fm undo: {e}");
        }

        if let crate::fm::UndoAction::Paste {
            ref saved_clipboard,
            ref saved_yank_paths,
            ref saved_cut_paths,
            ref previous_dir,
            ref target_dir,
            ..
        } = action
        {
            if let Ok(mut cb) = clipboard.lock() {
                *cb = saved_clipboard.clone();
            }
            state.picker_ui.results.yank_paths = saved_yank_paths.clone();
            state.picker_ui.results.cut_paths = saved_cut_paths.clone();
            let yanks: Vec<String> = saved_yank_paths.iter().cloned().collect();
            let cuts: Vec<String> = saved_cut_paths.iter().cloned().collect();
            let _ = render_tx.send(RenderCommand::Action(Action::Custom(
                MMAction::FmSetYankPaths(yanks.join("\n")),
            )));
            let _ = render_tx.send(RenderCommand::Action(Action::Custom(
                MMAction::FmSetCutPaths(cuts.join("\n")),
            )));
            if let Some(prev) = previous_dir {
                if let Some(target) = target_dir
                    && let Some(target_name) = target.file_name()
                {
                    let name = target_name.to_string_lossy().to_string();
                    *crate::start::TARGET_ITEM.lock().unwrap() = Some(name.clone());
                }
                let _ = render_tx.send(RenderCommand::Action(Action::ChDir(
                    prev.to_string_lossy().to_string(),
                )));
            }
        }

        let msg = match &action {
            crate::fm::UndoAction::Paste { op, items, .. } => {
                let count = items.len();
                let item_word = if count == 1 { "item" } else { "items" };
                let op_word = match op {
                    crate::fm::ClipOp::Copy => "copy",
                    crate::fm::ClipOp::Cut => "move",
                };
                format!("{{yellow:󰕌 Undone {op_word} of {count} {item_word} (clipboard restored)}}")
            }
            crate::fm::UndoAction::DeletedFiles { items } => {
                let count = items.len();
                let item_word = if count == 1 { "item" } else { "items" };
                format!("{{yellow:󰕌 Restored {count} {item_word}}}")
            }
            crate::fm::UndoAction::CreatedFile { path } => {
                format!("{{yellow:󰕌 Undone creation of: {}}}", path.display())
            }
            crate::fm::UndoAction::Renamed { from, to } => {
                format!(
                    "{{yellow:󰕌 Reverted rename: {} -> {}}}",
                    to.display(),
                    from.display()
                )
            }
        };

        if let Ok(mut rs) = redo_stack.lock() {
            rs.push(action);
        }

        show_styled_info_box(state, &msg);
        let _ = render_tx.send(RenderCommand::Action(Action::Reload(String::new())));
    }
}

/// `FmPasteInto`: pastes the clipboard into the focused directory.
fn fm_paste_into(
    state: &mut MMState<'_, '_>,
    render_tx: &mut waymaker::event::RenderSender<MMAction>,
    clipboard: &mut crate::fm::Clipboard,
    undo_stack: &mut crate::fm::UndoStack,
) {
    let clip = clipboard.lock().ok().and_then(|g| g.clone());
    if let Some(clip) = clip {
        let cwd = std::env::current_dir().unwrap_or_default();
        let focused_path = fm_current_items(state).into_iter().next().or_else(|| {
            state.current_raw().and_then(|item| {
                state
                    .picker_ui
                    .worker
                    .columns
                    .first()
                    .map(|c| c.raw(item).into_owned())
            })
        });

        let (dest_dir, is_subfolder, dest_display_name) = if let Some(ref raw) = focused_path {
            let p = std::path::Path::new(raw);
            let abs = if p.is_absolute() {
                p.to_path_buf()
            } else {
                cwd.join(p)
            };
            let clean = raw.trim().trim_end_matches('/').trim_end_matches('\\');
            if abs.is_dir() && clean != "." && clean != ".." && abs != cwd {
                (abs, true, raw.clone())
            } else {
                (cwd.clone(), false, String::new())
            }
        } else {
            (cwd.clone(), false, String::new())
        };

        let mut had_error = false;
        let mut pasted_items = Vec::new();

        for src in &clip.items {
            let result = match clip.op {
                crate::fm::ClipOp::Copy => crate::fm::copy_into(src, &dest_dir),
                crate::fm::ClipOp::Cut => crate::fm::move_into(src, &dest_dir),
            };
            match result {
                Ok(dest) => {
                    pasted_items.push((src.clone(), dest));
                }
                Err(e) => {
                    error!("fm paste into '{}': {e}", src.display());
                    had_error = true;
                }
            }
        }

        if !pasted_items.is_empty()
            && let Ok(mut u) = undo_stack.lock()
        {
            u.push(crate::fm::UndoAction::Paste {
                op: clip.op,
                items: pasted_items.clone(),
                saved_clipboard: Some(clip.clone()),
                saved_yank_paths: state.picker_ui.results.yank_paths.clone(),
                saved_cut_paths: state.picker_ui.results.cut_paths.clone(),
                previous_dir: if is_subfolder {
                    Some(cwd.clone())
                } else {
                    None
                },
                target_dir: if is_subfolder {
                    Some(dest_dir.clone())
                } else {
                    None
                },
            });
        }

        if !had_error {
            let store = waymaker::frecency::FrecencyStore::open();
            let _ = store.add(&dest_dir.to_string_lossy());

            if let Ok(mut cb) = clipboard.lock() {
                *cb = None;
            }
            let _ = render_tx.send(RenderCommand::Action(Action::Custom(
                MMAction::FmSetCutPaths(String::new()),
            )));
            let _ = render_tx.send(RenderCommand::Action(Action::Custom(
                MMAction::FmSetYankPaths(String::new()),
            )));
            state.picker_ui.results.cut_paths.clear();
            state.picker_ui.results.yank_paths.clear();
        }

        if is_subfolder {
            let action = match clip.op {
                crate::fm::ClipOp::Copy => MMAction::FmSetFlashCopy(dest_display_name.clone()),
                crate::fm::ClipOp::Cut => MMAction::FmSetFlashCut(dest_display_name.clone()),
            };
            let _ = render_tx.send(RenderCommand::Action(Action::Custom(action)));
        }

        let count = clip.items.len();
        let item_word = if count == 1 { "item" } else { "items" };
        let (verb, icon) = match clip.op {
            crate::fm::ClipOp::Copy => ("Pasted", "󰆏"),
            crate::fm::ClipOp::Cut => ("Moved", "󰆐"),
        };
        let color = if had_error { "red" } else { "cyan" };
        let msg = if is_subfolder {
            let target_display = abbreviate_target_dir_display(&dest_display_name);
            format!(
                "{{{color}:{icon} {verb} {count} {item_word} into {target_display} (u to undo)}}"
            )
        } else {
            format!("{{{color}:{icon} {verb} {count} {item_word} into current folder (u to undo)}}")
        };
        show_styled_info_box(state, &msg);

        if is_subfolder && !pasted_items.is_empty() {
            if let Some((_, dest)) = pasted_items.first()
                && let Some(item_name) = dest.file_name()
            {
                let name = item_name.to_string_lossy().to_string();
                *crate::start::TARGET_ITEM.lock().unwrap() = Some(name.clone());
            }
            let _ = render_tx.send(RenderCommand::Action(Action::ChDir(
                dest_dir.to_string_lossy().to_string(),
            )));
        }
        let _ = render_tx.send(RenderCommand::Action(Action::Reload(String::new())));
    }
}

/// `FmPaste`: pastes the clipboard (copy or move) into the current directory.
fn fm_paste(
    state: &mut MMState<'_, '_>,
    render_tx: &mut waymaker::event::RenderSender<MMAction>,
    clipboard: &mut crate::fm::Clipboard,
    fm_notify: &mut bool,
    undo_stack: &mut crate::fm::UndoStack,
) {
    let clip = clipboard.lock().ok().and_then(|g| g.clone());
    if let Some(clip) = clip {
        let cwd = std::env::current_dir().unwrap_or_default();
        let mut had_error = false;
        let mut pasted_items = Vec::new();

        for src in &clip.items {
            let result = match clip.op {
                crate::fm::ClipOp::Copy => crate::fm::copy_into(src, &cwd),
                crate::fm::ClipOp::Cut => crate::fm::move_into(src, &cwd),
            };
            match result {
                Ok(dest) => {
                    pasted_items.push((src.clone(), dest));
                }
                Err(e) => {
                    error!("fm paste '{}': {e}", src.display());
                    had_error = true;
                }
            }
        }

        if !pasted_items.is_empty()
            && let Ok(mut u) = undo_stack.lock()
        {
            u.push(crate::fm::UndoAction::Paste {
                op: clip.op,
                items: pasted_items.clone(),
                saved_clipboard: Some(clip.clone()),
                saved_yank_paths: state.picker_ui.results.yank_paths.clone(),
                saved_cut_paths: state.picker_ui.results.cut_paths.clone(),
                previous_dir: None,
                target_dir: None,
            });
        }

        if !had_error {
            // Record paste destination directory in frecency store
            let store = waymaker::frecency::FrecencyStore::open();
            let _ = store.add(&cwd.to_string_lossy());

            if let Ok(mut cb) = clipboard.lock() {
                *cb = None;
            }
            let _ = render_tx.send(RenderCommand::Action(Action::Custom(
                MMAction::FmSetCutPaths(String::new()),
            )));
            let _ = render_tx.send(RenderCommand::Action(Action::Custom(
                MMAction::FmSetYankPaths(String::new()),
            )));
            state.picker_ui.results.cut_paths.clear();
            state.picker_ui.results.yank_paths.clear();
        }

        if *fm_notify {
            let names: Vec<String> = clip
                .items
                .iter()
                .filter_map(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .collect();
            let (verb, icon) = match clip.op {
                crate::fm::ClipOp::Copy => ("Pasted", "󰆏"),
                crate::fm::ClipOp::Cut => ("Moved", "󰆐"),
            };
            let color = if had_error { "red" } else { "cyan" };
            let count = clip.items.len();
            let item_word = if count == 1 { "item" } else { "items" };
            let msg = format!(
                "{{{color}:{icon} {verb} {count} {item_word} ({}) into current folder (u to undo)}}",
                names.join(", ")
            );
            show_styled_info_box(state, &msg);
        }
        let _ = render_tx.send(RenderCommand::Action(Action::Reload(String::new())));
    }
}

/// `FmCut`: puts the selected (or focused) paths on the clipboard to be moved.
fn fm_cut(
    state: &mut MMState<'_, '_>,
    render_tx: &mut waymaker::event::RenderSender<MMAction>,
    clipboard: &mut crate::fm::Clipboard,
    fm_notify: &mut bool,
) {
    let items = fm_current_items(state);
    if !items.is_empty() {
        let cwd = std::env::current_dir().unwrap_or_default();
        let abs_items: Vec<PathBuf> = items
            .iter()
            .map(|item| {
                let path = PathBuf::from(item);
                if path.is_absolute() {
                    path
                } else {
                    cwd.join(path)
                }
            })
            .collect();
        if let Ok(mut cb) = clipboard.lock() {
            *cb = Some(crate::fm::FmClipboard {
                items: abs_items,
                op: crate::fm::ClipOp::Cut,
            });
        }
        let _ = render_tx.send(RenderCommand::Action(Action::Custom(
            MMAction::FmSetFlashCut(items.join("\n")),
        )));
        let _ = render_tx.send(RenderCommand::Action(Action::Custom(
            MMAction::FmSetCutPaths(items.join("\n")),
        )));
        let _ = render_tx.send(RenderCommand::Action(Action::Custom(
            MMAction::FmSetYankPaths(String::new()),
        )));
        if *fm_notify {
            let msg = fm_notify_msg("Cut", &items, "red");
            show_styled_info_box(state, &msg);
        }
    }
}

/// `FmYank`: puts the selected (or focused) paths on the clipboard to be copied.
fn fm_yank(
    state: &mut MMState<'_, '_>,
    render_tx: &mut waymaker::event::RenderSender<MMAction>,
    clipboard: &mut crate::fm::Clipboard,
    fm_notify: &mut bool,
) {
    let items = fm_current_items(state);
    if !items.is_empty() {
        let cwd = std::env::current_dir().unwrap_or_default();
        let abs_items: Vec<PathBuf> = items
            .iter()
            .map(|item| {
                let path = PathBuf::from(item);
                if path.is_absolute() {
                    path
                } else {
                    cwd.join(path)
                }
            })
            .collect();
        if let Ok(mut cb) = clipboard.lock() {
            *cb = Some(crate::fm::FmClipboard {
                items: abs_items,
                op: crate::fm::ClipOp::Copy,
            });
        }
        let _ = render_tx.send(RenderCommand::Action(Action::Custom(
            MMAction::FmSetFlashCopy(items.join("\n")),
        )));
        let _ = render_tx.send(RenderCommand::Action(Action::Custom(
            MMAction::FmSetYankPaths(items.join("\n")),
        )));
        let _ = render_tx.send(RenderCommand::Action(Action::Custom(
            MMAction::FmSetCutPaths(String::new()),
        )));
        if *fm_notify {
            let msg = fm_notify_msg("Copied", &items, "yellow");
            show_styled_info_box(state, &msg);
        }
    }
}
