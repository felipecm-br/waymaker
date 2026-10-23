//! Applying a single bound action inside the render loop.

use std::io::Write;

use super::{Focus, State, get_current};
use crate::action::{Action, ActionExt};
use crate::config::ExitConfig;
use crate::event::{BindSender, EventSender};
use crate::message::{Interrupt, RenderCommand};
use crate::tui::Tui;
use crate::ui::{DisplayUI, OverlayUI, PickerUI, PreviewUI, UI};
use crate::{ActionExtHandler, MatchError, SSS, Selection};

/// What the render loop does after an action has been applied.
pub(super) enum Flow<S> {
    /// Go straight to the next event, skipping interrupt handling.
    Continue,
    /// Fall through to interrupt handling for this event.
    Proceed,
    /// Leave the render loop with this result.
    Exit(Result<Vec<S>, MatchError>),
}

/// The render loop's state that action handling reads or mutates.
pub(super) struct ActionCtx<'r, 'a, W: Write, T: SSS, S: Selection, A: ActionExt> {
    pub state: &'r mut State,
    pub ui: &'r mut UI,
    pub picker_ui: &'r mut PickerUI<'a, T, S>,
    pub footer_ui: &'r mut DisplayUI,
    pub preview_ui: &'r mut Option<PreviewUI>,
    pub tui: &'r mut Tui<W>,
    pub overlay_ui: &'r mut Option<OverlayUI<A>>,
    pub ext_handler: &'r mut Option<ActionExtHandler<T, S, A>>,
    pub did_cursor_wrap: &'r mut bool,
    pub exit_config: &'r ExitConfig,
    pub controller_tx: &'r EventSender,
    pub bind_tx: &'r BindSender<A>,
}

pub(super) fn handle_action<W: Write, T: SSS, S: Selection, A: ActionExt>(
    action: Action<A>,
    events: &mut std::vec::Drain<'_, RenderCommand<A>>,
    cx: ActionCtx<'_, '_, W, T, S, A>,
) -> Flow<S> {
    let ActionCtx {
        state,
        ui,
        picker_ui,
        footer_ui,
        preview_ui,
        tui,
        overlay_ui,
        ext_handler,
        did_cursor_wrap,
        exit_config,
        controller_tx,
        bind_tx,
    } = cx;

    if let Some(x) = overlay_ui.as_mut()
        && match action {
            Action::Char(c) => x.handle_input(c),
            _ => x.handle_action(&action),
        }
    {
        return Flow::Continue;
    }
    let PickerUI {
        query,
        action: action_input,
        action_visible,
        prompt_input,
        prompt_input_active,
        live_input_active,
        results,
        worker,
        selector: selections,
        ..
    } = &mut *picker_ui;
    match action {
        Action::Select => {
            if let Some(item) = worker.get_nth(results.index()) {
                selections.sel(item);
            }
        }
        Action::Deselect => {
            if let Some(item) = worker.get_nth(results.index()) {
                selections.desel(item);
            }
        }
        Action::DeselectUp => {
            results.cursor_prev();
            if let Some(item) = worker.get_nth(results.index()) {
                selections.desel(item);
            }
        }
        Action::Toggle => {
            if let Some(item) = worker.get_nth(results.index()) {
                if selections.contains(item) {
                    selections.toggle(item);
                    results.cursor_prev();
                } else {
                    selections.toggle(item);
                    results.cursor_next();
                }
            }
        }
        Action::ToggleUp => {
            results.cursor_prev();
            if let Some(item) = worker.get_nth(results.index()) {
                selections.toggle(item);
            }
        }
        Action::CycleAll => {
            selections.cycle_all_bg(worker.raw_results());
        }
        Action::ClearSelections => {
            selections.clear();
        }
        Action::Accept => {
            if state.prompt_input_active {
                state.prompt_input_active = false;
                *prompt_input_active = false;
                crate::PROMPT_INPUT_ACTIVE.store(false, std::sync::atomic::Ordering::Relaxed);
                let input_text = std::mem::take(&mut prompt_input.input);
                prompt_input.cancel();

                state.prompt_input_value = Some(input_text.clone());
                state.envs.set("WM_INPUT", input_text.clone());
                state.envs.set("MM_INPUT", input_text);

                if let Some(template) = state.prompt_input_template.take() {
                    let id = state.prompt_input_stash_id.take().unwrap_or(1);
                    state.set_interrupt(Interrupt::ExecuteAsync, template);
                    state.discriminant_payload = Some(2 * id + 1);
                }
                tui.redraw();
                return Flow::Continue;
            }
            let ret = if selections.is_empty() {
                if let Some(item) = get_current(picker_ui) {
                    vec![item.1]
                } else if exit_config.allow_empty {
                    vec![]
                } else {
                    return Flow::Continue;
                }
            } else {
                selections.output().collect::<Vec<S>>()
            };
            return Flow::Exit(Ok(ret));
        }
        Action::Quit(code) => {
            if state.live_input_active {
                state.live_input_active = false;
                *live_input_active = false;
                crate::LIVE_INPUT_ACTIVE.store(false, std::sync::atomic::Ordering::Relaxed);
                state.live_input_template = None;
                state.live_input_key = None;
                tui.redraw();
                return Flow::Continue;
            } else if state.prompt_input_active {
                state.prompt_input_active = false;
                *prompt_input_active = false;
                crate::PROMPT_INPUT_ACTIVE.store(false, std::sync::atomic::Ordering::Relaxed);
                if let Some(id) = state.prompt_input_stash_id.take() {
                    let _ = state.take_actions(id);
                }
                state.prompt_input_template = None;
                prompt_input.cancel();
                tui.redraw();
                return Flow::Continue;
            } else if picker_ui.action_visible {
                picker_ui.action_visible = false;
                picker_ui.action.cancel();
                crate::ACTION_BOX_ACTIVE.store(false, std::sync::atomic::Ordering::Relaxed);
            } else if !picker_ui.selector.is_empty() {
                // First Esc: clear multi-selections.
                picker_ui.selector.clear();
            } else if !picker_ui.results.yank_paths.is_empty() {
                // Second Esc: clear yanked paths.
                picker_ui.results.yank_paths.clear();
            } else if !picker_ui.results.cut_paths.is_empty() {
                // Third Esc: clear cut paths.
                picker_ui.results.cut_paths.clear();
            } else {
                return Flow::Exit(Err(MatchError::Abort(code)));
            }
        }

        // Results
        Action::ToggleWrap => {
            results.wrap(!results.is_wrap());
        }
        Action::ToggleActionBox => {
            picker_ui.action_visible = !picker_ui.action_visible;
            crate::ACTION_BOX_ACTIVE.store(
                picker_ui.action_visible,
                std::sync::atomic::Ordering::Relaxed,
            );
        }
        Action::ToggleParentPeek => {
            ui.config.parent_peek.enabled = !ui.config.parent_peek.enabled;
        }
        Action::ToggleFooter => {
            footer_ui.show = !footer_ui.show;
        }
        Action::ToggleHeader => {
            picker_ui.header.show = !picker_ui.header.show;
        }
        Action::Up(x) | Action::Down(x) => {
            let next = matches!(action, Action::Down(_)) ^ results.reverse();
            for _ in 0..x.into() {
                *did_cursor_wrap = if next {
                    results.cursor_next()
                } else {
                    results.cursor_prev()
                };
            }
        }
        Action::Pos(pos) => {
            let (_, status) = crate::nucleo::Worker::new_snapshot(&mut worker.nucleo);
            results.status = status;
            let pos = if pos >= 0 {
                (pos as u32).min(results.end())
            } else {
                results.status.matched_count.saturating_sub((-pos) as u32)
            };
            results.cursor_jump(pos);
        }
        Action::QueryPos(pos) => {
            let pos = if pos >= 0 {
                pos as u16
            } else {
                (query.len() as u16).saturating_sub((-pos) as u16)
            };
            query.set(None, pos);
        }
        Action::HScroll(n) | Action::VScroll(n) => {
            if let Some(p) = preview_ui
                && !p.config.wrap
                && false
            // track mouse location?
            {
                p.scroll(true, n);
            } else if !matches!(action, Action::HScroll(_))
                || n >= 0
                || results.hscroll > 0
                || !query.input.is_empty()
            {
                results.current_scroll(n, matches!(action, Action::HScroll(_)));
            }
        }
        Action::HalfPageDown | Action::HalfPageUp => {
            let x = results.height().div_ceil(2);
            let next = matches!(action, Action::HalfPageDown) ^ results.reverse();
            for _ in 0..x.into() {
                *did_cursor_wrap = if next {
                    results.cursor_next()
                } else {
                    results.cursor_prev()
                };
            }
        }

        // Preview Navigation
        Action::PreviewUp(n) => {
            if let Some(p) = preview_ui.as_mut() {
                if n == 0 {
                    p.reset_scroll();
                } else {
                    p.up(n);
                }
            }
        }
        Action::PreviewDown(n) => {
            if let Some(p) = preview_ui.as_mut() {
                if n == 0 {
                    p.scroll_end();
                } else {
                    p.down(n);
                }
            }
        }
        Action::ExpandPreview(n) => {
            if let Some(p) = preview_ui.as_mut() {
                p.expand(n)
            }
        }
        Action::ShrinkPreview(n) => {
            if let Some(p) = preview_ui.as_mut() {
                p.shrink(n)
            }
        }
        Action::PreviewZoomIn | Action::DiagramZoomIn => {
            if let Some(p) = preview_ui.as_mut() {
                p.zoom = (p.zoom * 1.25_f32).min(50.0);
                p.view
                    .image_id
                    .fetch_add(1, std::sync::atomic::Ordering::Release);
                p.view
                    .changed
                    .store(true, std::sync::atomic::Ordering::Release);
            }
        }
        Action::PreviewZoomOut | Action::DiagramZoomOut => {
            if let Some(p) = preview_ui.as_mut() {
                p.zoom = (p.zoom / 1.25_f32).max(0.1);
                p.view
                    .image_id
                    .fetch_add(1, std::sync::atomic::Ordering::Release);
                p.view
                    .changed
                    .store(true, std::sync::atomic::Ordering::Release);
            }
        }
        Action::PreviewResetZoom | Action::DiagramResetZoom => {
            if let Some(p) = preview_ui.as_mut() {
                p.zoom = 1.0_f32;
                p.pan_x = 0;
                p.pan_y = 0;
                p.view
                    .image_id
                    .fetch_add(1, std::sync::atomic::Ordering::Release);
                p.view
                    .changed
                    .store(true, std::sync::atomic::Ordering::Release);
            }
        }
        Action::ToggleDiagram => {
            if let Some(p) = preview_ui.as_mut() {
                p.toggle_diagram();
                state.preview_diagram_mode = p.is_diagram_mode();
                state.needs_redraw = true;
            }
        }
        Action::PreviewHalfPageUp | Action::PreviewHalfPageDown => {
            if let Some(p) = preview_ui.as_mut() {
                let n = p.area.height.div_ceil(2);

                if matches!(action, Action::PreviewHalfPageUp) {
                    p.up(n)
                } else {
                    p.down(n)
                }
            }
        }

        Action::PreviewHScroll(x) | Action::PreviewScroll(x) => {
            if let Some(p) = preview_ui.as_mut() {
                p.scroll(matches!(action, Action::PreviewHScroll(_)), x);
            }
        }
        Action::PreviewJump => {
            if let Some(p) = preview_ui.as_mut() {
                p.jump()
            }
        }

        Action::NextDiagram | Action::PrevDiagram => {
            if let Some(p) = preview_ui.as_mut() {
                let offsets = p
                    .view
                    .diagram_offsets
                    .lock()
                    .ok()
                    .map(|g| g.clone())
                    .unwrap_or_default();
                if !offsets.is_empty() {
                    let current = p.current_offset();
                    let (target, next_idx) = if matches!(action, Action::NextDiagram) {
                        // Find first offset strictly greater than current scroll
                        let found = offsets
                            .iter()
                            .copied()
                            .enumerate()
                            .find(|(_, o)| *o > current);
                        if let Some((idx, o)) = found {
                            (o, idx)
                        } else {
                            (*offsets.first().unwrap(), 0)
                        }
                    } else {
                        // Find last offset strictly less than current scroll
                        let found = offsets
                            .iter()
                            .copied()
                            .enumerate()
                            .rfind(|(_, o)| *o < current);
                        if let Some((idx, o)) = found {
                            (o, idx)
                        } else {
                            (*offsets.last().unwrap(), offsets.len() - 1)
                        }
                    };
                    p.scroll_to(target);

                    // Update active diagram image from sources if available
                    let src_opt = p
                        .view
                        .diagram_sources
                        .lock()
                        .ok()
                        .and_then(|sources| sources.get(next_idx).cloned());
                    if let Some(src) = src_opt {
                        p.view
                            .current_diagram_idx
                            .store(next_idx, std::sync::atomic::Ordering::Release);
                        if let Some(img) =
                            crate::utils::mermaid::render_mermaid_to_image_with_options(
                                &src,
                                2.0,
                                p.config.diagrams.theme,
                                p.config.diagrams.background,
                            )
                        {
                            if let Ok(mut guard) = p.view.image.lock() {
                                *guard = Some(img);
                            }
                            p.reset_diagram_pan();
                            p.view
                                .image_id
                                .fetch_add(1, std::sync::atomic::Ordering::Release);
                            p.view
                                .changed
                                .store(true, std::sync::atomic::Ordering::Release);
                        }
                    }
                } else {
                    let src_opt = p.view.diagram_sources.lock().ok().and_then(|sources| {
                        if sources.is_empty() {
                            return None;
                        }
                        let total = sources.len();
                        let cur = p
                            .view
                            .current_diagram_idx
                            .load(std::sync::atomic::Ordering::Relaxed);
                        let next_idx = if matches!(action, Action::NextDiagram) {
                            (cur + 1) % total
                        } else {
                            (cur + total.saturating_sub(1)) % total
                        };
                        p.view
                            .current_diagram_idx
                            .store(next_idx, std::sync::atomic::Ordering::Release);
                        sources.get(next_idx).cloned()
                    });
                    if let Some(src) = src_opt
                        && let Some(img) =
                            crate::utils::mermaid::render_mermaid_to_image_with_options(
                                &src,
                                2.0,
                                p.config.diagrams.theme,
                                p.config.diagrams.background,
                            )
                    {
                        if let Ok(mut guard) = p.view.image.lock() {
                            *guard = Some(img);
                        }
                        p.reset_diagram_pan();
                        p.view
                            .image_id
                            .fetch_add(1, std::sync::atomic::Ordering::Release);
                        p.view
                            .changed
                            .store(true, std::sync::atomic::Ordering::Release);
                    }
                }
            }
        }

        // Preview
        Action::CyclePreview => {
            if let Some(p) = preview_ui.as_mut() {
                p.cycle_layout();
                p.current_dimension = None;
                p.view
                    .image_id
                    .fetch_add(1, std::sync::atomic::Ordering::Release);
                p.view
                    .changed
                    .store(true, std::sync::atomic::Ordering::Release);
                state.insert(crate::message::Event::PreviewChange);
                if !p.command().is_empty() {
                    state.update_preview_payload(p.command());
                }
                if p.is_fullscreen() {
                    state.focus = Focus::Results;
                    state.preview_fullscreen = true;
                } else {
                    state.focus = if ui.config.nav.active {
                        Focus::Results
                    } else {
                        Focus::Input
                    };
                    state.preview_fullscreen = false;
                }
                tui.redraw();
            }
        }

        Action::Preview(context) => {
            if let Some(p) = preview_ui.as_mut() {
                if !state.update_preview_payload(context.as_str()) {
                    p.toggle_show()
                } else {
                    p.show(true);
                }
            };
        }
        Action::Help(context) => {
            if let Some(p) = preview_ui.as_mut() {
                // empty payload signifies help
                if !state.update_preview_set(Err(context.into())) {
                    state.update_preview_unset()
                } else {
                    p.show(true);
                }
            };
        }
        Action::SetPreview(idx) => {
            if let Some(p) = preview_ui.as_mut() {
                if let Some(idx) = idx {
                    p.show(true);
                    p.set_layout(idx);
                    p.current_dimension = None;
                    p.view
                        .image_id
                        .fetch_add(1, std::sync::atomic::Ordering::Release);
                    p.view
                        .changed
                        .store(true, std::sync::atomic::Ordering::Release);
                    state.insert(crate::message::Event::PreviewChange);
                    if !p.command().is_empty() {
                        state.update_preview_payload(p.command());
                    }
                    if p.is_fullscreen() {
                        state.focus = Focus::Results;
                        state.preview_fullscreen = true;
                    } else {
                        state.focus = if ui.config.nav.active {
                            Focus::Results
                        } else {
                            Focus::Input
                        };
                        state.preview_fullscreen = false;
                    }
                    tui.redraw();
                } else {
                    state.update_preview_payload(p.command());
                }
            }
        }
        Action::SwitchPreview(idx) => {
            if let Some(p) = preview_ui.as_mut() {
                if let Some(idx) = idx {
                    let target_idx = idx as usize;
                    let new_idx = if p.layout_idx() == target_idx {
                        if target_idx != 0 {
                            Some(0)
                        } else {
                            p.toggle_show();
                            tui.redraw();
                            None
                        }
                    } else {
                        Some(target_idx as u8)
                    };
                    if let Some(new_idx) = new_idx {
                        p.show(true);
                        p.set_layout(new_idx);
                        p.current_dimension = None;
                        p.view
                            .image_id
                            .fetch_add(1, std::sync::atomic::Ordering::Release);
                        p.view
                            .changed
                            .store(true, std::sync::atomic::Ordering::Release);
                        state.insert(crate::message::Event::PreviewChange);
                        if !p.command().is_empty() {
                            state.update_preview_payload(p.command());
                        }
                        if p.is_fullscreen() {
                            state.focus = Focus::Results;
                            state.preview_fullscreen = true;
                        } else {
                            state.focus = if ui.config.nav.active {
                                Focus::Results
                            } else {
                                Focus::Input
                            };
                            state.preview_fullscreen = false;
                        }
                        tui.redraw();
                    }
                } else {
                    p.toggle_show();
                    tui.redraw();
                }
            }
        }
        Action::TogglePreviewWrap => {
            if let Some(p) = preview_ui.as_mut() {
                p.wrap(!p.is_wrap());
            }
        }

        // Programmable
        Action::Execute(payload) => {
            state.set_interrupt(Interrupt::Execute, payload);
        }
        Action::CopyAsync(ref payload) => {
            state.set_interrupt(Interrupt::ExecuteAsync, payload.clone());
            state.discriminant_payload = Some(if tui.config.osc52 { 1 } else { 0 });
        }
        Action::Copy(ref payload) => {
            state.set_interrupt(Interrupt::ExecuteSilent, payload.clone());
            state.discriminant_payload = Some(if tui.config.osc52 { 3 } else { 2 });
        }
        Action::ExecuteAsync(ref payload) | Action::ExecuteThen(ref payload) => {
            let is_async = matches!(action, Action::ExecuteAsync(_));
            let payload = payload.clone();

            let mut remainder = crate::action::Actions::default();
            for cmd in events.by_ref() {
                if let RenderCommand::Action(a) | RenderCommand::KeyAction { action: a, .. } = cmd {
                    remainder.push(a);
                }
            }

            if let Some(id) = state.stash_actions(remainder, bind_tx.clone()) {
                state.set_interrupt(Interrupt::ExecuteAsync, payload);
                state.discriminant_payload = Some(2 * id + (if is_async { 1 } else { 0 }));
            } else {
                log::error!("No free slots left: remaining actions dropped");
            }
        }
        Action::ExecuteSilent(payload) => {
            state.set_interrupt(Interrupt::ExecuteSilent, payload);
        }
        Action::Store(payload) => {
            state.envs.set("WM_STORE", payload.clone());
            state.envs.set("MM_STORE", payload);
        }
        Action::Become(payload) => {
            state.set_interrupt(Interrupt::Become, payload);
        }
        Action::BecomeSilent(payload) => {
            state.set_interrupt(Interrupt::BecomeSilent, payload);
        }
        Action::Reload(payload) => {
            state.set_interrupt(Interrupt::Reload, payload);
        }
        Action::ChDir(payload) => {
            if ui.config.nav.active {
                state.focus = Focus::Results;
                state.focus_blink = true;
                state.focus_tick = 0;
                let prompt = &ui.config.nav.prompt;
                if !prompt.is_empty() {
                    picker_ui
                        .query
                        .set_prompt(Some(ratatui::text::Line::raw(prompt.clone())));
                }
            }
            state.set_interrupt(Interrupt::ChDir, payload);
        }
        Action::Print(payload) => {
            state.set_interrupt(Interrupt::Print, payload);
        }
        Action::PromptInput(payload) => {
            let mut remainder = crate::action::Actions::default();
            for cmd in events.by_ref() {
                if let RenderCommand::Action(a) | RenderCommand::KeyAction { action: a, .. } = cmd {
                    remainder.push(a);
                }
            }

            state.prompt_input_stash_id = state.stash_actions(remainder, bind_tx.clone());
            state.prompt_input_template = Some(payload);
            state.prompt_input_active = true;
            *prompt_input_active = true;
            crate::PROMPT_INPUT_ACTIVE.store(true, std::sync::atomic::Ordering::Relaxed);
            prompt_input.cancel();
            tui.redraw();
        }
        Action::LiveInput(payload) => {
            state.live_input_active = true;
            *live_input_active = true;
            crate::LIVE_INPUT_ACTIVE.store(true, std::sync::atomic::Ordering::Relaxed);
            let template = if !payload.is_empty() {
                payload
            } else {
                ui.config.nav.live_input.clone().unwrap_or_default()
            };
            state.live_input_template = Some(template);
            tui.redraw();
        }
        Action::LiveInputKey(keys) => {
            if state.live_input_active
                && let Some(ref template) = state.live_input_template
            {
                state.live_input_key = Some(keys.clone());
                state.envs.set("WM_KEY", keys.clone());
                state.envs.set("MM_KEY", keys.clone());
                state.envs.set("WM_KEYS", keys.clone());
                state.envs.set("MM_KEYS", keys);
                state.set_interrupt(Interrupt::LiveInputKey, template.clone());
            }
        }

        // Columns
        Action::SwitchColumn(col_name) => {
            if worker.query.active_column_name(query.str_at_cursor()) != col_name
                && worker.columns.iter().any(|c| *c.name == col_name)
            {
                query.prepare_column_change();
                query.push_str(&format!("%{} ", col_name));
            } else {
                log::warn!("Column {} not found in worker columns", col_name);
            }
        }
        Action::NextColumn | Action::PrevColumn => {
            let cursor_byte = query.byte_index(query.cursor() as usize);
            let active_idx = worker.query.active_column_index(cursor_byte);

            let num_columns = worker.columns.len();
            let has_multiple_columns =
                num_columns > 1 || (num_columns == 1 && !worker.columns[0].name.is_empty());
            if has_multiple_columns {
                query.prepare_column_change();

                let mut next_idx = match action {
                    Action::NextColumn => active_idx + 1,
                    Action::PrevColumn => active_idx + num_columns - 1 % num_columns,
                    _ => unreachable!(),
                } % num_columns;

                loop {
                    if next_idx < results.hidden_columns.len() && results.hidden_columns[next_idx] {
                        next_idx = match action {
                            Action::NextColumn => (next_idx + 1) % num_columns,
                            Action::PrevColumn => (next_idx + num_columns - 1) % num_columns,
                            _ => unreachable!(),
                        };
                    } else {
                        break;
                    }
                }

                let col_name = &worker.columns[next_idx].name;
                query.push_str(&format!("%{} ", col_name));
            }
        }

        Action::ToggleColumn(col_name) => {
            let index = if let Some(name) = col_name {
                worker.columns.iter().position(|c| *c.name == name)
            } else {
                let cursor_byte = query.byte_index(query.cursor() as usize);
                Some(worker.query.active_column_index(cursor_byte))
            };

            if let Some(idx) = index {
                if idx >= results.hidden_columns.len() {
                    results.hidden_columns.resize(idx + 1, false);
                }
                results.hidden_columns[idx] = !results.hidden_columns[idx];
            }
        }

        Action::ShowColumn(col_name) => {
            if let Some(name) = col_name {
                if let Some(idx) = worker.columns.iter().position(|c| *c.name == name)
                    && idx < results.hidden_columns.len()
                {
                    results.hidden_columns[idx] = false;
                }
            } else {
                for val in results.hidden_columns.iter_mut() {
                    *val = false;
                }
            }
        }

        // Edit
        Action::SetQuery(context) => {
            if state.prompt_input_active {
                prompt_input.set(context, u16::MAX);
            } else if *action_visible
                && crate::ACTION_BOX_ACTIVE.load(std::sync::atomic::Ordering::Relaxed)
            {
                action_input.set(context, u16::MAX);
            } else {
                query.set(context, u16::MAX);
            }
        }
        Action::ForwardChar => {
            if state.prompt_input_active {
                prompt_input.forward_char()
            } else if *action_visible
                && crate::ACTION_BOX_ACTIVE.load(std::sync::atomic::Ordering::Relaxed)
            {
                action_input.forward_char()
            } else {
                query.forward_char()
            }
        }
        Action::BackwardChar => {
            if state.prompt_input_active {
                prompt_input.backward_char()
            } else if *action_visible
                && crate::ACTION_BOX_ACTIVE.load(std::sync::atomic::Ordering::Relaxed)
            {
                action_input.backward_char()
            } else {
                query.backward_char()
            }
        }
        Action::ForwardWord => {
            if state.prompt_input_active {
                prompt_input.forward_word()
            } else if *action_visible
                && crate::ACTION_BOX_ACTIVE.load(std::sync::atomic::Ordering::Relaxed)
            {
                action_input.forward_word()
            } else {
                query.forward_word()
            }
        }
        Action::BackwardWord => {
            if state.prompt_input_active {
                prompt_input.backward_word()
            } else if *action_visible
                && crate::ACTION_BOX_ACTIVE.load(std::sync::atomic::Ordering::Relaxed)
            {
                action_input.backward_word()
            } else {
                query.backward_word()
            }
        }
        Action::DeleteChar => {
            if state.prompt_input_active {
                prompt_input.delete()
            } else if *action_visible
                && crate::ACTION_BOX_ACTIVE.load(std::sync::atomic::Ordering::Relaxed)
            {
                action_input.delete()
            } else {
                query.delete()
            }
        }
        Action::DeleteWord => {
            if state.prompt_input_active {
                prompt_input.delete_word()
            } else if *action_visible
                && crate::ACTION_BOX_ACTIVE.load(std::sync::atomic::Ordering::Relaxed)
            {
                action_input.delete_word()
            } else {
                query.delete_word()
            }
        }
        Action::DeleteNextChar => {
            if state.prompt_input_active {
                prompt_input.delete_next()
            } else if *action_visible
                && crate::ACTION_BOX_ACTIVE.load(std::sync::atomic::Ordering::Relaxed)
            {
                action_input.delete_next()
            } else {
                query.delete_next()
            }
        }
        Action::DeleteNextWord => {
            if state.prompt_input_active {
                prompt_input.delete_next_word()
            } else if *action_visible
                && crate::ACTION_BOX_ACTIVE.load(std::sync::atomic::Ordering::Relaxed)
            {
                action_input.delete_next_word()
            } else {
                query.delete_next_word()
            }
        }
        Action::DeleteLineStart => {
            if state.prompt_input_active {
                prompt_input.delete_line_start()
            } else if *action_visible
                && crate::ACTION_BOX_ACTIVE.load(std::sync::atomic::Ordering::Relaxed)
            {
                action_input.delete_line_start()
            } else {
                query.delete_line_start()
            }
        }
        Action::DeleteLineEnd => {
            if state.prompt_input_active {
                prompt_input.delete_line_end()
            } else if *action_visible
                && crate::ACTION_BOX_ACTIVE.load(std::sync::atomic::Ordering::Relaxed)
            {
                action_input.delete_line_end()
            } else {
                query.delete_line_end()
            }
        }
        Action::Cancel => {
            if state.live_input_active {
                state.live_input_active = false;
                *live_input_active = false;
                crate::LIVE_INPUT_ACTIVE.store(false, std::sync::atomic::Ordering::Relaxed);
                state.live_input_template = None;
                state.live_input_key = None;
                tui.redraw();
                return Flow::Continue;
            } else if state.prompt_input_active {
                state.prompt_input_active = false;
                *prompt_input_active = false;
                crate::PROMPT_INPUT_ACTIVE.store(false, std::sync::atomic::Ordering::Relaxed);
                if let Some(id) = state.prompt_input_stash_id.take() {
                    let _ = state.take_actions(id);
                }
                state.prompt_input_template = None;
                prompt_input.cancel();
                tui.redraw();
                return Flow::Continue;
            } else if *action_visible {
                *action_visible = false;
                action_input.cancel();
                crate::ACTION_BOX_ACTIVE.store(false, std::sync::atomic::Ordering::Relaxed);
                tui.redraw();
            } else {
                query.cancel()
            }
        }

        // Other
        Action::Redraw => {
            tui.redraw();
        }
        Action::ToggleFocus => {
            if *action_visible
                && crate::ACTION_BOX_ACTIVE.load(std::sync::atomic::Ordering::Relaxed)
            {
                *action_visible = false;
                action_input.cancel();
                crate::ACTION_BOX_ACTIVE.store(false, std::sync::atomic::Ordering::Relaxed);
                tui.redraw();
                return Flow::Continue;
            }
            if ui.config.nav.active {
                state.focus = match state.focus {
                    Focus::Input => Focus::Results,
                    Focus::Results => Focus::Input,
                };
                state.focus_blink = true;
                state.focus_tick = 0;

                let prompt = &ui.config.nav.prompt;
                if !prompt.is_empty() {
                    let prompt = match state.focus {
                        Focus::Results => Some(ratatui::text::Line::raw(prompt.clone())),
                        Focus::Input => None,
                    };
                    picker_ui.query.set_prompt(prompt);
                }
            }
        }
        Action::FocusFilter => {
            if *action_visible
                && crate::ACTION_BOX_ACTIVE.load(std::sync::atomic::Ordering::Relaxed)
            {
                *action_visible = false;
                action_input.cancel();
                crate::ACTION_BOX_ACTIVE.store(false, std::sync::atomic::Ordering::Relaxed);
                tui.redraw();
                return Flow::Continue;
            }
            if ui.config.nav.active {
                state.focus = Focus::Input;
                state.focus_blink = true;
                state.focus_tick = 0;
                let prompt = &ui.config.nav.prompt;
                if !prompt.is_empty() {
                    picker_ui.query.set_prompt(None);
                }
            }
        }
        Action::FocusNav => {
            if *action_visible
                && crate::ACTION_BOX_ACTIVE.load(std::sync::atomic::Ordering::Relaxed)
            {
                *action_visible = false;
                action_input.cancel();
                crate::ACTION_BOX_ACTIVE.store(false, std::sync::atomic::Ordering::Relaxed);
                tui.redraw();
                return Flow::Continue;
            }
            if ui.config.nav.active {
                state.focus = Focus::Results;
                state.focus_blink = true;
                state.focus_tick = 0;
                let prompt = &ui.config.nav.prompt;
                if !prompt.is_empty() {
                    picker_ui
                        .query
                        .set_prompt(Some(ratatui::text::Line::raw(prompt.clone())));
                }
            }
        }
        Action::Overlay(index) => {
            if let Some(x) = overlay_ui.as_mut() {
                x.enable(index, &ui.area());
                tui.redraw();
            };
        }
        Action::Custom(e) => {
            if let Some(handler) = ext_handler {
                handler(
                    e,
                    &mut state.dispatcher(
                        ui,
                        picker_ui,
                        footer_ui,
                        preview_ui,
                        controller_tx,
                    ),
                );
            }
        }
        Action::Char(c) => {
            if !c.is_ascii_control() {
                if state.prompt_input_active {
                    prompt_input.push_char(c);
                    tui.redraw();
                } else {
                    let is_action_box_input = *action_visible
                        && crate::ACTION_BOX_ACTIVE.load(std::sync::atomic::Ordering::Relaxed);
                    if is_action_box_input {
                        action_input.push_char(c)
                    } else {
                        if *action_visible {
                            *action_visible = false;
                            action_input.cancel();
                            crate::ACTION_BOX_ACTIVE
                                .store(false, std::sync::atomic::Ordering::Relaxed);
                        }
                        if !(ui.config.nav.active
                            && !ui.config.nav.passthrough
                            && state.focus == Focus::Results)
                        {
                            query.push_char(c)
                        }
                    }
                }
            }
        }
        Action::SetMode(s) => {
            if let Ok(mut m) = crate::MODE.lock() {
                *m = s;
            }
        }
        Action::Sort(order) => {
            picker_ui.worker.set_sort_order(order);
            state.sort_menu_active = false;
            state.needs_redraw = true;
        }
        Action::SortMenu => {
            state.sort_menu_active = !state.sort_menu_active;
            state.needs_redraw = true;
        }

        // unreachable
        Action::PrintKey => {}
        Action::Semantic(_) => {}
        Action::Trace(_) => {}
    }

    Flow::Proceed
}
