//! Frame drawing helpers called from the render loop.

use cba::bait::ResultExt;
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;

use crate::action::{Action, ActionExt};
use crate::config::{CursorSetting, RowConnectionStyle};
use crate::event::BindSender;
use crate::message::{BindDirective, RenderCommand};
use crate::ui::{DisplayUI, PickerUI, PreviewUI, QueryUI, ResultsUI, UI};
use crate::{SSS, Selection};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

// ------------------------- HELPERS ----------------------------

pub(super) struct FocusInfo {
    pub(super) focused: bool,
    pub(super) blink_phase: bool,
    pub(super) color: Color,
    pub(super) do_blink: bool,
    pub(super) bold: bool,
    pub(super) bar: Option<ratatui::widgets::BorderType>,
    pub(super) marker: String,
    pub(super) nav_char: String,
    pub(super) nav_prompt: String,
}

impl FocusInfo {
    pub(super) fn indicator_color(&self) -> Option<Color> {
        if self.focused {
            Some(if self.do_blink && !self.blink_phase {
                Color::DarkGray
            } else {
                self.color
            })
        } else {
            None
        }
    }
}

pub enum Click {
    None,
    ResultPos(u16),
    ResultIdx(u32),
    Semantic(String),
}

impl Click {
    pub(super) fn process<A: ActionExt>(
        &mut self,
        buffer: &mut Vec<RenderCommand<A>>,
        bind_tx: &BindSender<A>,
    ) {
        match self {
            Self::ResultIdx(u) => {
                buffer.push(RenderCommand::Action(Action::Pos(*u as i32)));
            }
            Self::Semantic(s) => {
                bind_tx
                    .send(BindDirective::Action(Action::Semantic(s.clone())))
                    ._elog();
                log::debug!("Click triggered: @{s}");
            }
            _ => {}
        }
        *self = Click::None
    }
}

pub(super) fn find_interaction(
    setting: &crate::config::InteractionRegionSetting,
    x: u16,
) -> Option<String> {
    setting
        .iter()
        .rev()
        .find(|(start, _)| x >= *start as u16)
        .map(|(_, action)| action.clone())
        .filter(|a| !a.is_empty())
}

pub(super) fn render_preview(frame: &mut Frame, area: Rect, ui: &mut PreviewUI) {
    assert!(ui.visible()); // don't call if not visible.
    ui.update_dimensions(&area);

    let has_markdown = ui.has_markdown();
    let has_diagram = ui.has_diagram();
    let is_diagram_mode = has_diagram && ui.show_diagram;

    let has_top_border = ui
        .active_border()
        .map(|b| b.sides().contains(ratatui::widgets::Borders::TOP))
        .unwrap_or(false)
        && !ui.is_fullscreen();

    let render_badge_if_needed = |frame: &mut Frame, area: Rect, ui: &PreviewUI| {
        if is_diagram_mode
            && !has_top_border
            && let Some(counter) = ui.diagram_counter_spans()
        {
            let width = counter.iter().map(|s| s.width()).sum::<usize>() as u16;
            let scrollbar_pad = if ui.config.scrollbar { 1 } else { 0 };
            if area.width >= width + scrollbar_pad && area.height > 0 {
                let badge_area = Rect::new(
                    area.right().saturating_sub(width + scrollbar_pad),
                    area.y,
                    width,
                    1,
                );
                frame.render_widget(
                    ratatui::widgets::Paragraph::new(ratatui::text::Line::from(counter)),
                    badge_area,
                );
            }
        }
    };

    // Zero-stutter, state-of-the-art Kitty Unicode Placeholder pipeline for modal diagram view.
    // In Kitty and Ghostty, avoids software cropping, software resizing, and multi-megabyte PTY retransmissions on pan.
    if is_diagram_mode && crate::utils::mermaid::is_kitty_supported() {
        let block = if ui.is_fullscreen() {
            None
        } else {
            ui.make_block()
        };
        let inner_area = if let Some(b) = &block {
            b.inner(area)
        } else {
            area
        };
        if let Some(b) = block {
            frame.render_widget(b, area);
        }

        if let Some(lines) = ui.get_diagram_placeholder_lines(inner_area) {
            frame.render_widget(ratatui::widgets::Paragraph::new(lines), inner_area);
            render_badge_if_needed(frame, area, ui);
            ui.render_scrollbar(frame, area);
            ui.was_image = true;
            return;
        }
    }

    let image_state_ready = ui.get_image_state().is_some();
    let is_image = if has_markdown {
        is_diagram_mode && image_state_ready
    } else {
        image_state_ready
    };
    if is_image {
        ui.was_image = true;
        let block = if ui.is_fullscreen() {
            None
        } else {
            ui.make_block()
        };
        let inner_area = if let Some(b) = &block {
            b.inner(area)
        } else {
            area
        };
        if let Some(b) = block {
            frame.render_widget(b, area);
        }

        let media_fit_str = ui.config.media.fit.as_deref().unwrap_or("fit");
        let resize_mode = match media_fit_str.to_lowercase().as_str() {
            "crop" | "cover" => ratatui_image::Resize::Crop(None),
            "scale" | "stretch" | "fit" | "contain" => ratatui_image::Resize::Scale(None),
            _ => ratatui_image::Resize::Fit(None),
        };

        if let Some(state) = ui.get_image_state() {
            let image_widget = ratatui_image::StatefulImage::new().resize(resize_mode);
            frame.render_stateful_widget(image_widget, inner_area, state);
        }
        render_badge_if_needed(frame, area, ui);
    } else {
        if ui.was_image {
            ui.was_image = false;
            if crate::utils::mermaid::is_kitty_supported() {
                crate::utils::mermaid::delete_kitty_all();
            }
        }
        let widget = ui.make_preview();
        frame.render_widget(widget, area);
        render_badge_if_needed(frame, area, ui);
    }

    ui.render_scrollbar(frame, area);
}

pub(super) fn render_results<T: SSS, S: Selection>(
    frame: &mut Frame,
    mut area: Rect,
    ui: &mut PickerUI<T, S>,
    click: &mut Click,
    focus_info: Option<FocusInfo>,
    freeze_snapshot: bool,
) {
    let nav_bar_style = focus_info.as_ref().and_then(|fi| {
        let bar_color = fi.indicator_color();
        if let (Some(border_type), Some(color)) = (fi.bar, bar_color) {
            let mut style = Style::default().fg(color);
            if fi.bold {
                style = style.add_modifier(Modifier::BOLD);
            }
            Some((border_type, style))
        } else {
            None
        }
    });

    let cap = matches!(ui.results.config.row_connection, RowConnectionStyle::Capped);
    let (widget, table_width) = ui.make_table(click, nav_bar_style, freeze_snapshot);

    if cap {
        area.width = area.width.min(table_width);
    }

    frame.render_widget(widget, area);

    if let Some(fi) = focus_info {
        let bar_color = fi.indicator_color();

        if !fi.marker.is_empty()
            && let Some(cursor_row) = ui.results.cursor_offset()
        {
            let marker_y = if ui.results.reverse() {
                area.y + area.height.saturating_sub(1 + cursor_row)
            } else {
                area.y + cursor_row
            };
            if marker_y >= area.y && marker_y < area.y + area.height {
                let marker_color = bar_color.unwrap_or(fi.color);
                let marker_w = unicode_width::UnicodeWidthStr::width(fi.marker.as_str()) as u16;
                let marker_rect = Rect {
                    x: area.x,
                    y: marker_y,
                    width: marker_w.min(area.width),
                    height: 1,
                };
                let span = Span::styled(fi.marker.clone(), Style::default().fg(marker_color));
                frame.render_widget(Paragraph::new(span), marker_rect);
            }
        }
    }
}

/// Returns the offset of the cursor against the drawing area
pub(super) fn render_input(
    frame: &mut Frame,
    area: Rect,
    ui: &mut QueryUI,
    status: Option<Line<'_>>,
    focus_info: Option<FocusInfo>,
) -> Position {
    ui.scroll_to_cursor();
    let focused = focus_info.as_ref().is_none_or(|f| f.focused);
    let nav_prompt = focus_info.as_ref().map(|f| f.nav_prompt.as_str());

    let active_prompt = ui.active_prompt(focused, nav_prompt);
    let widget = if let Some(label) = status {
        ui.make_input_with_status_focused(label, area.width, focused, nav_prompt)
    } else {
        ui.make_input_focused(area.width, focused, nav_prompt)
    };
    let p = ui.cursor_offset_for_prompt(&area, &active_prompt);

    let show_cursor = focused;
    if let CursorSetting::Default = ui.config.cursor
        && show_cursor
    {
        frame.set_cursor_position(p)
    };

    frame.render_widget(widget, area);

    if let Some(fi) = focus_info
        && let (Some(border_type), Some(color)) = (fi.bar, fi.indicator_color())
    {
        let mut style = Style::default().fg(color);
        if fi.bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        if border_type == ratatui::widgets::BorderType::Thick {
            let indicator_rect = Rect {
                x: area.x,
                y: area.y,
                width: 1,
                height: area.height,
            };
            let span = Span::styled(fi.nav_char.clone(), style);
            frame.render_widget(Paragraph::new(span), indicator_rect);
        } else {
            let indicator = Block::default()
                .borders(Borders::LEFT)
                .border_type(border_type)
                .border_style(style);
            frame.render_widget(indicator, area);
        }
    }

    p
}

pub(super) fn render_status(frame: &mut Frame, area: Rect, ui: &ResultsUI, full_width: u16) {
    if ui.status_config.show {
        let widget = ui.make_status(full_width);
        frame.render_widget(widget, area);
    }
}

pub(super) fn render_display(
    frame: &mut Frame,
    area: Rect,
    ui: &mut DisplayUI,
    results_ui: &ResultsUI,
) {
    if !ui.show {
        return;
    }
    let widths = results_ui.widths().to_vec();

    let widget = ui.make_display(
        results_ui.indentation() as u16 + results_ui.config.border.left(),
        widths,
        results_ui.config.column_spacing.0,
    );

    frame.render_widget(widget, area);

    if ui.is_single_column() {
        let widget = ui.make_full_width_row(results_ui.indentation() as u16);
        frame.render_widget(widget, area);
    }
}

pub(super) const SORT_MENU_ITEMS: &[(&str, &str, ratatui::style::Color)] = &[
    ("[a]", "Alpha", ratatui::style::Color::Cyan),
    ("[A]", "Alpha (rev)", ratatui::style::Color::Cyan),
    ("[n]", "Natural", ratatui::style::Color::Green),
    ("[N]", "Natural (rev)", ratatui::style::Color::Green),
    ("[m]", "Mtime", ratatui::style::Color::Yellow),
    ("[M]", "Mtime (rev)", ratatui::style::Color::Yellow),
    ("[b]", "Btime", ratatui::style::Color::LightGreen),
    ("[B]", "Btime (rev)", ratatui::style::Color::LightGreen),
    ("[s]", "Size", ratatui::style::Color::Magenta),
    ("[S]", "Size (rev)", ratatui::style::Color::Magenta),
    ("[e]", "Ext", ratatui::style::Color::Blue),
    ("[E]", "Ext (rev)", ratatui::style::Color::Blue),
    ("[Esc]", "Cancel", ratatui::style::Color::DarkGray),
];

pub(super) fn render_sort_menu(frame: &mut Frame, area: Rect, cfg: &crate::config::SortMenuConfig) {
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Paragraph;

    if cfg.columns <= 1 {
        let mut spans = Vec::new();
        let mut total_w = 0;
        if cfg.show_title {
            let title_span = Span::styled(
                " Sort: ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            );
            total_w += title_span.width();
            spans.push(title_span);
        }
        let max_w = area.width as usize;

        for (key, label, color) in SORT_MENU_ITEMS {
            let key_span = Span::styled(
                key.to_string(),
                Style::default().fg(*color).add_modifier(Modifier::BOLD),
            );
            let label_span = Span::styled(format!(" {label} "), Style::default().fg(Color::White));
            let pair_w = key_span.width() + label_span.width();
            if total_w + pair_w > max_w {
                break;
            }
            total_w += pair_w;
            spans.push(key_span);
            spans.push(label_span);
        }

        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    } else {
        let cols = cfg.columns;
        let col_w = (area.width as usize) / cols;
        if col_w == 0 {
            return;
        }

        let num_rows = SORT_MENU_ITEMS.len().div_ceil(cols);
        let mut lines = Vec::with_capacity(num_rows);

        for row in 0..num_rows {
            let mut line_spans = Vec::new();
            for col in 0..cols {
                let idx = row * cols + col;
                if idx < SORT_MENU_ITEMS.len() {
                    let (key, label, color) = SORT_MENU_ITEMS[idx];
                    let key_span = Span::styled(
                        format!(" {key}"),
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    );
                    let label_span =
                        Span::styled(format!(" {label}"), Style::default().fg(Color::White));
                    let used_w = key_span.width() + label_span.width();
                    line_spans.push(key_span);
                    line_spans.push(label_span);
                    if col + 1 < cols && used_w < col_w {
                        line_spans.push(Span::raw(" ".repeat(col_w - used_w)));
                    }
                }
            }
            lines.push(Line::from(line_spans));
        }

        frame.render_widget(Paragraph::new(lines), area);
    }
}

pub const BASIC_NAV_HINTS: &[(&str, &str, ratatui::style::Color)] = &[
    ("[/]", "Filter", ratatui::style::Color::Cyan),
    ("[j/k]", "Move", ratatui::style::Color::Yellow),
    ("[J/K]", "Scroll", ratatui::style::Color::Blue),
    ("[gg/G]", "Top/End", ratatui::style::Color::Yellow),
    ("[C-p]", "Preview", ratatui::style::Color::Blue),
    ("[,]", "Sort", ratatui::style::Color::Yellow),
    ("[\\]", "Pane", ratatui::style::Color::Cyan),
    ("[q/Esc]", "Back", ratatui::style::Color::Red),
];

pub const LIST_NAV_HINTS: &[(&str, &str, ratatui::style::Color)] = &[
    ("[/]", "Filter", ratatui::style::Color::Cyan),
    ("[j/k]", "Move", ratatui::style::Color::Yellow),
    ("[Space]", "Select", ratatui::style::Color::Yellow),
    ("[,]", "Sort", ratatui::style::Color::Yellow),
    ("[J/K]", "Scroll", ratatui::style::Color::Blue),
    ("[C-p]", "Preview", ratatui::style::Color::Blue),
    ("[\\]", "Pane", ratatui::style::Color::Cyan),
    ("[q/Esc]", "Back", ratatui::style::Color::Red),
];

pub const NAV_HINTS: &[(&str, &str, ratatui::style::Color)] = &[
    ("[/]", "Filter", ratatui::style::Color::Cyan),
    ("[Space]", "Select", ratatui::style::Color::Yellow),
    ("[,]", "Sort", ratatui::style::Color::Yellow),
    ("[f]", "Frecency", ratatui::style::Color::Cyan),
    ("[b]", "Bookmarks", ratatui::style::Color::Magenta),
    ("[*]", "Bookmark", ratatui::style::Color::Yellow),
    ("[e]", "Edit", ratatui::style::Color::Green),
    ("[a]", "Add", ratatui::style::Color::Green),
    ("[r]", "Rename", ratatui::style::Color::Yellow),
    ("[d]", "Trash", ratatui::style::Color::Red),
    ("[y/x]", "Copy/Cut", ratatui::style::Color::Green),
    ("[p/P]", "Paste", ratatui::style::Color::Magenta),
    ("[u]", "Undo", ratatui::style::Color::Blue),
    ("[z/Z]", "Zip", ratatui::style::Color::Blue),
    ("[\\]", "Pane", ratatui::style::Color::Cyan),
    ("[C-p]", "Preview", ratatui::style::Color::Blue),
];

pub const PREVIEW_NAV_HINTS: &[(&str, &str, ratatui::style::Color)] = &[
    ("[hjkl]", "Pan", ratatui::style::Color::Yellow),
    ("[J/K]", "Jump15", ratatui::style::Color::Yellow),
    ("[C-u/u]", "HalfUp", ratatui::style::Color::Yellow),
    ("[C-d/d]", "HalfDn", ratatui::style::Color::Yellow),
    ("[s]", "Diagram", ratatui::style::Color::Magenta),
    ("[n/N]", "DiagIdx", ratatui::style::Color::Cyan),
    ("[+/-]", "Zoom", ratatui::style::Color::Green),
    ("[0/z]", "Reset", ratatui::style::Color::Blue),
    ("[esc/enter]", "Back", ratatui::style::Color::Red),
];

pub const FILTER_NAV_HINTS: &[(&str, &str, ratatui::style::Color)] = &[
    ("[Esc]", "Nav", ratatui::style::Color::Red),
    ("[Tab]", "Cycle", ratatui::style::Color::Magenta),
    ("[C-j/k]", "Move", ratatui::style::Color::Yellow),
    ("[C-h/l]", "Dir", ratatui::style::Color::Cyan),
    ("[C-u]", "Parents", ratatui::style::Color::Blue),
    ("[C-d]", "Delete", ratatui::style::Color::Red),
    ("[C-v]", "Insert", ratatui::style::Color::Green),
    ("[C-e]", "Edit", ratatui::style::Color::Green),
    ("[C-y]", "Copy", ratatui::style::Color::Magenta),
    ("[C-p]", "Preview", ratatui::style::Color::Blue),
    ("[Enter]", "Accept", ratatui::style::Color::Cyan),
    ("[C-c]", "Quit", ratatui::style::Color::Red),
];

pub(super) fn render_nav_hints(
    frame: &mut Frame,
    area: Rect,
    profile: crate::config::NavProfile,
    columns: usize,
    preview_fullscreen: bool,
    zoom_pct: Option<u32>,
    is_filter_mode: bool,
) {
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Paragraph;

    let hints: &[(&str, &str, Color)] = if preview_fullscreen {
        PREVIEW_NAV_HINTS
    } else if is_filter_mode {
        FILTER_NAV_HINTS
    } else {
        match profile {
            crate::config::NavProfile::Fm => NAV_HINTS,
            crate::config::NavProfile::List => LIST_NAV_HINTS,
            crate::config::NavProfile::Basic => BASIC_NAV_HINTS,
            crate::config::NavProfile::None => &[],
        }
    };

    if hints.is_empty() {
        return;
    }

    let cols = if columns == 0 { 4 } else { columns };

    if cols <= 1 {
        let mut spans = Vec::new();
        let mut total_w = 0;
        let max_w = area.width as usize;

        for (key, label, color) in hints {
            let label_owned = if preview_fullscreen
                && *key == "[+/-]"
                && let Some(pct) = zoom_pct
            {
                format!("Zoom {pct}%")
            } else {
                label.to_string()
            };
            let key_span = Span::styled(
                format!(" {key}"),
                Style::default().fg(*color).add_modifier(Modifier::BOLD),
            );
            let label_span = Span::styled(
                format!(" {label_owned} "),
                Style::default().fg(Color::DarkGray),
            );
            let pair_w = key_span.width() + label_span.width();
            if total_w + pair_w > max_w {
                break;
            }
            total_w += pair_w;
            spans.push(key_span);
            spans.push(label_span);
        }

        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    } else {
        let col_w = (area.width as usize) / cols;
        if col_w == 0 {
            return;
        }

        let num_rows = hints.len().div_ceil(cols);
        let mut lines = Vec::with_capacity(num_rows);

        for row in 0..num_rows {
            let mut line_spans = Vec::new();
            for col in 0..cols {
                let idx = row * cols + col;
                if idx < hints.len() {
                    let (key, label, color) = hints[idx];
                    let label_owned = if preview_fullscreen
                        && key == "[+/-]"
                        && let Some(pct) = zoom_pct
                    {
                        format!("Zoom {pct}%")
                    } else {
                        label.to_string()
                    };
                    let key_span = Span::styled(
                        format!(" {key}"),
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    );
                    let label_span =
                        Span::styled(format!(" {label_owned}"), Style::default().fg(Color::White));
                    let used_w = key_span.width() + label_span.width();
                    line_spans.push(key_span);
                    line_spans.push(label_span);
                    if col + 1 < cols && used_w < col_w {
                        line_spans.push(Span::raw(" ".repeat(col_w - used_w)));
                    }
                }
            }
            lines.push(Line::from(line_spans));
        }

        frame.render_widget(Paragraph::new(lines), area);
    }
}

pub(super) fn render_footer_separator(
    frame: &mut Frame,
    area: Rect,
    separator: crate::config::HorizontalSeparator,
    junctions: &[u16],
    preview_border_type: Option<BorderType>,
    style: Style,
) {
    if area.height == 0 || area.width == 0 || separator == crate::config::HorizontalSeparator::None
    {
        return;
    }

    let line_char = match separator {
        crate::config::HorizontalSeparator::None => return,
        crate::config::HorizontalSeparator::Empty => ' ',
        crate::config::HorizontalSeparator::Light
        | crate::config::HorizontalSeparator::Normal
        | crate::config::HorizontalSeparator::Underline => '─',
        crate::config::HorizontalSeparator::Heavy => '━',
        crate::config::HorizontalSeparator::Dashed => '╌',
        crate::config::HorizontalSeparator::Top => '▔',
        crate::config::HorizontalSeparator::Bottom => ' ',
    };

    let junction_char = match separator {
        crate::config::HorizontalSeparator::None => return,
        crate::config::HorizontalSeparator::Empty => ' ',
        crate::config::HorizontalSeparator::Heavy => match preview_border_type {
            Some(BorderType::Thick) => '┻',
            Some(BorderType::Double) => '╩',
            _ => '┷',
        },
        crate::config::HorizontalSeparator::Light
        | crate::config::HorizontalSeparator::Normal
        | crate::config::HorizontalSeparator::Dashed
        | crate::config::HorizontalSeparator::Top
        | crate::config::HorizontalSeparator::Bottom
        | crate::config::HorizontalSeparator::Underline => match preview_border_type {
            Some(BorderType::Thick) => '┸',
            Some(BorderType::Double) => '╨',
            _ => '┴',
        },
    };

    let mut line_chars = Vec::with_capacity(area.width as usize);

    for col in 0..area.width {
        let x = area.x + col;
        if junctions.contains(&x) {
            line_chars.push(junction_char);
        } else {
            line_chars.push(line_char);
        }
    }

    let line_str: String = line_chars.into_iter().collect();
    let span = Span::styled(line_str, style);
    frame.render_widget(Paragraph::new(Line::from(span)), area);
}

pub(super) fn render_parent_peek(
    frame: &mut Frame,
    area: Rect,
    cfg: &crate::config::ParentPeekConfig,
    cache: Option<&crate::render::state::ParentPeekCache>,
) {
    if area.height <= 2 || area.width <= 2 {
        return;
    }

    let Some(cache) = cache else {
        return;
    };

    let parent_name = &cache.parent_name;
    let current_name = cache.current_name.as_deref();

    let parent_color = cfg.parent_color.unwrap_or(Color::Cyan);

    let inner = if cfg.border.show {
        let border_color = cfg.border.color.unwrap_or(Color::DarkGray);
        let border_type = cfg
            .border
            .r#type
            .unwrap_or(ratatui::widgets::BorderType::Plain);

        let block = Block::default()
            .borders(Borders::RIGHT)
            .border_type(border_type)
            .border_style(Style::default().fg(border_color))
            .title(Span::styled(
                format!(" {} ", parent_name),
                Style::default()
                    .fg(parent_color)
                    .add_modifier(Modifier::BOLD),
            ));

        let inner_area = block.inner(area);
        frame.render_widget(block, area);
        inner_area
    } else {
        area
    };

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let entries = &cache.entries;

    let mut selected_idx = 0;
    if let Some(cur) = current_name
        && let Some(pos) = entries.iter().position(|e| e.0 == cur)
    {
        selected_idx = pos;
    }

    let visible_rows = inner.height as usize;
    let half_visible = visible_rows / 2;
    let start_idx = selected_idx.saturating_sub(half_visible);
    let end_idx = (start_idx + visible_rows).min(entries.len());

    let highlight_color = cfg.highlight_color.unwrap_or(Color::Yellow);

    let mut lines = Vec::new();
    for (idx, (name, is_dir)) in entries[start_idx..end_idx].iter().enumerate() {
        let actual_idx = start_idx + idx;
        let is_selected = (actual_idx == selected_idx) && cfg.highlight;

        let icon = if *is_dir { " " } else { " " };
        let text = format!("{}{}", icon, name);

        let style = if is_selected {
            Style::default()
                .fg(highlight_color)
                .add_modifier(Modifier::BOLD)
        } else if *is_dir {
            Style::default().fg(Color::Blue)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        lines.push(Line::from(Span::styled(text, style)));
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

// a bit weird, do we want mutable, do we want &mut ui, whatever this is simplest
pub(super) fn render_ui(frame: &mut Frame, area: &mut Rect, ui: &UI) {
    if ui.config.border.sides.is_some() {
        let widget = ui.make_ui();
        frame.render_widget(widget, *area);
        if let Some(stops) = crate::ui::gradient::parse_stops(&ui.config.border.gradient) {
            crate::ui::gradient::paint_frame(frame.buffer_mut(), *area, &stops);
        }
    }
    *area = ui.compute_area(area);
}

pub(super) fn split(rect: &mut Rect, height: u16, cut_top: bool) -> Rect {
    let h = height.min(rect.height);

    if cut_top {
        let offshoot = Rect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: h,
        };

        rect.y += h;
        rect.height -= h;

        offshoot
    } else {
        let offshoot = Rect {
            x: rect.x,
            y: rect.y + rect.height - h,
            width: rect.width,
            height: h,
        };

        rect.height -= h;

        offshoot
    }
}

// -----------------------------------------------------------------------------------
