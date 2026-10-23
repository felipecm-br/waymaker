mod actions;
mod draw;
mod dynamic;
mod frame;
mod keys;
mod live_input;
mod mouse;
mod state;
#[cfg(test)]
mod tests;

use actions::{ActionCtx, handle_action};
use cba::bait::ResultExt;
use draw::*;
pub use draw::{
    BASIC_NAV_HINTS, Click, FILTER_NAV_HINTS, LIST_NAV_HINTS, NAV_HINTS, PREVIEW_NAV_HINTS,
};
pub use dynamic::*;
use frame::{DrawCtx, draw_frame};
use keys::*;
pub use live_input::quote_live_token;
use live_input::*;
use mouse::{MouseCtx, handle_mouse};
pub use state::*;
// ------------------------------

use std::io::Write;

use log::{info, warn};
use ratatui::layout::Position;
use ratatui::text::Line;
use tokio::sync::mpsc;

#[cfg(feature = "bracketed-paste")]
use crate::PasteHandler;
use crate::action::ActionExt;
use crate::config::{ExitConfig, RowConnectionStyle};
use crate::event::{BindSender, EventSender};
use crate::message::{Event, Interrupt, RenderCommand};
use crate::tui::Tui;
use crate::ui::{DisplayUI, OverlayUI, PickerUI, PreviewUI, UI};
use crate::{ActionAliaser, ActionExtHandler, Initializer, MatchError, SSS, Selection};
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use ratatui::widgets::{Borders, Paragraph};

/// What the render loop does after an event handler has run.
pub(super) enum Flow<S> {
    /// Go straight to the next event, skipping interrupt handling.
    Continue,
    /// Fall through to interrupt handling for this event.
    Proceed,
    /// Leave the render loop with this result.
    Exit(Result<Vec<S>, MatchError>),
}

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
                    let cx = MouseCtx {
                        state: &mut state,
                        picker_ui: &mut picker_ui,
                        footer_ui: &footer_ui,
                        preview_ui: &mut preview_ui,
                        click: &mut click,
                        mouse_hover: &mut mouse_hover,
                        did_cursor_wrap: &mut did_cursor_wrap,
                    };
                    if let Flow::Continue = handle_mouse(mouse, cx) {
                        continue;
                    }
                }
                RenderCommand::NoMatch => {
                    return Err(MatchError::NoMatch);
                }
                RenderCommand::Empty => {
                    return Ok(vec![]);
                }
                RenderCommand::Action(action) | RenderCommand::KeyAction { action, .. } => {
                    let cx = ActionCtx {
                        state: &mut state,
                        ui: &mut ui,
                        picker_ui: &mut picker_ui,
                        footer_ui: &mut footer_ui,
                        preview_ui: &mut preview_ui,
                        tui: &mut tui,
                        overlay_ui: &mut overlay_ui,
                        ext_handler: &mut ext_handler,
                        did_cursor_wrap: &mut did_cursor_wrap,
                        exit_config: &exit_config,
                        controller_tx: &controller_tx,
                        bind_tx: &bind_tx,
                    };
                    match handle_action(action, &mut events, cx) {
                        Flow::Continue => continue,
                        Flow::Exit(result) => return result,
                        Flow::Proceed => {}
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
            tui.terminal
                .draw(|frame| {
                    draw_frame(
                        frame,
                        DrawCtx {
                            state: &mut state,
                            ui: &mut ui,
                            picker_ui: &mut picker_ui,
                            footer_ui: &mut footer_ui,
                            preview_ui: &mut preview_ui,
                            overlay_ui: overlay_ui.as_mut(),
                            click: &mut click,
                            mouse_hover,
                            did_resize: &mut did_resize,
                        },
                    )
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
