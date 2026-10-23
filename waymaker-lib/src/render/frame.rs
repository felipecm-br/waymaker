//! Drawing one frame of the picker: layout, results, input, preview, footer and overlays.

use ratatui::Frame;
use ratatui::layout::{Position, Rect};

use super::*;
use crate::ui::{DisplayUI, OverlayUI, PickerUI, PreviewUI, UI};

/// The render loop's state that drawing reads or mutates.
pub(super) struct DrawCtx<'r, 'a, T: SSS, S: Selection, A: ActionExt> {
    pub state: &'r mut State,
    pub ui: &'r mut UI,
    pub picker_ui: &'r mut PickerUI<'a, T, S>,
    pub footer_ui: &'r mut DisplayUI,
    pub preview_ui: &'r mut Option<PreviewUI>,
    pub overlay_ui: Option<&'r mut OverlayUI<A>>,
    pub click: &'r mut Click,
    pub mouse_hover: Option<Position>,
    pub did_resize: &'r mut bool,
}

pub(super) fn draw_frame<T: SSS, S: Selection, A: ActionExt>(
    frame: &mut Frame,
    cx: DrawCtx<'_, '_, T, S, A>,
) {
    let DrawCtx {
        state,
        ui,
        picker_ui,
        footer_ui,
        preview_ui,
        overlay_ui: mut overlay_ui_ref,
        click,
        mouse_hover,
        did_resize,
    } = cx;

    let mut area = frame.area();

    // mutates area!
    render_ui(frame, &mut area, ui);

    let mut _area = area;

    if let Some(p) = preview_ui.as_mut() {
        if state.live_input_active {
            p.set_border_color_override(Some(Color::Green));
        } else {
            p.set_border_color_override(None);
        }
    }

    let full_width_footer =
        footer_ui.is_single_column() && footer_ui.config.row_connection == RowConnectionStyle::Full;

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

        if picker_ui.breadcrumb_config.current_folder_only && !components.is_empty() {
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
                    ratatui::style::Style::from(picker_ui.breadcrumb_config.separator_style),
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

    let effective_footer_height = if base_footer_height > 0 && has_footer_separator {
        base_footer_height + 1
    } else {
        base_footer_height
    };

    let is_full_footer = full_width_footer || show_nav_hints || show_sort_menu;

    let mut footer = if is_full_footer || preview_ui.as_ref().is_none_or(|p| !p.visible()) {
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

    let [preview, picker_area, footer, gap_area] = if let Some(preview_ui) = preview_ui.as_mut()
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
            let original_gap = preview_ui.setting().map(|s| s.layout.gap).unwrap_or(0);
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
    let [breadcrumb, action, input, status, header, results] = picker_ui.layout(picker_area);
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
    *did_resize = state.update_layout(Layout {
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

    if *did_resize {
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

    let status_inline_label: Option<Line<'_>> = if picker_ui.query.config.status_inline {
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
        if cfg.preview_height > 0 && action_full_rect.height > current_y - action_full_rect.y {
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
                let p =
                    ratatui::widgets::Paragraph::new(ratatui::text::Line::from(breadcrumb_spans));
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
                        ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray),
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
                        ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray),
                    ),
                ])
            };
            frame.render_widget(ratatui::widgets::Paragraph::new(line), input);
        } else if state.prompt_input_active {
            render_input(
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
            );
        } else if picker_ui.query.config.show {
            render_input(
                frame,
                input,
                &mut picker_ui.query,
                status_inline_label,
                input_focus_info,
            );
        }
        // When status_inline is active, skip the separate status row.
        if !picker_ui.query.config.status_inline {
            render_status(frame, status, &picker_ui.results, ui.area().width);
        }
        render_results(
            frame,
            results,
            picker_ui,
            click,
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
        let (separator_rect, content_rect) = if has_footer_separator && footer.height > 1 {
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
            let preview_junction_x =
                if state.preview_fullscreen || preview_ui.as_ref().is_none_or(|p| !p.visible()) {
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
                footer_junctions
                    .push(parent_peek_rect.x + parent_peek_rect.width.saturating_sub(1));
            }
            if let Some(jx) = preview_junction_x {
                footer_junctions.push(jx);
            }

            let preview_border = preview_ui.as_ref().map(|p| p.border());

            let preview_border_color = preview_ui.as_ref().and_then(|p| p.effective_border_color());

            let preview_border_type = preview_border.and_then(|b| b.r#type);

            let sep_fg = footer_ui
                .config
                .separator_style
                .fg
                .or(preview_border_color)
                .unwrap_or(Color::DarkGray);

            let sep_style = Style::from(footer_ui.config.separator_style).fg(sep_fg);

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
                let zoom_pct = preview_ui.as_ref().map(|p| (p.zoom * 100.0).round() as u32);
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
                render_display(frame, content_rect, footer_ui, &picker_ui.results);
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
                let show_sel = sel_raw > 0 && sel_raw != yank_raw && sel_raw != cut_raw;

                // Collect the groups to render (cut first = highest priority).
                let rcfg = &picker_ui.results.config;
                let groups: Vec<(usize, Color)> = [
                    show_cut.then(|| (cut_raw, rcfg.cut_prefix_style.fg.unwrap_or(Color::Red))),
                    show_yank
                        .then(|| (yank_raw, rcfg.yank_prefix_style.fg.unwrap_or(Color::Yellow))),
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
}
