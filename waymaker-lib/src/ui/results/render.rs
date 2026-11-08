use ratatui::{
    layout::Alignment,
    style::Style,
    text::Span,
    widgets::{Row, Table},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    SSS, Selection, Selector,
    config::{HorizontalSeparator, RowConnectionStyle},
    nucleo::Worker,
    render::Click,
    ui::results::{
        ResultsUI,
        icons::{
            apply_bookmark_text_style, bookmark_color, extract_col0_name, insert_icon_span,
            maybe_append_symlink_target,
        },
        styler::{RowCtx, RowStyler, RowsFrame, RowsIter},
    },
    utils::{
        string::{fit_width, substitute_escaped},
        text::{clip_text_lines, prefix_span},
    },
};

impl ResultsUI {
    /// Moves the cursor to `results.pos` once enough items have arrived.
    fn apply_initial_pos<T: SSS>(&mut self, worker: &mut Worker<T>) {
        if let Some(pos) = self.initial_pos {
            let (_, status) = Worker::new_snapshot(&mut worker.nucleo);
            self.status = status;
            let total = self.status.matched_count;
            if total > 0 {
                let target = if pos >= 0 {
                    (pos as u32).min(total.saturating_sub(1))
                } else {
                    total.saturating_sub((-pos) as u32)
                };
                self.cursor_jump(target);
                if pos >= 0 {
                    if total > pos as u32 || !self.status.running {
                        self.initial_pos = None;
                    }
                } else if total >= (-pos) as u32 || !self.status.running {
                    self.initial_pos = None;
                }
            } else if !self.status.running {
                self.initial_pos = None;
            }
        }
    }

    /// Widens the first visible column for indentation, group headers and symlink targets.
    fn adjust_widths<T: SSS>(
        &self,
        widths: &mut [u16],
        results: &crate::nucleo::WorkerResults<'_, T>,
    ) {
        if self.indentation() > 0 {
            if let Some(first_vis) = widths.iter().position(|&w| w != 0) {
                widths[first_vis] += self.indentation() as u16;
            } else if !widths.is_empty() {
                widths[0] += self.indentation() as u16;
            }
        }

        // Ensure visible column is at least wide enough for any group headers in results
        let max_group_width = results
            .iter()
            .filter_map(|(g, _, _)| match g.as_ref() {
                Some(crate::nucleo::GroupHeader::Named(s)) => {
                    Some(crate::utils::cells(UnicodeWidthStr::width(&**s)).saturating_add(2))
                }
                _ => None,
            })
            .max()
            .unwrap_or(0);
        if max_group_width > 0
            && let Some(first_vis) = widths.iter().position(|&w| w != 0)
        {
            widths[first_vis] = widths[first_vis].max(max_group_width);
        }

        // When symlink targets are enabled, expand column 0 to use all
        // remaining horizontal space so the annotation has room to display.
        if self.config.symlink.active && !widths.is_empty() && widths[0] != 0 {
            let other_cols: u16 = widths[1..].iter().sum();
            let col0_max = self
                .width
                .saturating_sub(other_cols)
                .saturating_sub(self.column_spacing_width());
            widths[0] = widths[0].max(col0_max);
        }
    }

    /// The row for a group header or tier separator; `None` for an underline or no separator.
    fn group_header_row(
        &self,
        group: &crate::nucleo::GroupHeader,
        nav_bar_span: Option<Span<'static>>,
        active_group_name: &Option<std::sync::Arc<str>>,
        as_cols: bool,
        widths: &[u16],
    ) -> Option<Row<'static>> {
        match group {
            crate::nucleo::GroupHeader::Named(group_name) => {
                let is_active =
                    active_group_name.as_ref().map(|s| s.as_ref()) == Some(group_name.as_ref());
                let group_style: Style = if is_active {
                    self.config.group_header_style.into()
                } else {
                    self.config.inactive_group_header_style.into()
                };
                let mut line_spans = vec![];
                if let Some(nav_span) = nav_bar_span {
                    line_spans.push(nav_span);
                }
                line_spans.push(Span::raw(" "));
                line_spans.push(Span::styled(group_name.to_string(), group_style));

                let line = ratatui::text::Line::from(line_spans);
                if as_cols {
                    let first_visible = widths.iter().position(|&w| w != 0).unwrap_or(0);
                    let mut cells = vec![];
                    for i in 0..widths.len() {
                        if i == first_visible {
                            cells.push(ratatui::widgets::Cell::from(line.clone()));
                        } else {
                            cells.push(ratatui::widgets::Cell::from(""));
                        }
                    }
                    Some(Row::new(cells).height(1))
                } else {
                    Some(Row::new(vec![line]).height(1))
                }
            }
            crate::nucleo::GroupHeader::TierSeparator => {
                let tier_sep = self.config.tier.separator;
                if matches!(
                    tier_sep,
                    HorizontalSeparator::None | HorizontalSeparator::Underline
                ) {
                    None
                } else {
                    let sep_char = tier_sep.as_str();
                    let tier_style: Style = self.config.tier.style.into();
                    let total_w = self.width as usize;

                    if as_cols {
                        let mut cells = vec![];
                        let first_visible = widths.iter().position(|&w| w != 0).unwrap_or(0);
                        for (col_i, &w) in widths.iter().enumerate() {
                            if w == 0 {
                                cells.push(ratatui::widgets::Cell::from(""));
                                continue;
                            }
                            let mut spans = vec![];
                            let mut fill_w = w as usize;
                            if col_i == first_visible
                                && let Some(nav_span) = nav_bar_span.clone()
                            {
                                fill_w = fill_w.saturating_sub(nav_span.width());
                                spans.push(nav_span);
                            }
                            let line_str = sep_char.repeat(fill_w);
                            spans.push(Span::styled(line_str, tier_style));
                            cells.push(ratatui::widgets::Cell::from(ratatui::text::Line::from(
                                spans,
                            )));
                        }
                        Some(Row::new(cells).height(1))
                    } else {
                        let mut spans = vec![];
                        let mut fill_w = total_w;
                        if let Some(nav_span) = nav_bar_span {
                            fill_w = fill_w.saturating_sub(nav_span.width());
                            spans.push(nav_span);
                        }
                        let line_str = sep_char.repeat(fill_w);
                        spans.push(Span::styled(line_str, tier_style));
                        Some(Row::new(vec![ratatui::text::Line::from(spans)]).height(1))
                    }
                }
            }
        }
    }

    /// The prefix column of a row (spinner frame, multi-select marker or the default prefix),
    /// plus what it found: the item's name, spinner state and yank/cut marks. Animates or strips
    /// a spinner marker in `row` on the way.
    #[allow(clippy::too_many_arguments)]
    fn row_prefix<T: SSS>(
        &self,
        row: &mut [ratatui::text::Text<'_>],
        is_selected: bool,
        idx: usize,
        item: &T,
        columns: &[crate::nucleo::Column<T>],
        is_first: bool,
        is_last: bool,
        cwd: &std::path::Path,
        get_dynamic_multi_prefix: &impl Fn(bool, bool) -> String,
    ) -> (String, String, bool, usize, bool, bool) {
        let mut icon_name = String::new();
        let mut is_spinner = false;
        let mut spinner_col_idx = 0;
        // The marker found in the row and the spinner it animates.
        let mut spinner_marker: &str = &self.config.spinner_prefix;
        let mut spinner_name: &str = &self.config.spinner;

        if !row.is_empty() {
            let markers = [
                (&self.config.spinner_prefix, &self.config.spinner),
                (&self.config.slow_spinner_prefix, &self.config.slow_spinner),
            ];
            'find: for (i, col_text) in row.iter().enumerate() {
                let text_content = extract_col0_name(col_text);
                for (marker, name) in markers {
                    if !marker.is_empty() && text_content.contains(marker.as_str()) {
                        is_spinner = true;
                        spinner_col_idx = i;
                        spinner_marker = marker;
                        spinner_name = name;
                        break 'find;
                    }
                }
            }
        }

        if !row.is_empty() {
            icon_name = columns[0].raw(item).into_owned();
            if is_spinner && spinner_col_idx == 0 {
                icon_name = icon_name.replace(spinner_marker, "");
            }
        }

        if is_spinner {
            if self.config.spinner_inline {
                let frame = crate::spinner::Spinner::from_name(spinner_name).current_frame();
                crate::utils::text::replace_string_in_text(
                    &mut row[spinner_col_idx],
                    spinner_marker,
                    frame,
                );
            } else {
                crate::utils::text::strip_string_from_text(
                    &mut row[spinner_col_idx],
                    spinner_marker,
                );
            }
        }
        let is_yanked = Self::is_path_in_set(&self.yank_paths, &icon_name, cwd);
        let is_cut = Self::is_path_in_set(&self.cut_paths, &icon_name, cwd);

        let prefix = if is_spinner && !self.config.spinner_inline {
            let frame = crate::spinner::Spinner::from_name(spinner_name).current_frame();
            let f = format!("{frame} ");
            crate::utils::string::fit_width(&f, self.config.multi_prefix.width())
        } else if is_selected || is_yanked || is_cut {
            get_dynamic_multi_prefix(is_first, is_last)
        } else {
            self.default_prefix(idx)
        };
        (
            prefix,
            icon_name,
            is_spinner,
            spinner_col_idx,
            is_yanked,
            is_cut,
        )
    }

    /// Scrolling down: moves `bottom` past the rows above the cursor that no longer fit, and
    /// draws the first one cut if it is only partly visible (setting `bottom_clip`).
    #[allow(clippy::too_many_arguments)]
    fn cut_top_row_scrolling_down<'a, T: SSS, S: Selection>(
        &mut self,
        styler: &RowStyler,
        results: &mut crate::nucleo::WorkerResults<'a, T>,
        rows: &mut Vec<Row<'a>>,
        selector: &Selector<T, S>,
        columns: &std::sync::Arc<[crate::nucleo::Column<T>]>,
        cwd: &std::path::Path,
        widths: &[u16],
        results_len: usize,
        as_cols: bool,
        h_to_cursor_end: u16,
        cursor_end_should_lte: u16,
        mut start_index: u16,
        mut total_height: u16,
    ) -> (u16, u16) {
        let get_nav_bar_span =
            |is_first: bool, is_last: bool, is_current: bool, is_yanked: bool, is_cut: bool| {
                styler.nav_bar_span(is_first, is_last, is_current, is_yanked, is_cut)
            };
        let get_dynamic_multi_prefix =
            |is_first: bool, is_last: bool| styler.dynamic_multi_prefix(is_first, is_last);
        let height_of = |t: &(
            Option<crate::nucleo::GroupHeader>,
            Vec<ratatui::text::Text<'a>>,
            &'a T,
        )| styler.height_of(&t.0, &t.1);
        let style_text = |t: ratatui::text::Text<'a>, x: usize, is_current_row: bool| {
            styler.style_text(t, x, is_current_row)
        };
        let is_current_row = false;
        let mut trunc_height = h_to_cursor_end - cursor_end_should_lte;
        // note that there is a funny side effect that scrolling up near the bottom can scroll up a bit, but it seems fine to me

        for r in results[start_index as usize..self.cursor as usize].iter_mut() {
            let h = height_of(r);
            let (_, row, item) = r;
            start_index += 1; // we always skip at least the first item

            if trunc_height < h {
                let mut remaining_height = h - trunc_height;
                let is_selected = selector.contains(item);
                let is_first = rows.is_empty();
                let is_last = (self.height <= total_height + remaining_height)
                    || (start_index as usize >= results_len);
                let (prefix, icon_name, is_spinner, spinner_col_idx, is_yanked, is_cut) = self
                    .row_prefix(
                        row,
                        is_selected,
                        0,
                        *item,
                        columns,
                        is_first,
                        is_last,
                        cwd,
                        &get_dynamic_multi_prefix,
                    );
                let nav_bar_span =
                    get_nav_bar_span(is_first, is_last, is_current_row, is_yanked, is_cut);
                let is_pinned = Self::is_path_in_set(&self.pin_paths, &icon_name, cwd);
                let b_color = if is_pinned || self.mode_index == 2 {
                    Some(bookmark_color(&self.config, &icon_name))
                } else {
                    None
                };

                total_height += remaining_height;

                // log::debug!("r: {remaining_height}");
                if as_cols {
                    if remaining_height < h - self._hr() {
                        for (_, t) in row.iter_mut().enumerate().filter(|(i, _)| widths[*i] != 0) {
                            clip_text_lines(t, remaining_height, !self.reverse());
                        }
                    }

                    let first_visible = widths.iter().position(|&w| w != 0).unwrap_or(0);
                    let target_prefix_col =
                        if is_spinner && widths.get(spinner_col_idx).copied().unwrap_or(0) != 0 {
                            spinner_col_idx
                        } else {
                            first_visible
                        };

                    let last_visible = widths
                        .iter()
                        .enumerate()
                        .rev()
                        .find_map(|(i, w)| (*w != 0).then_some(i));

                    let row_texts: Vec<_> = row
                        .iter()
                        .take(last_visible.map(|x| x + 1).unwrap_or(0))
                        .cloned()
                        .enumerate()
                        .map(|(x, mut t)| {
                            t = style_text(t, x, is_current_row);
                            if let Some(color) = b_color {
                                apply_bookmark_text_style(&mut t, color);
                            }
                            if x == target_prefix_col {
                                prefix_span(
                                    &mut t,
                                    prefix.clone(),
                                    self.active_prefix_style(
                                        &icon_name,
                                        is_selected,
                                        is_spinner,
                                        cwd,
                                    ),
                                    self.inactive_prefix_style(
                                        &icon_name,
                                        is_selected,
                                        is_spinner,
                                        cwd,
                                    ),
                                    is_current_row,
                                    if !is_selected {
                                        nav_bar_span.clone()
                                    } else {
                                        None
                                    },
                                    self.config.current_style,
                                    self.config.current_nav_bar_style,
                                );
                                self.decorate_cut_col_icon_and_symlink(
                                    &mut t,
                                    &icon_name,
                                    is_selected,
                                    is_current_row,
                                    is_pinned,
                                    is_yanked,
                                    is_cut,
                                    &nav_bar_span,
                                    cwd,
                                    widths,
                                    target_prefix_col,
                                );
                            }
                            t
                        })
                        .collect();

                    self.push_styled_cut_cols_row(row_texts, rows, remaining_height, is_selected);
                } else {
                    let col_count = row.len();
                    let mut push = vec![];

                    for (rev_i, col) in row.iter_mut().rev().enumerate() {
                        let col_idx = col_count.saturating_sub(1 + rev_i);
                        let mut height = col.height() as u16;
                        if remaining_height == 0 {
                            break;
                        } else if remaining_height < height {
                            clip_text_lines(col, remaining_height, !self.reverse());
                            height = remaining_height;
                        }
                        remaining_height -= height;

                        self.push_cut_stacked_col(
                            col,
                            &mut push,
                            col_idx,
                            height,
                            b_color,
                            &prefix,
                            &icon_name,
                            is_selected,
                            is_spinner,
                            is_current_row,
                            is_pinned,
                            is_yanked,
                            is_cut,
                            &nav_bar_span,
                            cwd,
                        );
                    }
                    rows.extend(push.into_iter().rev());
                }

                self.bottom += start_index as u32 - 1;
                self.cursor -= start_index - 1;
                self.bottom_clip = Some(remaining_height);
                break;
            } else if trunc_height == h {
                self.bottom += start_index as u32;
                self.cursor -= start_index;
                self.bottom_clip = None;
                break;
            }

            trunc_height -= h;
        }
        (start_index, total_height)
    }

    /// The first row stayed cut since the last frame (`bottom_clip`): draws its visible part.
    #[allow(clippy::too_many_arguments)]
    fn redraw_kept_top_row<'a, T: SSS, S: Selection>(
        &mut self,
        styler: &RowStyler,
        results: &mut crate::nucleo::WorkerResults<'a, T>,
        rows: &mut Vec<Row<'a>>,
        selector: &Selector<T, S>,
        columns: &std::sync::Arc<[crate::nucleo::Column<T>]>,
        cwd: &std::path::Path,
        widths: &[u16],
        results_len: usize,
        as_cols: bool,
        is_underline_mode: bool,
        mut remaining_height: u16,
        mut start_index: u16,
        mut total_height: u16,
    ) -> (u16, u16) {
        let get_nav_bar_span =
            |is_first: bool, is_last: bool, is_current: bool, is_yanked: bool, is_cut: bool| {
                styler.nav_bar_span(is_first, is_last, is_current, is_yanked, is_cut)
            };
        let get_dynamic_multi_prefix =
            |is_first: bool, is_last: bool| styler.dynamic_multi_prefix(is_first, is_last);
        let apply_tier_underline =
            |t: &mut ratatui::text::Text<'_>, target_w: usize, skip_spans: usize| {
                styler.tier_underline(t, target_w, skip_spans)
            };
        let height_of = |t: &(
            Option<crate::nucleo::GroupHeader>,
            Vec<ratatui::text::Text<'a>>,
            &'a T,
        )| styler.height_of(&t.0, &t.1);
        let style_text = |t: ratatui::text::Text<'a>, x: usize, is_current_row: bool| {
            styler.style_text(t, x, is_current_row)
        };
        let is_current_row = false;
        start_index += 1;
        let is_topside_tier_underlined = is_underline_mode
            && ((start_index as usize) < results_len
                && matches!(
                    results.get(start_index as usize),
                    Some((Some(crate::nucleo::GroupHeader::TierSeparator), _, _))
                ));
        // same as above
        let h = height_of(&results[0]);
        let (_, row, item) = &mut results[0];
        let is_selected = selector.contains(item);
        let is_first = rows.is_empty();
        let is_last = (self.height <= total_height + remaining_height)
            || (start_index as usize >= results_len);
        let (prefix, icon_name, is_spinner, spinner_col_idx, is_yanked, is_cut) = self.row_prefix(
            row,
            is_selected,
            0,
            *item,
            columns,
            is_first,
            is_last,
            cwd,
            &get_dynamic_multi_prefix,
        );
        let nav_bar_span = get_nav_bar_span(is_first, is_last, is_current_row, is_yanked, is_cut);
        let is_pinned = Self::is_path_in_set(&self.pin_paths, &icon_name, cwd);
        let b_color = if is_pinned || self.mode_index == 2 {
            Some(bookmark_color(&self.config, &icon_name))
        } else {
            None
        };

        total_height += remaining_height;

        if as_cols {
            self.push_kept_cols_top_row(
                row,
                rows,
                widths,
                h,
                remaining_height,
                is_spinner,
                spinner_col_idx,
                is_current_row,
                is_topside_tier_underlined,
                b_color,
                &prefix,
                icon_name,
                is_selected,
                is_pinned,
                is_yanked,
                is_cut,
                &nav_bar_span,
                cwd,
                &style_text,
                &apply_tier_underline,
            );
        } else {
            let col_count = row.len();
            let mut push = vec![];

            for (rev_i, col) in row.iter_mut().rev().enumerate() {
                let col_idx = col_count.saturating_sub(1 + rev_i);
                let mut height = col.height() as u16;
                if remaining_height == 0 {
                    break;
                } else if remaining_height < height {
                    clip_text_lines(col, remaining_height, !self.reverse());
                    height = remaining_height;
                }
                remaining_height -= height;

                if let Some(color) = b_color {
                    apply_bookmark_text_style(col, color);
                }

                prefix_span(
                    col,
                    prefix.clone(),
                    self.active_prefix_style(&icon_name, is_selected, is_spinner, cwd),
                    self.inactive_prefix_style(&icon_name, is_selected, is_spinner, cwd),
                    is_current_row,
                    if !is_selected {
                        nav_bar_span.clone()
                    } else {
                        None
                    },
                    self.config.current_style,
                    self.config.current_nav_bar_style,
                );
                if self.config.icons && col_idx == 0 {
                    insert_icon_span(
                        col,
                        &icon_name,
                        !is_selected && nav_bar_span.is_some(),
                        is_current_row,
                        self.config.uncolor_current_icon,
                        self.config.invert_current_icon,
                        self.config.current_icon_style,
                        is_pinned,
                        is_yanked,
                        is_cut,
                        self.mode_index,
                        &self.config,
                        self.get_flash_op(&icon_name, cwd),
                    );
                }
                if self.config.symlink.active && col_idx == 0 {
                    maybe_append_symlink_target(
                        col,
                        &icon_name,
                        self.config.symlink.style.into(),
                        self.width,
                    );
                }

                if is_topside_tier_underlined {
                    let target_w = self.width as usize;
                    let has_nav = !is_selected && nav_bar_span.is_some();
                    let prefix_skip = if has_nav { 2 } else { 1 };
                    let skip = if col_idx == 0 { prefix_skip } else { 0 };
                    apply_tier_underline(col, target_w, skip);
                }

                let row = Row::new(vec![col.clone()]).height(height);
                let row = if is_selected && !is_current_row {
                    row.style(Style::from(self.config.selected_style))
                } else {
                    row
                };
                push.push(row);
            }
            rows.extend(push.into_iter().rev());
        }
        (start_index, total_height)
    }

    /// Moves `bottom` so the cursor row stays inside the scroll padding, and draws the rows above
    /// the cursor that are cut by the move (when the first visible row is only partly shown).
    /// Returns the index in `results` of the first complete row and the height used so far.
    #[allow(clippy::too_many_arguments)]
    fn scroll_to_cursor_rows<'a, T: SSS, S: Selection>(
        &mut self,
        styler: &RowStyler,
        results: &mut crate::nucleo::WorkerResults<'a, T>,
        rows: &mut Vec<Row<'a>>,
        selector: &Selector<T, S>,
        columns: &std::sync::Arc<[crate::nucleo::Column<T>]>,
        cwd: &std::path::Path,
        widths: &[u16],
        results_len: usize,
        as_cols: bool,
        is_underline_mode: bool,
        h_at_cursor: u16,
        h_to_cursor: u16,
        cursor_end_should_lte: u16,
        mut start_index: u16,
        mut total_height: u16,
    ) -> (u16, u16) {
        if h_at_cursor >= cursor_end_should_lte {
            start_index = self.cursor;
            self.bottom += self.cursor as u32;
            self.cursor = 0;
            self.cursor_above = 0;
            self.bottom_clip = None;
        } else
        // increase the bottom index so that cursor_should_above is maintained
        if let h_to_cursor_end = h_to_cursor + h_at_cursor
            && h_to_cursor_end > cursor_end_should_lte
        {
            (start_index, total_height) = self.cut_top_row_scrolling_down(
                styler,
                results,
                rows,
                selector,
                columns,
                cwd,
                widths,
                results_len,
                as_cols,
                h_to_cursor_end,
                cursor_end_should_lte,
                start_index,
                total_height,
            );
        } else if let Some(remaining_height) = self.bottom_clip {
            (start_index, total_height) = self.redraw_kept_top_row(
                styler,
                results,
                rows,
                selector,
                columns,
                cwd,
                widths,
                results_len,
                as_cols,
                is_underline_mode,
                remaining_height,
                start_index,
                total_height,
            );
        }
        (start_index, total_height)
    }

    /// Pushes the rows from `start_index` on, with their group headers, separators, prefixes and
    /// styles, until the table is full. Returns the row counter and the height left over.
    #[allow(clippy::too_many_arguments)]
    fn push_visible_rows<'a, T: SSS, S: Selection>(
        &mut self,
        styler: &RowStyler,
        results: &mut crate::nucleo::WorkerResults<'a, T>,
        start_index: u16,
        rows: &mut Vec<Row<'a>>,
        click: &mut Click,
        selector: &Selector<T, S>,
        columns: &std::sync::Arc<[crate::nucleo::Column<T>]>,
        cwd: &std::path::Path,
        widths: &[u16],
        as_cols: bool,
        is_underline_mode: bool,
        active_group_name: &Option<std::sync::Arc<str>>,
        active_column: usize,
        mut i: usize,
        mut remaining_height: u16,
    ) -> (usize, u16) {
        let get_nav_bar_span =
            |is_first: bool, is_last: bool, is_current: bool, is_yanked: bool, is_cut: bool| {
                styler.nav_bar_span(is_first, is_last, is_current, is_yanked, is_cut)
            };
        let get_dynamic_multi_prefix =
            |is_first: bool, is_last: bool| styler.dynamic_multi_prefix(is_first, is_last);
        let frame = &RowsFrame {
            styler,
            widths,
            cwd,
            is_underline_mode,
            active_column,
        };

        let mut drain_iter = results.drain(start_index as usize..).peekable();
        while let Some((group, mut row, item)) = drain_iter.next() {
            // note that the index changes *next* frame
            if let Click::ResultPos(c) = click {
                let c = if self.reverse() {
                    self.height.saturating_sub(*c).saturating_sub(1)
                } else {
                    *c
                };
                if self.height - remaining_height > c {
                    let idx = self.bottom + i as u32 - 1;
                    log::debug!(
                        "Mapped click position to index: {c} -> {idx} with remaining {remaining_height}",
                    );
                    *click = Click::ResultIdx(idx);
                }
            }

            // insert group header or tier separator
            if let Some(ref group) = group
                && remaining_height > 0
            {
                let is_first = rows.is_empty();
                let is_last = remaining_height <= 1;
                let nav_bar_span = get_nav_bar_span(is_first, is_last, false, false, false);

                let row_opt =
                    self.group_header_row(group, nav_bar_span, active_group_name, as_cols, widths);

                if let Some(row) = row_opt {
                    rows.push(row);
                    remaining_height = remaining_height.saturating_sub(1);
                }
            }
            if remaining_height == 0 {
                break;
            }

            // insert hr
            if let Some(hr) = self.hr()
                && remaining_height > 0
            {
                rows.push(hr);
                remaining_height -= self._hr();
            }
            if remaining_height == 0 {
                break;
            }

            if self.is_current(i) {
                self.cursor_above = self.height - remaining_height;
            }

            // determine prefix
            let is_selected = selector.contains(item);
            let is_first = rows.is_empty();
            let is_last_in_results = drain_iter.peek().is_none();
            let h = if as_cols {
                row.iter()
                    .map(|t| t.height() as u16)
                    .max()
                    .unwrap_or_default()
            } else {
                row.iter().map(|t| t.height() as u16).sum::<u16>()
            };
            let is_last = is_last_in_results || (remaining_height <= h);
            let is_current_row = self.is_current(i);
            let (prefix, icon_name_hz, is_spinner, spinner_col_idx, is_yanked, is_cut) = self
                .row_prefix(
                    &mut row,
                    is_selected,
                    i,
                    item,
                    columns,
                    is_first,
                    is_last,
                    cwd,
                    &get_dynamic_multi_prefix,
                );
            let nav_bar_span =
                get_nav_bar_span(is_first, is_last, is_current_row, is_yanked, is_cut);
            let is_pinned = Self::is_path_in_set(&self.pin_paths, &icon_name_hz, cwd);
            let b_color = if is_pinned || self.mode_index == 2 {
                Some(bookmark_color(&self.config, &icon_name_hz))
            } else {
                None
            };

            let row_ctx = RowCtx {
                i,
                prefix,
                icon_name_hz,
                is_spinner,
                spinner_col_idx,
                is_yanked,
                is_cut,
                is_selected,
                is_current_row,
                is_pinned,
                b_color,
                nav_bar_span,
            };
            remaining_height = if as_cols {
                self.push_columns_row(frame, &mut drain_iter, rows, row, row_ctx, remaining_height)
            } else {
                self.push_stacked_row(frame, &mut drain_iter, rows, row, row_ctx, remaining_height)
            };
            i += 1;
        }
        (i, remaining_height)
    }

    /// One item as a table row with a cell per column; returns the height left over.
    fn push_columns_row<'a, T: SSS>(
        &self,
        frame: &RowsFrame<'_>,
        drain_iter: &mut RowsIter<'_, 'a, T>,
        rows: &mut Vec<Row<'a>>,
        mut row: Vec<ratatui::text::Text<'a>>,
        row_ctx: RowCtx,
        mut remaining_height: u16,
    ) -> u16 {
        let RowCtx {
            i,
            prefix,
            icon_name_hz,
            is_spinner,
            spinner_col_idx,
            is_yanked,
            is_cut,
            is_selected,
            is_current_row,
            is_pinned,
            b_color,
            nav_bar_span,
        } = row_ctx;
        let RowsFrame {
            styler,
            widths,
            cwd,
            is_underline_mode,
            ..
        } = *frame;
        let style_text = |t: ratatui::text::Text<'a>, x: usize, is_current_row: bool| {
            styler.style_text(t, x, is_current_row)
        };
        let apply_tier_underline =
            |t: &mut ratatui::text::Text<'_>, target_w: usize, skip_spans: usize| {
                styler.tier_underline(t, target_w, skip_spans)
            };
        // scroll down
        if self.is_current(i) && self.config.vscroll_current_only && self.vscroll > 0 {
            for (x, t) in row.iter_mut().enumerate().filter(|(i, _)| widths[*i] != 0) {
                if self.col.is_none() || self.col() == Some(x) {
                    let scroll = self.vscroll as usize;

                    if scroll < t.lines.len() {
                        t.lines = t.lines.split_off(scroll);
                    } else {
                        t.lines.clear();
                    }
                }
            }
        }

        let mut height = row
            .iter()
            .map(|t| t.height() as u16)
            .max()
            .unwrap_or_default();

        if remaining_height < height {
            height = remaining_height;

            for (_, t) in row.iter_mut().enumerate().filter(|(i, _)| widths[*i] != 0) {
                clip_text_lines(t, height, self.reverse());
            }
        }
        remaining_height -= height;

        // same as above
        let first_visible = widths.iter().position(|&w| w != 0).unwrap_or(0);
        let target_prefix_col =
            if is_spinner && widths.get(spinner_col_idx).copied().unwrap_or(0) != 0 {
                spinner_col_idx
            } else {
                first_visible
            };

        let last_visible = widths
            .iter()
            .enumerate()
            .rev()
            .find_map(|(i, w)| (*w != 0).then_some(i));

        let mut row_texts: Vec<_> = row
            .iter()
            .take(last_visible.map(|x| x + 1).unwrap_or(0))
            .cloned()
            // highlight
            .enumerate()
            .map(|(x, mut t)| {
                t = style_text(t, x, self.is_current(i));
                if let Some(color) = b_color {
                    apply_bookmark_text_style(&mut t, color);
                }

                // prefix after hscroll
                if x == target_prefix_col {
                    prefix_span(
                        &mut t,
                        prefix.clone(),
                        self.active_prefix_style(&icon_name_hz, is_selected, is_spinner, cwd),
                        self.inactive_prefix_style(
                            &icon_name_hz,
                            is_selected && !is_current_row,
                            is_spinner,
                            cwd,
                        ),
                        is_current_row,
                        if !is_selected {
                            nav_bar_span.clone()
                        } else {
                            None
                        },
                        self.config.current_style,
                        self.config.current_nav_bar_style,
                    );
                    if self.config.icons {
                        insert_icon_span(
                            &mut t,
                            &icon_name_hz,
                            !is_selected && nav_bar_span.is_some(),
                            is_current_row,
                            self.config.uncolor_current_icon,
                            self.config.invert_current_icon,
                            self.config.current_icon_style,
                            is_pinned,
                            is_yanked,
                            is_cut,
                            self.mode_index,
                            &self.config,
                            self.get_flash_op(&icon_name_hz, cwd),
                        );
                    }
                    if self.config.symlink.active {
                        maybe_append_symlink_target(
                            &mut t,
                            &icon_name_hz,
                            self.config.symlink.style.into(),
                            widths[target_prefix_col],
                        );
                    }
                };
                t
            })
            .collect();

        if self.config.right_align_last && row_texts.len() > 1 {
            row_texts.last_mut().unwrap().alignment = Some(Alignment::Right)
        }

        if is_underline_mode
            && matches!(
                drain_iter.peek(),
                Some((Some(crate::nucleo::GroupHeader::TierSeparator), _, _))
            )
        {
            let total_allocated: u16 = widths.iter().sum();
            let total_available = self.width.saturating_sub(self.column_spacing_width());
            let surplus = total_available.saturating_sub(total_allocated);
            let num_cols = row_texts.len();
            let has_nav = !is_selected && nav_bar_span.is_some();
            let prefix_skip = if has_nav { 2 } else { 1 };
            for (col_idx, t) in row_texts.iter_mut().enumerate() {
                let is_last_col = col_idx == num_cols.saturating_sub(1);
                let base_w = widths.get(col_idx).copied().unwrap_or(0);
                let target_w = if is_last_col {
                    base_w + surplus
                } else {
                    base_w
                } as usize;
                let skip = if col_idx == target_prefix_col {
                    prefix_skip
                } else {
                    0
                };
                apply_tier_underline(t, target_w, skip);
            }
        }

        // push
        let mut row = Row::new(row_texts).height(height);

        if self.is_current(i) {
            match self.config.row_connection {
                RowConnectionStyle::Capped => row = row.style(self.config.inactive_current_style),
                RowConnectionStyle::Full => row = row.style(self.config.current_style),
                _ => {}
            }
        } else if is_selected {
            row = row.style(Style::from(self.config.selected_style));
        }

        rows.push(row);
        remaining_height
    }

    /// One item as stacked rows, one per column; returns the height left over.
    fn push_stacked_row<'a, T: SSS>(
        &self,
        frame: &RowsFrame<'_>,
        drain_iter: &mut RowsIter<'_, 'a, T>,
        rows: &mut Vec<Row<'a>>,
        row: Vec<ratatui::text::Text<'a>>,
        row_ctx: RowCtx,
        mut remaining_height: u16,
    ) -> u16 {
        let RowCtx {
            i,
            prefix,
            icon_name_hz,
            is_spinner,
            spinner_col_idx: _,
            is_yanked,
            is_cut,
            is_selected,
            is_current_row: _,
            is_pinned,
            b_color,
            nav_bar_span,
        } = row_ctx;
        let RowsFrame {
            styler,
            cwd,
            is_underline_mode,
            active_column,
            ..
        } = *frame;
        let apply_tier_underline =
            |t: &mut ratatui::text::Text<'_>, target_w: usize, skip_spans: usize| {
                styler.tier_underline(t, target_w, skip_spans)
            };
        let mut push = vec![];
        let mut vscroll_to_skip = if self.is_current(i) && self.config.vscroll_current_only {
            self.vscroll as usize
        } else {
            0
        };

        for (x, mut col) in row.into_iter().enumerate() {
            if vscroll_to_skip > 0 {
                let col_height = col.lines.len();
                if vscroll_to_skip >= col_height {
                    vscroll_to_skip -= col_height;
                    continue;
                } else {
                    col.lines = col.lines.split_off(vscroll_to_skip);
                    vscroll_to_skip = 0;
                }
            }

            let mut height = col.height() as u16;

            if remaining_height == 0 {
                break;
            } else if remaining_height < height {
                height = remaining_height;
                clip_text_lines(&mut col, remaining_height, self.reverse());
            }
            remaining_height -= height;

            if let Some(color) = b_color {
                apply_bookmark_text_style(&mut col, color);
            }

            let is_current_row = self.is_current(i);
            prefix_span(
                &mut col,
                prefix.clone(),
                self.active_prefix_style(&icon_name_hz, is_selected, is_spinner, cwd),
                self.inactive_prefix_style(
                    &icon_name_hz,
                    is_selected && !is_current_row,
                    is_spinner,
                    cwd,
                ),
                is_current_row,
                if !is_selected {
                    nav_bar_span.clone()
                } else {
                    None
                },
                self.config.current_style,
                self.config.current_nav_bar_style,
            );
            if self.config.icons && x == 0 {
                insert_icon_span(
                    &mut col,
                    &icon_name_hz,
                    !is_selected && nav_bar_span.is_some(),
                    is_current_row,
                    self.config.uncolor_current_icon,
                    self.config.invert_current_icon,
                    self.config.current_icon_style,
                    is_pinned,
                    is_yanked,
                    is_cut,
                    self.mode_index,
                    &self.config,
                    self.get_flash_op(&icon_name_hz, cwd),
                );
            }
            if self.config.symlink.active && x == 0 {
                maybe_append_symlink_target(
                    &mut col,
                    &icon_name_hz,
                    self.config.symlink.style.into(),
                    self.width,
                );
            }

            let is_active_col = active_column == x;

            match self.config.row_connection {
                RowConnectionStyle::Disjoint => {
                    if is_active_col {
                        col = col.style(if is_current_row {
                            self.config.current_style
                        } else {
                            self.config.style
                        });
                    } else {
                        col = col.style(if is_current_row {
                            self.config.inactive_current_style
                        } else {
                            self.config.inactive_style
                        });
                    }
                }
                RowConnectionStyle::Capped => {
                    if is_active_col {
                        col = col.style(if is_current_row {
                            self.config.current_style
                        } else {
                            self.config.style
                        });
                    }
                }
                RowConnectionStyle::Full => {}
            }

            if is_underline_mode
                && matches!(
                    drain_iter.peek(),
                    Some((Some(crate::nucleo::GroupHeader::TierSeparator), _, _))
                )
            {
                let target_w = self.width as usize;
                let has_nav = !is_selected && nav_bar_span.is_some();
                let prefix_skip = if has_nav { 2 } else { 1 };
                let skip = if x == 0 { prefix_skip } else { 0 };
                apply_tier_underline(&mut col, target_w, skip);
            }

            // push
            let mut row = Row::new(vec![col]).height(height);
            if is_current_row {
                match self.config.row_connection {
                    RowConnectionStyle::Capped => {
                        row = row.style(self.config.inactive_current_style)
                    }
                    RowConnectionStyle::Full => row = row.style(self.config.current_style),
                    _ => {}
                }
            } else if is_selected {
                row = row.style(Style::from(self.config.selected_style));
            }
            push.push(row);
        }
        rows.extend(push);
        remaining_height
    }

    pub fn make_table<'a, T: SSS>(
        &mut self,
        active_column: usize,
        worker: &'a mut Worker<T>,
        selector: &mut Selector<T, impl Selection>,
        matcher: &mut nucleo::Matcher,
        click: &mut Click,
        nav_bar_style: Option<(ratatui::widgets::BorderType, ratatui::style::Style)>,
        freeze_snapshot: bool,
    ) -> Table<'a> {
        self.apply_initial_pos(worker);

        let cwd = std::env::current_dir().unwrap_or_default();
        let offset = self.bottom;
        let end = self.bottom + self.height as u32;
        let as_cols = !self.config.stacked_columns;

        let width_limits = if as_cols {
            self.max_widths()
        } else {
            let default = self.width.saturating_sub(self.indentation() as u16);

            (0..worker.columns.len())
                .map(|i| {
                    if self.hidden_columns.get(i).copied().unwrap_or(false) {
                        0
                    } else {
                        default
                    }
                })
                .collect()
        };

        let columns = worker.columns.clone();
        let (mut results, mut widths, medians, status) = worker.results(
            offset,
            end,
            &width_limits,
            self.config.wrap,
            self.config.max_height,
            self.config.match_style.into(),
            matcher,
            self.config.autoscroll.clone(),
            self.hscroll,
            (
                if self.config.vscroll_current_only {
                    0
                } else {
                    self.vscroll
                },
                !as_cols,
            ),
            self.config.show_skipped,
            freeze_snapshot,
        );
        let results_len = results.len();

        // log::trace!(
        //     "len: {}, hscroll: {},  offset: {}, end: {}, limits: {:?}, medians: {:?}, last_widths: {:?}",
        //     results.len(),
        //     self.hscroll,
        //     offset,
        //     end,
        //     width_limits,
        //     medians,
        //     self.widths
        // );

        self.status = status.clone();
        self.medians = medians;
        self.adjust_widths(&mut widths, &results);

        // should generally be true already, but act as a safeguard
        // for x in widths.iter_mut().zip(&self.hidden_columns) {
        //     if *x.1 {
        //         *x.0 = 0
        //     }
        // }
        let widths = widths;

        let match_count = status.matched_count;

        if match_count < self.bottom + self.cursor as u32 && !self.cursor_disabled {
            if !status.running && match_count > 0 {
                self.cursor_jump(match_count);
            }
        } else {
            self.cursor = self.cursor.min(results.len().saturating_sub(1) as u16);
        }

        let is_underline_mode = self.config.tier.separator == HorizontalSeparator::Underline;

        let mut rows = vec![];
        let total_height = 0;

        if results.is_empty() {
            return Table::new(rows, widths);
        }

        let effective_active_col = if widths.get(active_column).copied().unwrap_or(0) == 0 {
            widths.iter().position(|&w| w != 0).unwrap_or(active_column)
        } else {
            active_column
        };
        let styler = RowStyler::new(
            &self.config,
            nav_bar_style,
            effective_active_col,
            as_cols,
            self._hr(),
        );
        let height_of = |t: &(
            Option<crate::nucleo::GroupHeader>,
            Vec<ratatui::text::Text<'a>>,
            _,
        )| styler.height_of(&t.0, &t.1);

        // log::trace!("results initial: {}, {}, {}, {}, {}", self.bottom, self.cursor, total_height, self.height, results.len());
        let h_at_cursor = height_of(&results[self.cursor as usize]);
        let h_after_cursor = results[self.cursor as usize + 1..]
            .iter()
            .map(height_of)
            .sum();
        let h_to_cursor = results[0..self.cursor as usize]
            .iter()
            .map(height_of)
            .sum::<u16>();
        let cursor_end_should_lte = self.height - self.scroll_padding().min(h_after_cursor);
        // let cursor_start_should_gt = self.scroll_padding().min(h_to_cursor);

        // log::trace!(
        //     "Computed heights: {}, {h_at_cursor}, {h_to_cursor}, {h_after_cursor}, {cursor_end_should_lte}",
        //     self.cursor
        // );

        // begin adjustment
        let start_index = 0; // the index in results of the first complete item
        let (start_index, total_height) = self.scroll_to_cursor_rows(
            &styler,
            &mut results,
            &mut rows,
            selector,
            &columns,
            &cwd,
            &widths,
            results_len,
            as_cols,
            is_underline_mode,
            h_at_cursor,
            h_to_cursor,
            cursor_end_should_lte,
            start_index,
            total_height,
        );

        // topside padding is not self-correcting, and can only do its best to stay at #padding lines without obscuring cursor on cursor movement events.
        let remaining_height = self.height.saturating_sub(total_height);

        let i = self.bottom_clip.is_some() as usize;

        let active_group_name = self.active_group_name(&results, start_index);

        let (i, remaining_height) = self.push_visible_rows(
            &styler,
            &mut results,
            start_index,
            &mut rows,
            click,
            selector,
            &columns,
            &cwd,
            &widths,
            as_cols,
            is_underline_mode,
            &active_group_name,
            active_column,
            i,
            remaining_height,
        );

        // doesn't loop back after results is exhausted so we have to set here
        if let Click::ResultPos(_c) = click {
            log::debug!("Mapped click to last row = {i}");
            *click = Click::ResultIdx(self.bottom + i as u32 - 1);
        }

        self.finalize_table(rows, remaining_height, widths, as_cols)
    }

    #[allow(clippy::too_many_arguments)]
    fn decorate_cut_col_icon_and_symlink(
        &self,
        t: &mut ratatui::text::Text<'_>,
        icon_name: &str,
        is_selected: bool,
        is_current_row: bool,
        is_pinned: bool,
        is_yanked: bool,
        is_cut: bool,
        nav_bar_span: &Option<ratatui::text::Span<'static>>,
        cwd: &std::path::Path,
        widths: &[u16],
        target_prefix_col: usize,
    ) {
        if self.config.icons {
            insert_icon_span(
                t,
                icon_name,
                !is_selected && nav_bar_span.is_some(),
                is_current_row,
                self.config.uncolor_current_icon,
                self.config.invert_current_icon,
                self.config.current_icon_style,
                is_pinned,
                is_yanked,
                is_cut,
                self.mode_index,
                &self.config,
                self.get_flash_op(icon_name, cwd),
            );
        }
        if self.config.symlink.active {
            maybe_append_symlink_target(
                t,
                icon_name,
                self.config.symlink.style.into(),
                widths[target_prefix_col],
            );
        }
    }

    fn active_group_name<'a, T: SSS>(
        &self,
        results: &crate::nucleo::WorkerResults<'a, T>,
        start_index: u16,
    ) -> Option<std::sync::Arc<str>> {
        if !self.cursor_disabled {
            let mut curr = None;
            let base_i = self.bottom_clip.is_some() as usize;
            let cursor_offset = (self.cursor as usize).saturating_sub(base_i);
            let target_idx = (start_index as usize).saturating_add(cursor_offset);
            for (k, (g, _, _)) in results.iter().enumerate() {
                if let Some(crate::nucleo::GroupHeader::Named(name)) = g {
                    curr = Some(name.clone());
                }
                if k == target_idx {
                    break;
                }
            }
            curr
        } else {
            None
        }
    }

    fn push_styled_cut_cols_row<'a>(
        &self,
        mut row_texts: Vec<ratatui::text::Text<'a>>,
        rows: &mut Vec<Row<'a>>,
        remaining_height: u16,
        is_selected: bool,
    ) {
        if self.config.right_align_last && row_texts.len() > 1 {
            row_texts.last_mut().unwrap().alignment = Some(Alignment::Right)
        }

        let text_h = row_texts
            .iter()
            .map(|t| t.lines.len() as u16)
            .max()
            .unwrap_or(1);
        let row = Row::new(row_texts).height(text_h.min(remaining_height));
        let row = if is_selected {
            row.style(Style::from(self.config.selected_style))
        } else {
            row
        };
        rows.push(row);
    }

    #[allow(clippy::too_many_arguments)]
    fn push_cut_stacked_col<'a>(
        &self,
        col: &mut ratatui::text::Text<'a>,
        push: &mut Vec<Row<'a>>,
        col_idx: usize,
        height: u16,
        b_color: Option<ratatui::style::Color>,
        prefix: &str,
        icon_name: &str,
        is_selected: bool,
        is_spinner: bool,
        is_current_row: bool,
        is_pinned: bool,
        is_yanked: bool,
        is_cut: bool,
        nav_bar_span: &Option<ratatui::text::Span<'static>>,
        cwd: &std::path::Path,
    ) {
        if let Some(color) = b_color {
            apply_bookmark_text_style(col, color);
        }

        prefix_span(
            col,
            prefix.to_owned(),
            self.active_prefix_style(icon_name, is_selected, is_spinner, cwd),
            self.inactive_prefix_style(icon_name, is_selected, is_spinner, cwd),
            is_current_row,
            if !is_selected {
                nav_bar_span.clone()
            } else {
                None
            },
            self.config.current_style,
            self.config.current_nav_bar_style,
        );
        if self.config.icons && col_idx == 0 {
            insert_icon_span(
                col,
                icon_name,
                !is_selected && nav_bar_span.is_some(),
                is_current_row,
                self.config.uncolor_current_icon,
                self.config.invert_current_icon,
                self.config.current_icon_style,
                is_pinned,
                is_yanked,
                is_cut,
                self.mode_index,
                &self.config,
                self.get_flash_op(icon_name, cwd),
            );
        }
        if self.config.symlink.active && col_idx == 0 {
            maybe_append_symlink_target(
                col,
                icon_name,
                self.config.symlink.style.into(),
                self.width,
            );
        }

        let row = Row::new(vec![col.clone()]).height(height);
        let row = if is_selected {
            row.style(Style::from(self.config.selected_style))
        } else {
            row
        };
        push.push(row);
    }

    #[allow(clippy::too_many_arguments)]
    fn push_kept_cols_top_row<'a>(
        &self,
        row: &mut [ratatui::text::Text<'a>],
        rows: &mut Vec<Row<'a>>,
        widths: &[u16],
        h: u16,
        remaining_height: u16,
        is_spinner: bool,
        spinner_col_idx: usize,
        is_current_row: bool,
        is_topside_tier_underlined: bool,
        b_color: Option<ratatui::style::Color>,
        prefix: &str,
        icon_name: String,
        is_selected: bool,
        is_pinned: bool,
        is_yanked: bool,
        is_cut: bool,
        nav_bar_span: &Option<ratatui::text::Span<'static>>,
        cwd: &std::path::Path,
        style_text: &impl Fn(ratatui::text::Text<'a>, usize, bool) -> ratatui::text::Text<'a>,
        apply_tier_underline: &impl Fn(&mut ratatui::text::Text<'_>, usize, usize),
    ) {
        if self._hr() + remaining_height != h {
            for (_, t) in row.iter_mut().enumerate().filter(|(i, _)| widths[*i] != 0) {
                clip_text_lines(t, remaining_height, !self.reverse());
            }
        }

        let first_visible = widths.iter().position(|&w| w != 0).unwrap_or(0);
        let target_prefix_col =
            if is_spinner && widths.get(spinner_col_idx).copied().unwrap_or(0) != 0 {
                spinner_col_idx
            } else {
                first_visible
            };

        let last_visible = widths
            .iter()
            .enumerate()
            .rev()
            .find_map(|(i, w)| (*w != 0).then_some(i));

        let mut row_texts: Vec<_> = row
            .iter()
            .take(last_visible.map(|x| x + 1).unwrap_or(0))
            .cloned()
            .enumerate()
            .map(|(x, mut t)| {
                t = style_text(t, x, is_current_row);
                if let Some(color) = b_color {
                    apply_bookmark_text_style(&mut t, color);
                }
                if x == target_prefix_col {
                    prefix_span(
                        &mut t,
                        prefix.to_owned(),
                        self.active_prefix_style(&icon_name, is_selected, is_spinner, cwd),
                        self.inactive_prefix_style(
                            &icon_name,
                            is_selected && !is_current_row,
                            is_spinner,
                            cwd,
                        ),
                        is_current_row,
                        if !is_selected {
                            nav_bar_span.clone()
                        } else {
                            None
                        },
                        self.config.current_style,
                        self.config.current_nav_bar_style,
                    );
                    if self.config.icons {
                        insert_icon_span(
                            &mut t,
                            &icon_name,
                            !is_selected && nav_bar_span.is_some(),
                            is_current_row,
                            self.config.uncolor_current_icon,
                            self.config.invert_current_icon,
                            self.config.current_icon_style,
                            is_pinned,
                            is_yanked,
                            is_cut,
                            self.mode_index,
                            &self.config,
                            self.get_flash_op(&icon_name, cwd),
                        );
                    }
                    if self.config.symlink.active {
                        maybe_append_symlink_target(
                            &mut t,
                            &icon_name,
                            self.config.symlink.style.into(),
                            widths[target_prefix_col],
                        );
                    }
                }
                t
            })
            .collect();

        if self.config.right_align_last && row_texts.len() > 1 {
            row_texts.last_mut().unwrap().alignment = Some(Alignment::Right)
        }

        if is_topside_tier_underlined {
            let total_allocated: u16 = widths.iter().sum();
            let total_available = self.width.saturating_sub(self.column_spacing_width());
            let surplus = total_available.saturating_sub(total_allocated);
            let num_cols = row_texts.len();
            let has_nav = !is_selected && nav_bar_span.is_some();
            let prefix_skip = if has_nav { 2 } else { 1 };
            for (col_idx, t) in row_texts.iter_mut().enumerate() {
                let is_last_col = col_idx == num_cols.saturating_sub(1);
                let base_w = widths.get(col_idx).copied().unwrap_or(0);
                let target_w = if is_last_col {
                    base_w + surplus
                } else {
                    base_w
                } as usize;
                let skip = if col_idx == target_prefix_col {
                    prefix_skip
                } else {
                    0
                };
                apply_tier_underline(t, target_w, skip);
            }
        }

        let text_h = row_texts
            .iter()
            .map(|t| t.lines.len() as u16)
            .max()
            .unwrap_or(1);
        let row = Row::new(row_texts).height(text_h.min(remaining_height));
        let row = if is_selected && !is_current_row {
            row.style(Style::from(self.config.selected_style))
        } else {
            row
        };
        rows.push(row);
    }

    fn finalize_table<'a>(
        &mut self,
        mut rows: Vec<Row<'a>>,
        remaining_height: u16,
        widths: Vec<u16>,
        as_cols: bool,
    ) -> Table<'a> {
        if self.reverse() {
            rows.reverse();
            if remaining_height > 0 {
                rows.insert(0, Row::new(vec![vec![]]).height(remaining_height));
            }
        }

        // ratatui column_spacing eats into the constraints
        let table_widths = if as_cols {
            // first 0 element after which all is 0
            let pos = widths.iter().rposition(|&x| x != 0);
            // column_spacing eats into the width
            let mut widths: Vec<_> = widths[..pos.map_or(0, |x| x + 1)].to_vec();

            let total_available = self.width.saturating_sub(self.column_spacing_width());
            let surplus = total_available.saturating_sub(widths.iter().sum());

            if surplus > 0 {
                // occupy full row
                let is_single_col = widths.iter().filter(|&&w| w != 0).count() <= 1;
                if (matches!(self.config.row_connection, RowConnectionStyle::Full)
                    || is_single_col
                    || (matches!(self.config.row_connection, RowConnectionStyle::Disjoint)
                        && self.config.right_align_last))
                    && let Some(s) = widths.iter_mut().rfind(|w| **w != 0)
                {
                    *s += surplus;
                }
            }

            // save actual widths of each column
            self.widths = widths.clone();

            widths
        } else {
            vec![self.width]
        };

        // log::trace!(
        //     "limits: {width_limits:?}, widths: {widths:?}, {:?}, medians {:?}",
        //     self.width,
        //     self.medians
        // );

        let mut table = Table::new(rows, table_widths).column_spacing(self.config.column_spacing.0);

        table = match self.config.row_connection {
            RowConnectionStyle::Full => table.style(self.config.style),
            RowConnectionStyle::Capped => table.style(self.config.inactive_style),
            _ => table,
        };

        // log::trace!("{table:?}");

        if !self.config.border.is_empty() {
            table = table.block(self.config.border.as_static_block());
        }
        table
    }
}

impl ResultsUI {
    fn default_prefix(&self, i: usize) -> String {
        let substituted = substitute_escaped(
            &self.config.unselected_prefix,
            &[
                ('d', &(i + 1).to_string()),                        // cursor index
                ('r', &(i + 1 + self.bottom as usize).to_string()), // absolute index
            ],
        );

        fit_width(&substituted, self.config.multi_prefix.width())
    }

    fn is_current(&self, i: usize) -> bool {
        !self.cursor_disabled && self.cursor == i as u16
    }

    fn hr(&self) -> Option<Row<'static>> {
        let sep = self.config.separator;

        if matches!(sep, HorizontalSeparator::None) {
            return None;
        }

        let unit = sep.as_str();
        let line = unit.repeat(self.width as usize);

        // todo: support non_stacked properly by doing a seperate rendering pass
        if !self.config.stacked_columns && self.widths.len() > 1 {
            // Some(Row::new(vec![vec![]]))
            Some(Row::new(vec![line; self.widths().len()]).style(self.config.separator_style))
        } else {
            Some(Row::new(vec![line]).style(self.config.separator_style))
        }
    }

    fn _hr(&self) -> u16 {
        !matches!(self.config.separator, HorizontalSeparator::None) as u16
    }
}
