mod draw;
mod dynamic;
mod keys;
mod live_input;
mod state;
#[cfg(test)]
mod tests;

use cba::bait::ResultExt;
use crossterm::event::{MouseButton, MouseEventKind};
use draw::*;
pub use draw::{
    BASIC_NAV_HINTS, Click, FILTER_NAV_HINTS, LIST_NAV_HINTS, NAV_HINTS, PREVIEW_NAV_HINTS,
};
pub use dynamic::*;
use keys::*;
pub use live_input::quote_live_token;
use live_input::*;
pub use state::*;
// ------------------------------

use std::io::Write;

use log::{debug, info, warn};
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;
use tokio::sync::mpsc;

#[cfg(feature = "bracketed-paste")]
use crate::PasteHandler;
use crate::action::{Action, ActionExt};
use crate::config::{ExitConfig, RowConnectionStyle};
use crate::event::{BindSender, EventSender};
use crate::message::{Event, Interrupt, RenderCommand};
use crate::tui::Tui;
use crate::ui::{DisplayUI, OverlayUI, PickerUI, PreviewUI, UI};
use crate::{ActionAliaser, ActionExtHandler, Initializer, MatchError, SSS, Selection};
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use ratatui::widgets::{Borders, Paragraph};

#[allow(clippy::too_many_arguments)]
pub(crate) async fn render_loop<'a, W: Write, T: SSS, S: Selection, A: ActionExt>(
    mut ui: UI,
    mut picker_ui: PickerUI<'a, T, S>,
    mut footer_ui: DisplayUI,
    mut preview_ui: Option<PreviewUI>,
    mut tui: Tui<W>,

    mut overlay_ui: Option<OverlayUI<A>>,
    exit_config: ExitConfig,

    mut render_rx: mpsc::UnboundedReceiver<RenderCommand<A>>,
    controller_tx: EventSender,
    bind_tx: BindSender<A>,

    mut dynamic_handlers: DynamicHandlers<T, S>,
    mut ext_handler: Option<ActionExtHandler<T, S, A>>,
    mut ext_aliaser: Option<ActionAliaser<T, S, A>>,
    initializer: Option<Initializer<T, S>>,
    #[cfg(feature = "bracketed-paste")] //
    mut paste_handler: Option<PasteHandler<T, S>>,
) -> Result<Vec<S>, MatchError> {
    let mut state = State::new();
    if ui.config.nav.active {
        match ui.config.nav.focus_on_start {
            crate::config::NavFocus::Picker => state.focus = Focus::Results,
            crate::config::NavFocus::Filter => state.focus = Focus::Input,
        }
    }

    if let Some(ref p) = ui.config.nav.prompt_input {
        picker_ui
            .prompt_input
            .set_prompt(Some(ratatui::text::Line::styled(
                p.clone(),
                ratatui::style::Style::default()
                    .fg(ratatui::style::Color::Magenta)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            )));
    }

    state.refresh_parent_peek_cache();
    if let Some(ref cwd) = state.cached_cwd {
        let sort_order = ui.config.resolve_sort_for_dir(cwd);
        picker_ui.worker.set_sort_order(sort_order);
    }

    if let Some(handler) = initializer {
        handler(&mut state.dispatcher(
            &mut ui,
            &mut picker_ui,
            &mut footer_ui,
            &mut preview_ui,
            &controller_tx,
        ));
    }

    let mut click = Click::None;
    // Tracks the last known mouse position for gap hover highlighting.
    let mut mouse_hover: Option<Position> = None;

    // place the initial command in the state where the preview listener can access
    if let Some(ref p) = preview_ui {
        state.update_preview_payload(p.get_initial_command());
    }

    let mut buffer = Vec::with_capacity(256);
    let mut last_preview_poll = std::time::Instant::now();

    while render_rx.recv_many(&mut buffer, 256).await > 0 {
        while let Ok(cmd) = render_rx.try_recv() {
            buffer.push(cmd);
        }
        if state.iterations == 0 {
            log::debug!("Render loop started");
            state.needs_redraw = true;
        }
        let has_non_tick = buffer.iter().any(|cmd| !matches!(cmd, RenderCommand::Tick));
        if has_non_tick {
            state.needs_redraw = true;
        }
        if let Some(p) = preview_ui.as_mut() {
            if p.is_pan_settling() {
                state.needs_redraw = true;
            }
            if p.visible() && !p.command().is_empty() {
                let poll_interval = p.poll_interval();
                let effective_interval = if state.live_input_active {
                    poll_interval.or(Some(std::time::Duration::from_millis(150)))
                } else {
                    poll_interval
                };

                if let Some(interval) = effective_interval
                    && last_preview_poll.elapsed() >= interval
                {
                    last_preview_poll = std::time::Instant::now();
                    p.view
                        .image_id
                        .fetch_add(1, std::sync::atomic::Ordering::Release);
                    p.view
                        .changed
                        .store(true, std::sync::atomic::Ordering::Release);
                    state.insert(crate::message::Event::PreviewChange);
                    state.needs_redraw = true;
                }
            }
        }
        let (mut did_pause, mut did_reload, mut did_exit, mut did_resize, mut did_cursor_wrap) =
            (false, false, None, false, false);

        if let Some(aliaser) = &mut ext_aliaser {
            apply_aliases(
                &mut buffer,
                aliaser,
                &mut state.dispatcher(
                    &mut ui,
                    &mut picker_ui,
                    &mut footer_ui,
                    &mut preview_ui,
                    &controller_tx,
                ),
            )
        };

        if state.live_input_active {
            apply_live_input(&mut buffer, state.live_input_active);
        } else if state.prompt_input_active {
            apply_prompt_input(&mut buffer, state.prompt_input_active);
        } else if state.sort_menu_active {
            apply_sort_menu(&mut buffer, &mut state.sort_menu_active);
        } else if ui.config.nav.active {
            if !ui.config.nav.passthrough {
                apply_focus_binds_mode(
                    &mut buffer,
                    state.focus,
                    &ui.config.nav.binds,
                    picker_ui.action_visible
                        || overlay_ui.as_ref().is_some_and(|o| o.index().is_some()),
                    &mut state.pending_nav_key,
                    &mut state.sort_menu_active,
                    state.preview_fullscreen,
                    state.preview_diagram_mode,
                );
            }

            if let Some(aliaser) = &mut ext_aliaser {
                apply_aliases(
                    &mut buffer,
                    aliaser,
                    &mut state.dispatcher(
                        &mut ui,
                        &mut picker_ui,
                        &mut footer_ui,
                        &mut preview_ui,
                        &controller_tx,
                    ),
                )
            };
        } else {
            for cmd in &mut buffer {
                if let RenderCommand::KeyAction { action, .. } = cmd {
                    *cmd = RenderCommand::Action(action.clone());
                }
            }
        }

        if state.should_quit {
            log::debug!("Exiting due to should_quit");
            return if picker_ui.selector.is_disabled()
                && let Some((_, item)) = get_current(&picker_ui)
            {
                Ok(vec![item])
            } else {
                Ok(picker_ui.selector.output().collect())
            };
        } else if state.should_quit_nomatch {
            log::debug!("Exiting due to should_quit_nomatch");
            return Err(MatchError::NoMatch);
        }

        let mut events = buffer.drain(..);
        while let Some(event) = events.next() {
            state.clear_interrupt();

            if !matches!(event, RenderCommand::Tick) {
                info!("Received {event:?}");
            } else {
                // log::trace!("Recieved {event:?}");
            }

            match event {
                #[cfg(feature = "bracketed-paste")]
                RenderCommand::Paste(content) => {
                    if let Some(handler) = &mut paste_handler {
                        let content = {
                            handler(
                                content,
                                &state.dispatcher(
                                    &mut ui,
                                    &mut picker_ui,
                                    &mut footer_ui,
                                    &mut preview_ui,
                                    &controller_tx,
                                ),
                            )
                        };
                        if !content.is_empty() {
                            if let Some(x) = overlay_ui.as_mut()
                                && x.index().is_some()
                            {
                                for c in content.chars() {
                                    x.handle_input(c);
                                }
                            } else {
                                picker_ui.query.push_str(&content);
                            }
                        }
                    }
                }
                RenderCommand::Resize(area) => {
                    tui.resize(area);
                    ui.update_dimensions(area);
                }
                RenderCommand::Refresh => {
                    picker_ui.header.init();
                    footer_ui.init();
                    picker_ui.query.set_prompt(None);
                    picker_ui.results.set_status_line(None);
                    tui.redraw();
                }
                RenderCommand::PreviewChange => {
                    if let Some(p) = preview_ui.as_mut() {
                        p.view
                            .image_id
                            .fetch_add(1, std::sync::atomic::Ordering::Release);
                        p.view
                            .changed
                            .store(true, std::sync::atomic::Ordering::Release);
                    }
                    state.insert(crate::message::Event::PreviewChange);
                    state.needs_redraw = true;
                }
                RenderCommand::HeaderTable(columns) => {
                    picker_ui.header.header_table(columns);
                }
                RenderCommand::Mouse(mouse) => {
                    use crate::config::Side;
                    // we could also impl this in the aliasing step
                    let pos = Position::from((mouse.column, mouse.row));
                    let layout = state.layout;

                    match mouse.kind {
                        MouseEventKind::Down(MouseButton::Left) => {
                            if let Some(p) = preview_ui.as_mut()
                                && p.visible()
                            {
                                let gap_rect = layout.gap;
                                let is_in_drag_area = if !gap_rect.is_empty() {
                                    // Dedicated gap area: click anywhere in the gap to start drag.
                                    gap_rect.contains(pos)
                                } else {
                                    // Fallback: use the legacy drag_width border-edge approach.
                                    let drag_width = p.drag_width();
                                    if drag_width > 0 {
                                        let side = p
                                            .setting()
                                            .map(|s| &s.layout.side)
                                            .unwrap_or(&Side::Right);
                                        match side {
                                            Side::Right => {
                                                let drag_area = Rect {
                                                    x: layout.preview.x,
                                                    y: layout.preview.y,
                                                    width: drag_width,
                                                    height: layout.preview.height,
                                                };
                                                drag_area.contains(pos)
                                            }
                                            Side::Left => {
                                                let drag_area = Rect {
                                                    x: layout.preview.x
                                                        + layout
                                                            .preview
                                                            .width
                                                            .saturating_sub(drag_width),
                                                    y: layout.preview.y,
                                                    width: drag_width,
                                                    height: layout.preview.height,
                                                };
                                                drag_area.contains(pos)
                                            }
                                            Side::Bottom => {
                                                let drag_area = Rect {
                                                    x: layout.preview.x,
                                                    y: layout.preview.y,
                                                    width: layout.preview.width,
                                                    height: drag_width,
                                                };
                                                drag_area.contains(pos)
                                            }
                                            Side::Top => {
                                                let drag_area = Rect {
                                                    x: layout.preview.x,
                                                    y: layout.preview.y
                                                        + layout
                                                            .preview
                                                            .height
                                                            .saturating_sub(drag_width),
                                                    width: layout.preview.width,
                                                    height: drag_width,
                                                };
                                                drag_area.contains(pos)
                                            }
                                        }
                                    } else {
                                        false
                                    }
                                };

                                if is_in_drag_area {
                                    state.dragging = Some(pos);
                                    continue;
                                }
                            }

                            if layout.results.contains(pos) {
                                let y = mouse.row - layout.results.top();
                                debug!("Results clicked at: {y}");
                                click = Click::ResultPos(y);
                            } else if layout.input.contains(pos) {
                                // The X offset of the start of the visible text relative to the terminal
                                let text_start_x = layout.input.x + picker_ui.query.left();

                                if pos.x >= text_start_x {
                                    let visual_offset = pos.x - text_start_x;
                                    picker_ui.query.set_at_visual_offset(visual_offset);
                                } else {
                                    picker_ui.query.set(None, 0);
                                }
                            } else if layout.status.contains(pos) {
                                let x = pos.x.saturating_sub(layout.status.x);
                                debug!("Status clicked at x: {x}");
                                if let Some(action) = find_interaction(
                                    &picker_ui.results.status_config.interactions,
                                    x,
                                ) {
                                    click = Click::Semantic(action);
                                }
                            } else if layout.header.contains(pos) {
                                let rel_x = pos.x.saturating_sub(layout.header.x);
                                let rel_y = pos.y.saturating_sub(layout.header.y);
                                debug!("Header clicked at x: {rel_x}, y: {rel_y}");

                                if let Some(setting) =
                                    picker_ui.header.config.interactions.get(rel_y as usize)
                                    && let Some(action) = find_interaction(setting, rel_x)
                                {
                                    click = Click::Semantic(action);
                                }
                            } else if layout.footer.contains(pos) {
                                let rel_x = pos.x.saturating_sub(layout.footer.x);
                                let rel_y = pos.y.saturating_sub(layout.footer.y);
                                debug!("Footer clicked at x: {rel_x}, y: {rel_y}");

                                if let Some(setting) =
                                    footer_ui.config.interactions.get(rel_y as usize)
                                    && let Some(action) = find_interaction(setting, rel_x)
                                {
                                    click = Click::Semantic(action);
                                }
                            }
                        }
                        MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                            if layout.preview.contains(pos) {
                                if let Some(p) = preview_ui.as_mut() {
                                    if matches!(mouse.kind, MouseEventKind::ScrollDown) {
                                        p.down(1)
                                    } else {
                                        p.up(1)
                                    }
                                }
                            } else {
                                let next = matches!(mouse.kind, MouseEventKind::ScrollDown)
                                    ^ picker_ui.results.reverse();
                                did_cursor_wrap = if next {
                                    picker_ui.results.cursor_next()
                                } else {
                                    picker_ui.results.cursor_prev()
                                };
                            }
                        }
                        MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight => {
                            let left = matches!(mouse.kind, MouseEventKind::ScrollLeft);
                            if layout.preview.contains(pos) {
                                if let Some(p) = preview_ui.as_mut() {
                                    p.scroll(true, if left { -1 } else { 1 })
                                }
                            } else {
                                if !left
                                    || picker_ui.results.hscroll > 0
                                    || !picker_ui.query.input.is_empty()
                                {
                                    picker_ui
                                        .results
                                        .current_scroll(if left { -1 } else { 1 }, true);
                                }
                            }
                        }
                        MouseEventKind::Drag(MouseButton::Left) => {
                            if let Some(start_pos) = state.dragging
                                && let Some(p) = preview_ui.as_mut()
                            {
                                let side =
                                    p.setting().map(|s| &s.layout.side).unwrap_or(&Side::Right);
                                match side {
                                    Side::Right => {
                                        if pos.x < start_pos.x {
                                            p.expand(start_pos.x - pos.x);
                                        } else if pos.x > start_pos.x {
                                            p.shrink(pos.x - start_pos.x);
                                        }
                                    }
                                    Side::Left => {
                                        if pos.x > start_pos.x {
                                            p.expand(pos.x - start_pos.x);
                                        } else if pos.x < start_pos.x {
                                            p.shrink(start_pos.x - pos.x);
                                        }
                                    }
                                    Side::Bottom => {
                                        if pos.y < start_pos.y {
                                            p.expand(start_pos.y - pos.y);
                                        } else if pos.y > start_pos.y {
                                            p.shrink(pos.y - start_pos.y);
                                        }
                                    }
                                    Side::Top => {
                                        if pos.y > start_pos.y {
                                            p.expand(pos.y - start_pos.y);
                                        } else if pos.y < start_pos.y {
                                            p.shrink(start_pos.y - pos.y);
                                        }
                                    }
                                }
                                state.dragging = Some(pos);
                                state.needs_redraw = true;
                            }
                        }
                        MouseEventKind::Up(MouseButton::Left) => {
                            state.needs_redraw |= state.dragging.take().is_some();
                        }
                        MouseEventKind::Moved => {
                            let prev_hover = mouse_hover;
                            mouse_hover = Some(pos);
                            let gap_rect = layout.gap;
                            let was_over_gap = prev_hover.is_some_and(|p| gap_rect.contains(p));
                            let is_over_gap = gap_rect.contains(pos);
                            if was_over_gap != is_over_gap {
                                state.needs_redraw = true;
                            }
                        }
                        _ => {}
                    }
                }
                RenderCommand::NoMatch => {
                    return Err(MatchError::NoMatch);
                }
                RenderCommand::Empty => {
                    return Ok(vec![]);
                }
                RenderCommand::Action(action) | RenderCommand::KeyAction { action, .. } => {
                    if let Some(x) = overlay_ui.as_mut()
                        && match action {
                            Action::Char(c) => x.handle_input(c),
                            _ => x.handle_action(&action),
                        }
                    {
                        continue;
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
                    } = &mut picker_ui;
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
                                crate::PROMPT_INPUT_ACTIVE
                                    .store(false, std::sync::atomic::Ordering::Relaxed);
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
                                continue;
                            }
                            let ret = if selections.is_empty() {
                                if let Some(item) = get_current(&picker_ui) {
                                    vec![item.1]
                                } else if exit_config.allow_empty {
                                    vec![]
                                } else {
                                    continue;
                                }
                            } else {
                                selections.output().collect::<Vec<S>>()
                            };
                            return Ok(ret);
                        }
                        Action::Quit(code) => {
                            if state.live_input_active {
                                state.live_input_active = false;
                                *live_input_active = false;
                                crate::LIVE_INPUT_ACTIVE
                                    .store(false, std::sync::atomic::Ordering::Relaxed);
                                state.live_input_template = None;
                                state.live_input_key = None;
                                tui.redraw();
                                continue;
                            } else if state.prompt_input_active {
                                state.prompt_input_active = false;
                                *prompt_input_active = false;
                                crate::PROMPT_INPUT_ACTIVE
                                    .store(false, std::sync::atomic::Ordering::Relaxed);
                                if let Some(id) = state.prompt_input_stash_id.take() {
                                    let _ = state.take_actions(id);
                                }
                                state.prompt_input_template = None;
                                prompt_input.cancel();
                                tui.redraw();
                                continue;
                            } else if picker_ui.action_visible {
                                picker_ui.action_visible = false;
                                picker_ui.action.cancel();
                                crate::ACTION_BOX_ACTIVE
                                    .store(false, std::sync::atomic::Ordering::Relaxed);
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
                                return Err(MatchError::Abort(code));
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
                                did_cursor_wrap = if next {
                                    results.cursor_next()
                                } else {
                                    results.cursor_prev()
                                };
                            }
                        }
                        Action::Pos(pos) => {
                            let (_, status) =
                                crate::nucleo::Worker::new_snapshot(&mut worker.nucleo);
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
                            if let Some(p) = &mut preview_ui
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
                                did_cursor_wrap = if next {
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
                                    let (target, next_idx) =
                                        if matches!(action, Action::NextDiagram) {
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
                                            p.view.image_id.fetch_add(
                                                1,
                                                std::sync::atomic::Ordering::Release,
                                            );
                                            p.view.changed.store(
                                                true,
                                                std::sync::atomic::Ordering::Release,
                                            );
                                        }
                                    }
                                } else {
                                    let src_opt =
                                        p.view.diagram_sources.lock().ok().and_then(|sources| {
                                            if sources.is_empty() {
                                                return None;
                                            }
                                            let total = sources.len();
                                            let cur = p
                                                .view
                                                .current_diagram_idx
                                                .load(std::sync::atomic::Ordering::Relaxed);
                                            let next_idx = if matches!(action, Action::NextDiagram)
                                            {
                                                (cur + 1) % total
                                            } else {
                                                (cur + total.saturating_sub(1)) % total
                                            };
                                            p.view.current_diagram_idx.store(
                                                next_idx,
                                                std::sync::atomic::Ordering::Release,
                                            );
                                            sources.get(next_idx).cloned()
                                        });
                                    if let Some(src) = src_opt
                                        && let Some(img) = crate::utils::mermaid::render_mermaid_to_image_with_options(
                                            &src,
                                            2.0,
                                            p.config.diagrams.theme,
                                            p.config.diagrams.background,
                                        ) {
                                            if let Ok(mut guard) = p.view.image.lock() {
                                                *guard = Some(img);
                                            }
                                            p.reset_diagram_pan();
                                            p.view.image_id.fetch_add(
                                                1,
                                                std::sync::atomic::Ordering::Release,
                                            );
                                            p.view.changed.store(
                                                true,
                                                std::sync::atomic::Ordering::Release,
                                            );
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
                                if let RenderCommand::Action(a)
                                | RenderCommand::KeyAction { action: a, .. } = cmd
                                {
                                    remainder.push(a);
                                }
                            }

                            if let Some(id) = state.stash_actions(remainder, bind_tx.clone()) {
                                state.set_interrupt(Interrupt::ExecuteAsync, payload);
                                state.discriminant_payload =
                                    Some(2 * id + (if is_async { 1 } else { 0 }));
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
                                if let RenderCommand::Action(a)
                                | RenderCommand::KeyAction { action: a, .. } = cmd
                                {
                                    remainder.push(a);
                                }
                            }

                            state.prompt_input_stash_id =
                                state.stash_actions(remainder, bind_tx.clone());
                            state.prompt_input_template = Some(payload);
                            state.prompt_input_active = true;
                            *prompt_input_active = true;
                            crate::PROMPT_INPUT_ACTIVE
                                .store(true, std::sync::atomic::Ordering::Relaxed);
                            prompt_input.cancel();
                            tui.redraw();
                        }
                        Action::LiveInput(payload) => {
                            state.live_input_active = true;
                            *live_input_active = true;
                            crate::LIVE_INPUT_ACTIVE
                                .store(true, std::sync::atomic::Ordering::Relaxed);
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
                            let has_multiple_columns = num_columns > 1
                                || (num_columns == 1 && !worker.columns[0].name.is_empty());
                            if has_multiple_columns {
                                query.prepare_column_change();

                                let mut next_idx = match action {
                                    Action::NextColumn => active_idx + 1,
                                    Action::PrevColumn => {
                                        active_idx + num_columns - 1 % num_columns
                                    }
                                    _ => unreachable!(),
                                } % num_columns;

                                loop {
                                    if next_idx < results.hidden_columns.len()
                                        && results.hidden_columns[next_idx]
                                    {
                                        next_idx = match action {
                                            Action::NextColumn => (next_idx + 1) % num_columns,
                                            Action::PrevColumn => {
                                                (next_idx + num_columns - 1) % num_columns
                                            }
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
                                if let Some(idx) =
                                    worker.columns.iter().position(|c| *c.name == name)
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
                                && crate::ACTION_BOX_ACTIVE
                                    .load(std::sync::atomic::Ordering::Relaxed)
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
                                && crate::ACTION_BOX_ACTIVE
                                    .load(std::sync::atomic::Ordering::Relaxed)
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
                                && crate::ACTION_BOX_ACTIVE
                                    .load(std::sync::atomic::Ordering::Relaxed)
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
                                && crate::ACTION_BOX_ACTIVE
                                    .load(std::sync::atomic::Ordering::Relaxed)
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
                                && crate::ACTION_BOX_ACTIVE
                                    .load(std::sync::atomic::Ordering::Relaxed)
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
                                && crate::ACTION_BOX_ACTIVE
                                    .load(std::sync::atomic::Ordering::Relaxed)
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
                                && crate::ACTION_BOX_ACTIVE
                                    .load(std::sync::atomic::Ordering::Relaxed)
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
                                && crate::ACTION_BOX_ACTIVE
                                    .load(std::sync::atomic::Ordering::Relaxed)
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
                                && crate::ACTION_BOX_ACTIVE
                                    .load(std::sync::atomic::Ordering::Relaxed)
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
                                && crate::ACTION_BOX_ACTIVE
                                    .load(std::sync::atomic::Ordering::Relaxed)
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
                                && crate::ACTION_BOX_ACTIVE
                                    .load(std::sync::atomic::Ordering::Relaxed)
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
                                crate::LIVE_INPUT_ACTIVE
                                    .store(false, std::sync::atomic::Ordering::Relaxed);
                                state.live_input_template = None;
                                state.live_input_key = None;
                                tui.redraw();
                                continue;
                            } else if state.prompt_input_active {
                                state.prompt_input_active = false;
                                *prompt_input_active = false;
                                crate::PROMPT_INPUT_ACTIVE
                                    .store(false, std::sync::atomic::Ordering::Relaxed);
                                if let Some(id) = state.prompt_input_stash_id.take() {
                                    let _ = state.take_actions(id);
                                }
                                state.prompt_input_template = None;
                                prompt_input.cancel();
                                tui.redraw();
                                continue;
                            } else if *action_visible {
                                *action_visible = false;
                                action_input.cancel();
                                crate::ACTION_BOX_ACTIVE
                                    .store(false, std::sync::atomic::Ordering::Relaxed);
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
                                && crate::ACTION_BOX_ACTIVE
                                    .load(std::sync::atomic::Ordering::Relaxed)
                            {
                                *action_visible = false;
                                action_input.cancel();
                                crate::ACTION_BOX_ACTIVE
                                    .store(false, std::sync::atomic::Ordering::Relaxed);
                                tui.redraw();
                                continue;
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
                                        Focus::Results => {
                                            Some(ratatui::text::Line::raw(prompt.clone()))
                                        }
                                        Focus::Input => None,
                                    };
                                    picker_ui.query.set_prompt(prompt);
                                }
                            }
                        }
                        Action::FocusFilter => {
                            if *action_visible
                                && crate::ACTION_BOX_ACTIVE
                                    .load(std::sync::atomic::Ordering::Relaxed)
                            {
                                *action_visible = false;
                                action_input.cancel();
                                crate::ACTION_BOX_ACTIVE
                                    .store(false, std::sync::atomic::Ordering::Relaxed);
                                tui.redraw();
                                continue;
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
                                && crate::ACTION_BOX_ACTIVE
                                    .load(std::sync::atomic::Ordering::Relaxed)
                            {
                                *action_visible = false;
                                action_input.cancel();
                                crate::ACTION_BOX_ACTIVE
                                    .store(false, std::sync::atomic::Ordering::Relaxed);
                                tui.redraw();
                                continue;
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
                            if let Some(handler) = &mut ext_handler {
                                handler(
                                    e,
                                    &mut state.dispatcher(
                                        &mut ui,
                                        &mut picker_ui,
                                        &mut footer_ui,
                                        &mut preview_ui,
                                        &controller_tx,
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
                                        && crate::ACTION_BOX_ACTIVE
                                            .load(std::sync::atomic::Ordering::Relaxed);
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
                }
                _ => {}
            }

            let interrupt = state.interrupt();

            match interrupt {
                Interrupt::None => continue,
                Interrupt::Execute => {
                    // because of this, we don't want to send controller events until after resuming at batch end
                    if controller_tx.send(Event::Pause).is_err() {
                        break;
                    }
                    tui.enter_execute();
                    if did_exit.is_none() {
                        did_exit = Some(true);
                    }
                    did_pause = true;
                }
                Interrupt::Reload => {
                    state.synced = [false; 2];
                    did_reload = true;
                }
                Interrupt::ChDir => {
                    state.synced = [false; 2];
                }
                Interrupt::Become => {
                    tui.exit(None);
                }
                Interrupt::BecomeSilent => {
                    tui.exit_lite();
                }
                _ => {}
            }
            // Apply interrupt effect
            {
                let mut dispatcher = state.dispatcher(
                    &mut ui,
                    &mut picker_ui,
                    &mut footer_ui,
                    &mut preview_ui,
                    &controller_tx,
                );
                for h in dynamic_handlers.1.get_mut(interrupt) {
                    h(&mut dispatcher);
                }

                if matches!(interrupt, Interrupt::ChDir) {
                    state.refresh_parent_peek_cache();
                    if let Some(ref cwd) = state.cached_cwd {
                        let sort_order = ui.config.resolve_sort_for_dir(cwd);
                        picker_ui.worker.set_sort_order(sort_order);
                    }
                }

                if matches!(interrupt, Interrupt::Become) {
                    return Err(MatchError::Become(state.payload().clone()));
                }
            }

            if state.should_quit {
                log::debug!("Exiting due to should_quit");
                return if picker_ui.selector.is_disabled()
                    && let Some((_, item)) = get_current(&picker_ui)
                {
                    Ok(vec![item])
                } else {
                    Ok(picker_ui.selector.output().collect())
                };
            } else if state.should_quit_nomatch {
                log::debug!("Exiting due to should_quit_nomatch");
                return Err(MatchError::NoMatch);
            }
        }

        // debug!("{state:?}");

        // ------------- update state + render ------------------------
        if state.filtering {
            picker_ui.update();
        } else if did_reload || state.reloading {
            picker_ui.worker.refind();
            let mut iters = 0;
            while picker_ui.worker.nucleo.tick(10).running && iters < 50 {
                iters += 1;
            }
        }
        if did_cursor_wrap {
            log::trace!("cursor wrapped"); // todo: event handler?
        }

        // process exit conditions
        if exit_config.select_1
            && picker_ui.results.status.matched_count == 1
            && let Some((_, item)) = get_current(&picker_ui)
        {
            return Ok(vec![item]);
        }

        // resume tui
        if let Some(clear) = did_exit {
            tui.return_execute(clear)
                .map_err(|e| MatchError::TUIError(e.to_string()))?;
            tui.redraw();
            state.needs_redraw = true;
        }

        if did_reload || did_resize {
            state.needs_redraw = true;
        }

        if picker_ui
            .worker
            .matcher_dirty
            .swap(false, std::sync::atomic::Ordering::AcqRel)
        {
            state.needs_redraw = true;
        }

        if picker_ui.results.status.changed || picker_ui.results.status.running {
            state.needs_redraw = true;
        }

        if preview_ui.as_ref().is_some_and(|p| p.view.changed()) {
            state.needs_redraw = true;
        }

        if !picker_ui.results.config.spinner_prefix.is_empty() {
            state.needs_redraw = true;
        }

        if !picker_ui.results.flash_targets.is_empty() {
            let now = std::time::Instant::now();
            let prev_len = picker_ui.results.flash_targets.len();
            picker_ui.results.flash_targets.retain(|_, (start, _)| {
                now.duration_since(*start) < std::time::Duration::from_millis(1500)
            });
            if !picker_ui.results.flash_targets.is_empty()
                || prev_len != picker_ui.results.flash_targets.len()
            {
                state.needs_redraw = true;
            }
        }

        if ui.config.nav.active {
            let blink_ticks = ui.config.nav.blink_rate.ticks();
            let prev_blink = state.focus_blink;
            state.focus_tick = state.focus_tick.wrapping_add(1);
            if state.focus_tick >= blink_ticks {
                state.focus_tick = 0;
                state.focus_blink = !state.focus_blink;
            }
            if prev_blink != state.focus_blink {
                state.needs_redraw = true;
            }
        }

        if state.needs_redraw {
            let mut overlay_ui_ref = overlay_ui.as_mut();
            let mut cursor_y_offset = 0;

            tui.terminal
                .draw(|frame| {
                    let mut area = frame.area();

                    // mutates area!
                    render_ui(frame, &mut area, &ui);

                    let mut _area = area;

                    if let Some(p) = preview_ui.as_mut() {
                        if state.live_input_active {
                            p.set_border_color_override(Some(Color::Green));
                        } else {
                            p.set_border_color_override(None);
                        }
                    }

                    let full_width_footer = footer_ui.is_single_column()
                        && footer_ui.config.row_connection == RowConnectionStyle::Full;

                    let mut breadcrumb_spans = Vec::new();
                    let mut is_global_breadcrumb = false;
                    let mut global_breadcrumb_rect = Rect::default();

                    if picker_ui.breadcrumb_config.show
                        && let Some(ref cwd) = state.cached_cwd
                    {
                        let mut components = Vec::new();
                        let home_dir = std::env::var("HOME").ok().map(std::path::PathBuf::from);

                        if let Some(home) = home_dir
                            && let Ok(stripped) = cwd.strip_prefix(&home)
                        {
                            components.push("~".to_string());
                            for comp in stripped.components() {
                                components.push(comp.as_os_str().to_string_lossy().to_string());
                            }
                        }

                        if components.is_empty() {
                            for comp in cwd.components() {
                                components.push(comp.as_os_str().to_string_lossy().to_string());
                            }
                        }

                        if picker_ui.breadcrumb_config.current_folder_only && !components.is_empty()
                        {
                            let last = components.pop().unwrap();
                            components.clear();
                            components.push(last);
                        }

                        let truncate_len = picker_ui.breadcrumb_config.truncate_length;
                        let num_components = components.len();
                        let mut breadcrumb_width = 0;

                        for (i, mut text) in components.into_iter().enumerate() {
                            if truncate_len > 0
                                && i < num_components.saturating_sub(1)
                                && text != "~"
                                && text.chars().count() > truncate_len
                            {
                                text = text.chars().take(truncate_len).collect();
                            }

                            breadcrumb_width += text.chars().count() as u16;
                            breadcrumb_spans.push(ratatui::text::Span::styled(
                                text,
                                ratatui::style::Style::from(picker_ui.breadcrumb_config.style),
                            ));
                            if i < num_components - 1 {
                                let sep = picker_ui.breadcrumb_config.separator.clone();
                                breadcrumb_width += sep.chars().count() as u16;
                                breadcrumb_spans.push(ratatui::text::Span::styled(
                                    sep,
                                    ratatui::style::Style::from(
                                        picker_ui.breadcrumb_config.separator_style,
                                    ),
                                ));
                            }
                        }

                        // Determine if we need global breadcrumb
                        let has_preview = preview_ui.as_ref().is_some_and(|p| p.visible());
                        let picker_w = if has_preview {
                            let [_, p_area, _] = preview_ui.as_ref().unwrap().split(_area);
                            p_area.width
                        } else {
                            _area.width
                        };

                        if has_preview || breadcrumb_width > picker_w {
                            is_global_breadcrumb = true;
                            global_breadcrumb_rect = split(&mut _area, 1, !picker_ui.reverse());
                        }
                    }

                    let is_filter_mode = state.focus == Focus::Input;
                    let show_sort_menu = state.sort_menu_active;
                    let show_nav_hints = if state.preview_fullscreen {
                        true
                    } else {
                        ui.config.nav.active && ui.config.nav.hints
                    };

                    let has_footer_separator =
                        footer_ui.config.separator != crate::config::HorizontalSeparator::None;

                    let base_footer_height = if state.preview_fullscreen {
                        ui.config.nav_hints_height(PREVIEW_NAV_HINTS.len()).max(1)
                    } else if show_sort_menu {
                        ui.config.sort_menu.height(SORT_MENU_ITEMS.len())
                    } else if show_nav_hints {
                        let count = if is_filter_mode {
                            FILTER_NAV_HINTS.len()
                        } else {
                            match ui.config.nav.effective_profile() {
                                crate::config::NavProfile::Fm => NAV_HINTS.len(),
                                crate::config::NavProfile::List => LIST_NAV_HINTS.len(),
                                crate::config::NavProfile::Basic => BASIC_NAV_HINTS.len(),
                                crate::config::NavProfile::None => 0,
                            }
                        };
                        ui.config.nav_hints_height(count)
                    } else if footer_ui.show {
                        footer_ui.content_height()
                    } else {
                        0
                    };

                    let effective_footer_height = if base_footer_height > 0 && has_footer_separator
                    {
                        base_footer_height + 1
                    } else {
                        base_footer_height
                    };

                    let is_full_footer = full_width_footer || show_nav_hints || show_sort_menu;

                    let mut footer =
                        if is_full_footer || preview_ui.as_ref().is_none_or(|p| !p.visible()) {
                            split(&mut _area, effective_footer_height, picker_ui.reverse())
                        } else {
                            Rect::default()
                        };

                    // Compute how wide the gap needs to be to show the counter.
                    // Rules:
                    //   - suppress selected if sel_count == yank_count (fully covered by yank)
                    //   - suppress yank    if yank_count == cut_count  (fully covered by cut)
                    //   - if only one group: horizontal, width = " N " of that group
                    //   - if multiple groups: vertical (one row each), width = widest " N "
                    let _counter_gap_width: u16 = {
                        let sel_raw = picker_ui.selector.len();
                        let yank_raw = picker_ui.results.yank_paths.len();
                        let cut_raw = picker_ui.results.cut_paths.len();

                        let show_cut = cut_raw > 0;
                        let show_yank = yank_raw > 0 && yank_raw != cut_raw;
                        let show_sel = sel_raw > 0 && sel_raw != yank_raw && sel_raw != cut_raw;

                        let widths: Vec<usize> = [
                            show_cut.then(|| cut_raw.to_string().len() + 2),
                            show_yank.then(|| yank_raw.to_string().len() + 2),
                            show_sel.then(|| sel_raw.to_string().len() + 2),
                        ]
                        .into_iter()
                        .flatten()
                        .collect();

                        widths.iter().copied().max().unwrap_or(0) as u16
                    };

                    let mut parent_peek_rect = Rect::default();
                    if ui.config.parent_peek.enabled && _area.width >= 50 {
                        let top_offset = if !picker_ui.reverse() && picker_ui.query.config.show {
                            1 + picker_ui.query.config.border.height()
                        } else {
                            0
                        };
                        let pw = ui
                            .config
                            .parent_peek
                            .pct
                            .compute_clamped(_area.width, 10, 30);
                        parent_peek_rect = Rect {
                            x: _area.x,
                            y: _area.y + top_offset,
                            width: pw,
                            height: _area.height.saturating_sub(top_offset),
                        };
                        _area.x += pw;
                        _area.width -= pw;
                    }

                    let [preview, picker_area, footer, gap_area] = if let Some(preview_ui) =
                        preview_ui.as_mut()
                        && preview_ui.visible()
                    {
                        if state.preview_fullscreen {
                            let (preview, footer) = if footer.height > 0 {
                                (_area, footer)
                            } else {
                                let mut full_area = _area;
                                let f = split(&mut full_area, effective_footer_height, false);
                                (full_area, f)
                            };
                            [preview, Rect::default(), footer, Rect::default()]
                        } else {
                            // Temporarily widen the gap so the counter fits horizontally.
                            let original_gap =
                                preview_ui.setting().map(|s| s.layout.gap).unwrap_or(0);
                            let effective_gap = original_gap.max(if _counter_gap_width > 0 {
                                _counter_gap_width + 2
                            } else {
                                0
                            });
                            if let Some(s) = preview_ui.setting_mut() {
                                s.layout.gap = effective_gap;
                            }
                            let [preview, mut picker_area, gap_area] = preview_ui.split(_area);
                            // Restore the configured gap so nothing else is affected.
                            if let Some(s) = preview_ui.setting_mut() {
                                s.layout.gap = original_gap;
                            }

                            if state.iterations == 0 && _area.width < 30 && picker_area.width <= 5 {
                                warn!("UI too narrow, hiding preview");
                                preview_ui.show(false);

                                [Rect::default(), _area, footer, Rect::default()]
                            } else {
                                if !is_full_footer {
                                    footer = split(
                                        &mut picker_area,
                                        effective_footer_height,
                                        picker_ui.reverse(),
                                    );
                                }

                                [preview, picker_area, footer, gap_area]
                            }
                        }
                    } else {
                        [Rect::default(), _area, footer, Rect::default()]
                    };

                    let original_breadcrumb_show = picker_ui.breadcrumb_config.show;
                    if is_global_breadcrumb {
                        picker_ui.breadcrumb_config.show = false;
                    }
                    let [breadcrumb, action, input, status, header, results] =
                        picker_ui.layout(picker_area);
                    if is_global_breadcrumb {
                        picker_ui.breadcrumb_config.show = original_breadcrumb_show;
                    }

                    let mut footer = footer;
                    if !picker_ui.reverse() && footer.height > 0 {
                        let picker_bottom = results.y + results.height;
                        let preview_bottom = if preview.height > 0 {
                            preview.y + preview.height
                        } else {
                            0
                        };
                        let content_bottom = picker_bottom.max(preview_bottom);
                        if content_bottom < footer.y {
                            footer.y = content_bottom;
                        }
                    }

                    if parent_peek_rect.width > 0 {
                        if footer.height > 0 {
                            let available_h = footer.y.saturating_sub(parent_peek_rect.y);
                            parent_peek_rect.height = parent_peek_rect.height.min(available_h);
                        }
                        if let Some(max_h) = ui.config.parent_peek.max {
                            parent_peek_rect.height = parent_peek_rect.height.min(max_h);
                        }
                    }

                    // save dimensions and check if dimensions changed
                    did_resize = state.update_layout(Layout {
                        preview,
                        action,
                        input,
                        status,
                        header,
                        results,
                        footer,
                        gap: gap_area,
                        pane: _area,
                    });

                    if did_resize {
                        picker_ui.results.update_dimensions(&results);
                        picker_ui.action.update_width(action.width);
                        picker_ui.query.update_width(input.width);
                        footer_ui.update_width(
                            if footer_ui.config.row_connection == RowConnectionStyle::Capped {
                                area.width
                            } else {
                                footer.width
                            },
                        );
                        picker_ui.header.update_width(header.width);
                        // although these only want update when the whole ui change
                        ui.update_dimensions(area);
                        if let Some(x) = overlay_ui_ref.as_deref_mut() {
                            x.update_dimensions(&area);
                        }
                        if let Some(preview_ui) = preview_ui.as_mut() {
                            preview_ui.update_dimensions(&preview);
                        }
                    };

                    let status_inline_label: Option<Line<'_>> =
                        if picker_ui.query.config.status_inline {
                            Some(picker_ui.results.status_line())
                        } else {
                            None
                        };

                    let nav_color = ui.config.nav.color;
                    let nav_mode = ui.config.nav.active;
                    let nav_do_blink = ui.config.nav.blink;
                    let nav_bold = ui.config.nav.bold;
                    let nav_bar = ui.config.nav.bar;
                    let nav_marker = ui.config.nav.marker.clone();
                    let nav_char = picker_ui
                        .results
                        .config
                        .multi_prefix
                        .chars()
                        .next()
                        .unwrap_or('│')
                        .to_string();
                    let input_focus_info = nav_mode.then_some(FocusInfo {
                        focused: state.focus == Focus::Input,
                        blink_phase: state.focus_blink,
                        color: nav_color,
                        do_blink: nav_do_blink,
                        bold: nav_bold,
                        bar: None,
                        marker: String::new(),
                        nav_char: nav_char.clone(),
                        nav_prompt: ui.config.nav.prompt.clone(),
                    });
                    let results_focus_info = nav_mode.then_some(FocusInfo {
                        focused: state.focus == Focus::Results,
                        blink_phase: state.focus_blink,
                        color: nav_color,
                        do_blink: nav_do_blink,
                        bold: nav_bold,
                        bar: nav_bar,
                        marker: nav_marker,
                        nav_char,
                        nav_prompt: ui.config.nav.prompt.clone(),
                    });

                    if picker_ui.action_visible {
                        let cfg = &picker_ui.action_config;
                        // Apply width percentage — centered within the allocated row(s).
                        let action_w = cfg.width_pct.compute_clamped(action.width, 1, 0);
                        let x_pad = action.width.saturating_sub(action_w) / 2;
                        let action_full_rect = Rect {
                            x: action.x + x_pad,
                            y: action.y,
                            width: action_w,
                            height: action.height,
                        };
                        // Render the separator border (default: bottom line) over the full area.
                        // It only draws at the edges of `action_full_rect`, so the input and
                        // preview content rendered afterwards is not obscured.
                        if !cfg.border.is_empty() {
                            frame.render_widget(cfg.border.as_static_block(), action_full_rect);
                        }

                        let mut current_y = action_full_rect.y;

                        let action_input_rect = Rect {
                            x: action_full_rect.x,
                            y: current_y,
                            width: action_w,
                            height: action_full_rect.height.min(1),
                        };
                        render_input(frame, action_input_rect, &mut picker_ui.action, None, None);
                        current_y += 1;

                        // Render preview area below if preview_height > 0.
                        if cfg.preview_height > 0
                            && action_full_rect.height > current_y - action_full_rect.y
                        {
                            let preview_rect = Rect {
                                x: action_full_rect.x,
                                y: current_y,
                                width: action_w,
                                height: cfg.preview_height,
                            };
                            let block = ratatui::widgets::Block::bordered();
                            frame.render_widget(block, preview_rect);
                        }
                    }

                    if !state.preview_fullscreen {
                        if picker_ui.breadcrumb_config.show && !breadcrumb_spans.is_empty() {
                            let target_rect = if is_global_breadcrumb {
                                global_breadcrumb_rect
                            } else {
                                breadcrumb
                            };

                            if target_rect.height > 0 {
                                let p = ratatui::widgets::Paragraph::new(
                                    ratatui::text::Line::from(breadcrumb_spans),
                                );
                                frame.render_widget(p, target_rect);
                            }
                        }
                        if state.live_input_active {
                            let line = if input.width >= 48 {
                                ratatui::text::Line::from(vec![
                                    ratatui::text::Span::styled(
                                        " 󰌌 LIVE INPUT ",
                                        ratatui::style::Style::default()
                                            .fg(ratatui::style::Color::Black)
                                            .bg(ratatui::style::Color::Green)
                                            .add_modifier(ratatui::style::Modifier::BOLD),
                                    ),
                                    ratatui::text::Span::raw(" "),
                                    ratatui::text::Span::styled(
                                        "streaming keys to agent",
                                        ratatui::style::Style::default()
                                            .fg(ratatui::style::Color::Green)
                                            .add_modifier(ratatui::style::Modifier::BOLD),
                                    ),
                                    ratatui::text::Span::raw("  •  "),
                                    ratatui::text::Span::styled(
                                        "[Esc]",
                                        ratatui::style::Style::default()
                                            .fg(ratatui::style::Color::DarkGray)
                                            .add_modifier(ratatui::style::Modifier::BOLD),
                                    ),
                                    ratatui::text::Span::styled(
                                        " exit",
                                        ratatui::style::Style::default()
                                            .fg(ratatui::style::Color::DarkGray),
                                    ),
                                ])
                            } else {
                                ratatui::text::Line::from(vec![
                                    ratatui::text::Span::styled(
                                        " 󰌌 LIVE ",
                                        ratatui::style::Style::default()
                                            .fg(ratatui::style::Color::Black)
                                            .bg(ratatui::style::Color::Green)
                                            .add_modifier(ratatui::style::Modifier::BOLD),
                                    ),
                                    ratatui::text::Span::raw(" "),
                                    ratatui::text::Span::styled(
                                        "streaming",
                                        ratatui::style::Style::default()
                                            .fg(ratatui::style::Color::Green)
                                            .add_modifier(ratatui::style::Modifier::BOLD),
                                    ),
                                    ratatui::text::Span::raw(" • "),
                                    ratatui::text::Span::styled(
                                        "[Esc]",
                                        ratatui::style::Style::default()
                                            .fg(ratatui::style::Color::DarkGray)
                                            .add_modifier(ratatui::style::Modifier::BOLD),
                                    ),
                                    ratatui::text::Span::styled(
                                        " exit",
                                        ratatui::style::Style::default()
                                            .fg(ratatui::style::Color::DarkGray),
                                    ),
                                ])
                            };
                            frame.render_widget(ratatui::widgets::Paragraph::new(line), input);
                            cursor_y_offset = input.y;
                        } else if state.prompt_input_active {
                            cursor_y_offset = render_input(
                                frame,
                                input,
                                &mut picker_ui.prompt_input,
                                None,
                                Some(FocusInfo {
                                    focused: true,
                                    blink_phase: state.focus_blink,
                                    color: nav_color,
                                    do_blink: false,
                                    bold: true,
                                    bar: None,
                                    marker: String::new(),
                                    nav_char: String::new(),
                                    nav_prompt: String::new(),
                                }),
                            )
                            .y;
                        } else if picker_ui.query.config.show {
                            cursor_y_offset = render_input(
                                frame,
                                input,
                                &mut picker_ui.query,
                                status_inline_label,
                                input_focus_info,
                            )
                            .y;
                        } else {
                            cursor_y_offset = input.y;
                        }
                        // When status_inline is active, skip the separate status row.
                        if !picker_ui.query.config.status_inline {
                            render_status(frame, status, &picker_ui.results, ui.area().width);
                        }
                        render_results(
                            frame,
                            results,
                            &mut picker_ui,
                            &mut click,
                            results_focus_info,
                            state.reloading,
                        );
                        render_display(frame, header, &mut picker_ui.header, &picker_ui.results);
                        if parent_peek_rect.width > 0 {
                            if state.parent_peek_cache.is_none() {
                                state.refresh_parent_peek_cache();
                            }
                            render_parent_peek(
                                frame,
                                parent_peek_rect,
                                &ui.config.parent_peek,
                                state.parent_peek_cache.as_ref(),
                            );
                        }
                    }

                    if footer.height > 0 {
                        let (separator_rect, content_rect) =
                            if has_footer_separator && footer.height > 1 {
                                (
                                    Rect {
                                        x: footer.x,
                                        y: footer.y,
                                        width: footer.width,
                                        height: 1,
                                    },
                                    Rect {
                                        x: footer.x,
                                        y: footer.y + 1,
                                        width: footer.width,
                                        height: footer.height.saturating_sub(1),
                                    },
                                )
                            } else if has_footer_separator && footer.height == 1 {
                                (footer, Rect::default())
                            } else {
                                (Rect::default(), footer)
                            };

                        if separator_rect.height > 0 {
                            let preview_junction_x = if state.preview_fullscreen
                                || preview_ui.as_ref().is_none_or(|p| !p.visible())
                            {
                                None
                            } else if preview.width > 0 && picker_area.width > 0 {
                                if preview.x > picker_area.x {
                                    Some(preview.x)
                                } else {
                                    let has_right_border = preview_ui
                                        .as_ref()
                                        .and_then(|p| p.active_border())
                                        .is_some_and(|b| b.sides().contains(Borders::RIGHT));
                                    if has_right_border {
                                        Some(preview.x + preview.width.saturating_sub(1))
                                    } else {
                                        Some(preview.x + preview.width)
                                    }
                                }
                            } else {
                                None
                            };

                            let mut footer_junctions = Vec::new();
                            if parent_peek_rect.width > 0 && ui.config.parent_peek.border.show {
                                footer_junctions.push(
                                    parent_peek_rect.x + parent_peek_rect.width.saturating_sub(1),
                                );
                            }
                            if let Some(jx) = preview_junction_x {
                                footer_junctions.push(jx);
                            }

                            let preview_border = preview_ui.as_ref().map(|p| p.border());

                            let preview_border_color =
                                preview_ui.as_ref().and_then(|p| p.effective_border_color());

                            let preview_border_type = preview_border.and_then(|b| b.r#type);

                            let sep_fg = footer_ui
                                .config
                                .separator_style
                                .fg
                                .or(preview_border_color)
                                .unwrap_or(Color::DarkGray);

                            let sep_style =
                                Style::from(footer_ui.config.separator_style).fg(sep_fg);

                            render_footer_separator(
                                frame,
                                separator_rect,
                                footer_ui.config.separator,
                                &footer_junctions,
                                preview_border_type,
                                sep_style,
                            );
                        }

                        if content_rect.height > 0 {
                            if state.preview_fullscreen {
                                let zoom_pct =
                                    preview_ui.as_ref().map(|p| (p.zoom * 100.0).round() as u32);
                                render_nav_hints(
                                    frame,
                                    content_rect,
                                    ui.config.nav.effective_profile(),
                                    ui.config.nav.hints_columns,
                                    true,
                                    zoom_pct,
                                    false,
                                );
                            } else if show_sort_menu {
                                render_sort_menu(frame, content_rect, &ui.config.sort_menu);
                            } else if show_nav_hints {
                                render_nav_hints(
                                    frame,
                                    content_rect,
                                    ui.config.nav.effective_profile(),
                                    ui.config.nav.hints_columns,
                                    false,
                                    None,
                                    is_filter_mode,
                                );
                            } else if footer_ui.show {
                                render_display(
                                    frame,
                                    content_rect,
                                    &mut footer_ui,
                                    &picker_ui.results,
                                );
                            }
                        }
                    }
                    if let Some(preview_ui) = preview_ui.as_mut() {
                        state.update_preview_visible(preview_ui);
                        if preview_ui.visible() {
                            // Set the dynamic title from the first column of the current item.
                            let item_title = picker_ui
                                .worker
                                .get_nth(picker_ui.results.index())
                                .map(|item| picker_ui.worker.columns[0].raw(item).into_owned());
                            preview_ui.set_title(item_title);
                            render_preview(frame, preview, preview_ui);

                            // Highlight the gap area when the mouse is hovering over it.
                            if !gap_area.is_empty() {
                                let is_hovered = mouse_hover.is_some_and(|p| gap_area.contains(p));
                                let is_dragging = state.dragging.is_some();
                                if is_hovered || is_dragging {
                                    let buf = frame.buffer_mut();
                                    let shadow_style = Style::default().bg(Color::DarkGray);
                                    for y in gap_area.top()..gap_area.bottom() {
                                        for x in gap_area.left()..gap_area.right() {
                                            if let Some(cell) = buf.cell_mut((x, y)) {
                                                cell.set_char(' ');
                                                cell.set_style(shadow_style);
                                            }
                                        }
                                    }
                                }

                                // Counter bar: render cut / yank / selected counts.
                                // - suppress selected if fully covered by yank (same count)
                                // - suppress yank    if fully covered by cut  (same count)
                                // - single group → one horizontal row
                                // - multiple groups → stacked vertically, one row per group
                                // - positioned 5 rows from the bottom of the gap
                                let sel_raw = picker_ui.selector.len();
                                let yank_raw = picker_ui.results.yank_paths.len();
                                let cut_raw = picker_ui.results.cut_paths.len();

                                let show_cut = cut_raw > 0;
                                let show_yank = yank_raw > 0 && yank_raw != cut_raw;
                                let show_sel =
                                    sel_raw > 0 && sel_raw != yank_raw && sel_raw != cut_raw;

                                // Collect the groups to render (cut first = highest priority).
                                let rcfg = &picker_ui.results.config;
                                let groups: Vec<(usize, Color)> = [
                                    show_cut.then(|| {
                                        (cut_raw, rcfg.cut_prefix_style.fg.unwrap_or(Color::Red))
                                    }),
                                    show_yank.then(|| {
                                        (
                                            yank_raw,
                                            rcfg.yank_prefix_style.fg.unwrap_or(Color::Yellow),
                                        )
                                    }),
                                    show_sel.then(|| {
                                        (
                                            sel_raw,
                                            rcfg.selected_prefix_style.fg.unwrap_or(Color::Cyan),
                                        )
                                    }),
                                ]
                                .into_iter()
                                .flatten()
                                .collect();

                                if !groups.is_empty() {
                                    let n_rows = groups.len() as u16;
                                    // Position from the top of the gap using the configured offset.
                                    let offset_from_top = preview_ui
                                        .setting()
                                        .map(|s| s.layout.gap_counter_offset)
                                        .unwrap_or(0);
                                    let start_y = (gap_area.y + offset_from_top)
                                        .min(gap_area.y + gap_area.height.saturating_sub(n_rows));

                                    for (row_idx, (count, bg)) in groups.iter().enumerate() {
                                        let row_y = start_y + row_idx as u16;
                                        if row_y >= gap_area.y + gap_area.height {
                                            break;
                                        }
                                        let row_rect = Rect {
                                            x: gap_area.x,
                                            y: row_y,
                                            width: gap_area.width,
                                            height: 1,
                                        };
                                        let span = Span::styled(
                                            format!(" {} ", count),
                                            Style::default().fg(Color::Black).bg(*bg),
                                        );
                                        frame.render_widget(
                                            Paragraph::new(Line::from(span))
                                                .alignment(ratatui::layout::Alignment::Center),
                                            row_rect,
                                        );
                                    }
                                }
                            }
                        }
                    }
                    if let Some(x) = overlay_ui_ref {
                        x.draw(frame);
                    }
                    if crate::ui::no_color() {
                        crate::ui::strip_colors(frame.buffer_mut());
                    }
                })
                .map_err(|e| MatchError::TUIError(e.to_string()))?;
            state.needs_redraw = false;
        }

        if did_resize {
            // useful to clear artifacts
            if tui.config.redraw_on_resize && did_exit.is_none() {
                tui.redraw();
            }
        }

        drop(events);
        buffer.clear();

        // note: the remainder could be scoped by a conditional on having run?
        // ====== Event handling ==========
        state.update(&picker_ui, &overlay_ui);

        let events = state.events();

        // ---- Invoke handlers -------
        let mut dispatcher = state.dispatcher(
            &mut ui,
            &mut picker_ui,
            &mut footer_ui,
            &mut preview_ui,
            &controller_tx,
        );
        // if let Some((signal, handler)) = signal_handler &&
        // let s = signal.load(std::sync::atomic::Ordering::Acquire) &&
        // s > 0
        // {
        //     handler(s, &mut dispatcher);
        //     signal.store(0, std::sync::atomic::Ordering::Release);
        // };

        // ping handlers with events
        for e in events.iter() {
            for h in dynamic_handlers.0.get(e) {
                h(&mut dispatcher, &e)
            }
        }
        state.reset();

        // ------------------------------
        // send events into controller
        for e in events.iter() {
            if !matches!(e, Event::PreviewChange) {
                controller_tx.send(e)._elog();
            }
        }
        // =================================

        if did_pause {
            log::debug!("Waiting for ack response to pause");
            if controller_tx.send(Event::Resume).is_err() {
                break;
            };
            // due to control flow, this does nothing, but is anyhow a useful safeguard to guarantee the pause
            while let Some(msg) = render_rx.recv().await {
                if matches!(msg, RenderCommand::Ack) {
                    log::debug!("Received ack response to pause");
                    break;
                }
            }
        }
        if did_reload {
            controller_tx.send(Event::Reloaded)._elog();
        }

        click.process(&mut buffer, &bind_tx);
    }

    Err(MatchError::EventLoopClosed)
}
