use super::*;

pub(super) fn handle_reload(a: MMAction, state: &mut MMState<'_, '_>, ctx: &mut ActionContext) {
    let ActionContext {
        render_tx,
        additional_commands,
        mode_history,
        nav,
        ..
    } = ctx;
    match a {
        MMAction::ReloadNext(x) => {
            if additional_commands.0.is_empty() {
                return;
            }

            let old_index = additional_commands.1;
            let index = match x {
                None => {
                    additional_commands.1 =
                        (additional_commands.1 + 1) % additional_commands.0.len();
                    additional_commands.1
                }
                Some(x) => {
                    if x < additional_commands.0.len() {
                        let target = if additional_commands.1 == x && x != 0 {
                            0
                        } else {
                            x
                        };
                        additional_commands.1 = target;
                        target
                    } else {
                        error!("Index {x} is out of bounds for ReloadNext");
                        return;
                    }
                }
            };

            switch_mode(
                state,
                render_tx,
                additional_commands,
                mode_history,
                nav,
                old_index,
                index,
            );
        }

        MMAction::ReloadPrev => {
            if additional_commands.0.is_empty() {
                return;
            }

            let old_index = additional_commands.1;
            additional_commands.1 = (additional_commands.1 + additional_commands.0.len() - 1)
                % additional_commands.0.len();

            let index = additional_commands.1;

            switch_mode(
                state,
                render_tx,
                additional_commands,
                mode_history,
                nav,
                old_index,
                index,
            );
        }

        MMAction::ReloadReady(_) => {
            state.reloading = false;
            state.picker_ui.worker.refind();
            let mut iters = 0;
            while state.picker_ui.worker.nucleo.tick(10).running && iters < 50 {
                iters += 1;
            }
            if state.filtering {
                state.picker_ui.update();
            }

            let target_opt = nav.lock().unwrap().target_item.clone();

            let mut target_found = false;

            if let Some(ref target) = target_opt {
                let is_local = state.picker_ui.worker.mode_index == 0;
                let col0 = &state.picker_ui.worker.columns[0];
                let found_idx = state.picker_ui.worker.find_item_index(|raw| {
                    let val = col0.raw(raw);
                    crate::start::is_target_item_match(&val, target, is_local)
                });
                if let Some(i) = found_idx {
                    state.picker_ui.results.cursor_jump(i as u32);
                    target_found = true;
                    nav.lock().unwrap().target_item = None;
                }
            }

            if !target_found && target_opt.is_none() {
                let mut restored = false;
                let prev_reload_item = nav.lock().unwrap().prev_reload_item.take();
                if let Some((prev_item, prev_idx, prev_dir)) = prev_reload_item {
                    let current_dir = std::env::current_dir().unwrap_or_default();
                    if prev_dir == current_dir {
                        let count = state.picker_ui.worker.counts().0;
                        if count > 0 {
                            if let Some(ref item_str) = prev_item {
                                let col0 = &state.picker_ui.worker.columns[0];
                                if let Some(i) = state
                                    .picker_ui
                                    .worker
                                    .find_item_index(|raw| col0.raw(raw) == item_str.as_str())
                                {
                                    state.picker_ui.results.cursor_jump(i as u32);
                                    restored = true;
                                }
                            }
                            if !restored {
                                let target_idx = prev_idx.min(count.saturating_sub(1));
                                state.picker_ui.results.cursor_jump(target_idx);
                                restored = true;
                            }
                        }
                    }
                }
                if !restored {
                    state.picker_ui.results.cursor_jump(0);
                }
            }

            state.needs_redraw = true;
        }

        _ => unreachable!("handle_reload got a non-reload action"),
    }
}

/// Switches from start command `old_index` to `index` (already set in `additional_commands`):
/// saves the old mode's query, cursor and focus, restores the new mode's, reloads with its
/// command and moves focus and cursor as remembered.
fn switch_mode(
    state: &mut MMState<'_, '_>,
    render_tx: &waymaker::event::RenderSender<MMAction>,
    additional_commands: &(Vec<String>, usize),
    mode_history: &mut std::collections::HashMap<usize, ModeHistory>,
    nav: &crate::start::SharedNav,
    old_index: usize,
    index: usize,
) {
    if old_index != index {
        let focused_item = state.current_raw().and_then(|item| {
            state
                .picker_ui
                .worker
                .columns
                .first()
                .map(|c| c.raw(item).into_owned())
        });
        mode_history.insert(
            old_index,
            ModeHistory {
                query: state.picker_ui.query.input.clone(),
                query_cursor: state.picker_ui.query.cursor,
                results_index: state.picker_ui.results.index(),
                focused_item,
                focus: state.focus,
            },
        );
        if let Some(saved) = mode_history.get(&index) {
            state
                .picker_ui
                .query
                .set(Some(saved.query.clone()), saved.query_cursor as u16);
            if let Some(ref target) = saved.focused_item {
                nav.lock().unwrap().target_item = Some(target.clone());
            }
        } else {
            state.picker_ui.query.set(Some(String::new()), 0);
            nav.lock().unwrap().target_item = None;
        }
        state.picker_ui.update();
    }

    let payload = &additional_commands.0[index];
    state.envs.set("WM_INDEX", index);
    state.envs.set("MM_INDEX", index);
    state.picker_ui.query.set_mode_index(index);
    state.picker_ui.results.set_mode_index(index);
    state.picker_ui.worker.set_mode_index(index);
    state.picker_ui.results.set_status_line(None);
    state.set_interrupt(Interrupt::Reload, payload.clone());

    let _ = render_tx.send(RenderCommand::Action(Action::Custom(
        MMAction::SetModeIndex(index),
    )));
    if state.ui.config.nav.active {
        let focus_to_set = if let Some(saved) = mode_history.get(&index) {
            saved.focus
        } else if index == 1
            && state.ui.config.nav.focus_on_start != waymaker::config::NavFocus::Picker
        {
            waymaker::render::Focus::Input
        } else {
            waymaker::render::Focus::Results
        };
        state.focus = focus_to_set;
        match focus_to_set {
            waymaker::render::Focus::Results => {
                let _ = render_tx.send(RenderCommand::Action(Action::FocusNav));
            }
            waymaker::render::Focus::Input => {
                let _ = render_tx.send(RenderCommand::Action(Action::FocusFilter));
            }
        }
    } else {
        state.focus = waymaker::render::Focus::Input;
        let _ = render_tx.send(RenderCommand::Action(Action::FocusFilter));
    }

    let has_target = mode_history
        .get(&index)
        .and_then(|s| s.focused_item.as_ref())
        .is_some()
        || nav.lock().unwrap().target_item.is_some();
    if !has_target
        && let Some(pos) = mode_history.get(&index).map(|s| s.results_index)
        && pos > 0
    {
        let _ = render_tx.send(RenderCommand::Action(Action::Pos(pos as i32)));
        state.picker_ui.results.cursor_jump(pos);
    }
}
