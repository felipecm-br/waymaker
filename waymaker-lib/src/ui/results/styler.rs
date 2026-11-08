use ratatui::{
    style::{Modifier, Style},
    text::Span,
};

use crate::config::{HorizontalSeparator, RowConnectionStyle};

/// The nav-bar glyph for a border type.
pub(super) fn nav_border_char(border_type: ratatui::widgets::BorderType) -> &'static str {
    match border_type {
        ratatui::widgets::BorderType::Plain | ratatui::widgets::BorderType::Rounded => "│",
        ratatui::widgets::BorderType::Double => "║",
        ratatui::widgets::BorderType::Thick => "█",
        ratatui::widgets::BorderType::QuadrantOutside => "▌",
        ratatui::widgets::BorderType::QuadrantInside => "▐",
        _ => "│",
    }
}

/// What make_table needs, per frame, to style a row: copied out of the config so the row code
/// can take it alongside `&mut self`.
/// Values the same for every row of one table, read by both row layouts.
#[derive(Clone, Copy)]
pub(super) struct RowsFrame<'f> {
    pub(super) styler: &'f RowStyler,
    pub(super) widths: &'f [u16],
    pub(super) cwd: &'f std::path::Path,
    pub(super) is_underline_mode: bool,
    pub(super) active_column: usize,
}

/// One `make_table` pass: what the row helpers read (the frame, the selection, the columns) and
/// what they consume and build (the frame's results and the table rows).
pub(super) struct TablePass<'p, 'a, T: crate::SSS, S: crate::Selection> {
    pub(super) frame: RowsFrame<'p>,
    pub(super) results: crate::nucleo::WorkerResults<'a, T>,
    pub(super) rows: Vec<ratatui::widgets::Row<'a>>,
    pub(super) selector: &'p crate::Selector<T, S>,
    pub(super) columns: &'p std::sync::Arc<[crate::nucleo::Column<T>]>,
    pub(super) results_len: usize,
    pub(super) as_cols: bool,
}

/// One item's values, computed once by `push_visible_rows` and read by both row layouts.
pub(super) struct RowCtx {
    pub(super) i: usize,
    pub(super) prefix: String,
    pub(super) icon_name_hz: String,
    pub(super) is_spinner: bool,
    pub(super) spinner_col_idx: usize,
    pub(super) is_yanked: bool,
    pub(super) is_cut: bool,
    pub(super) is_selected: bool,
    pub(super) is_current_row: bool,
    pub(super) is_pinned: bool,
    pub(super) b_color: Option<ratatui::style::Color>,
    pub(super) nav_bar_span: Option<ratatui::text::Span<'static>>,
}

/// Where one item sits in the table, from which [`RowCtx`] is computed.
#[derive(Clone, Copy)]
pub(super) struct RowPos {
    pub(super) i: usize,
    pub(super) is_selected: bool,
    pub(super) is_first: bool,
    pub(super) is_last: bool,
    pub(super) is_current_row: bool,
}

/// The rest of a frame's results, peeked to see whether a tier separator follows a row.
pub(super) type RowsIter<'d, 'a, T> = std::iter::Peekable<
    std::vec::Drain<
        'd,
        (
            Option<crate::nucleo::GroupHeader>,
            Vec<ratatui::text::Text<'a>>,
            &'a T,
        ),
    >,
>;

pub(super) struct RowStyler {
    nav_bar_style: Option<(ratatui::widgets::BorderType, Style)>,
    current_nav_bar: Option<ratatui::widgets::BorderType>,
    multi_prefix: String,
    /// The multi-select prefix with the nav bar as its first cell: (both, first, last, mid).
    nav_bar_multi_prefixes: Option<(String, String, String, String)>,
    tier_sep_style: crate::config::StyleSetting,
    tier_separator: HorizontalSeparator,
    row_connection: RowConnectionStyle,
    style: crate::config::StyleSetting,
    current_style: crate::config::StyleSetting,
    inactive_style: crate::config::StyleSetting,
    inactive_current_style: crate::config::StyleSetting,
    effective_active_col: usize,
    as_cols: bool,
    hr: u16,
}

impl RowStyler {
    pub(super) fn new(
        config: &crate::config::ResultsConfig,
        nav_bar_style: Option<(ratatui::widgets::BorderType, Style)>,
        effective_active_col: usize,
        as_cols: bool,
        hr: u16,
    ) -> Self {
        let multi_prefix = config.multi_prefix.clone();
        let nav_bar_multi_prefixes = nav_bar_style.as_ref().map(|(border_type, _)| {
            let rest: String = multi_prefix.chars().skip(1).collect();
            let make = |_is_first: bool, _is_last: bool| -> String {
                let border_char = nav_border_char(*border_type);
                if multi_prefix.chars().next().is_some() {
                    format!("{}{}", border_char, rest)
                } else {
                    format!("{} ", border_char)
                }
            };
            (
                make(true, true),
                make(true, false),
                make(false, true),
                make(false, false),
            )
        });
        Self {
            nav_bar_style,
            current_nav_bar: config.current_nav_bar,
            multi_prefix,
            nav_bar_multi_prefixes,
            tier_sep_style: config.tier.style,
            tier_separator: config.tier.separator,
            row_connection: config.row_connection.clone(),
            style: config.style,
            current_style: config.current_style,
            inactive_style: config.inactive_style,
            inactive_current_style: config.inactive_current_style,
            effective_active_col,
            as_cols,
            hr,
        }
    }

    pub(super) fn nav_bar_span(
        &self,
        _is_first: bool,
        _is_last: bool,
        is_current: bool,
        is_yanked: bool,
        is_cut: bool,
    ) -> Option<ratatui::text::Span<'static>> {
        self.nav_bar_style.as_ref().map(|(border_type, style)| {
            let bt = if is_current {
                self.current_nav_bar.unwrap_or(*border_type)
            } else {
                *border_type
            };
            let mut st = *style;
            if is_cut {
                st = st.fg(ratatui::style::Color::Red);
            } else if is_yanked {
                st = st.fg(ratatui::style::Color::Yellow);
            }
            let ch = nav_border_char(bt);
            ratatui::text::Span::styled(ch, st)
        })
    }

    pub(super) fn dynamic_multi_prefix(&self, is_first: bool, is_last: bool) -> String {
        if let Some((both, first, last, mid)) = &self.nav_bar_multi_prefixes {
            if is_first && is_last {
                both.clone()
            } else if is_first {
                first.clone()
            } else if is_last {
                last.clone()
            } else {
                mid.clone()
            }
        } else {
            self.multi_prefix.clone()
        }
    }

    /// Underlines the last line of `t` (padded to `target_w`) to mark a tier boundary.
    pub(super) fn tier_underline(
        &self,
        t: &mut ratatui::text::Text<'_>,
        target_w: usize,
        skip_spans: usize,
    ) {
        let tier_sep_style = self.tier_sep_style;
        if let Some(last_line) = t.lines.last_mut() {
            let cur_w = last_line.width();
            if cur_w < target_w {
                let pad_style = if let Some(fg) = tier_sep_style.fg {
                    Style::default()
                        .fg(fg)
                        .underline_color(fg)
                        .add_modifier(Modifier::UNDERLINED)
                } else {
                    Style::default().add_modifier(Modifier::UNDERLINED)
                };
                last_line
                    .spans
                    .push(Span::styled(" ".repeat(target_w - cur_w), pad_style));
            }
            for span in last_line.spans.iter_mut().skip(skip_spans) {
                span.style = span.style.add_modifier(Modifier::UNDERLINED);
                if let Some(fg) = tier_sep_style.fg {
                    span.style = span.style.underline_color(fg);
                }
            }
        }
    }

    /// Rows a result takes: its group header, the row separator, and its columns.
    pub(super) fn height_of(
        &self,
        group: &Option<crate::nucleo::GroupHeader>,
        cols: &[ratatui::text::Text<'_>],
    ) -> u16 {
        let group_h = match group {
            Some(crate::nucleo::GroupHeader::Named(_)) => 1,
            Some(crate::nucleo::GroupHeader::TierSeparator) => u16::from(!matches!(
                self.tier_separator,
                HorizontalSeparator::None | HorizontalSeparator::Underline
            )),
            None => 0,
        };
        group_h
            + self.hr
            + if self.as_cols {
                cols.iter()
                    .map(|t| t.height() as u16)
                    .max()
                    .unwrap_or_default()
            } else {
                cols.iter().map(|t| t.height() as u16).sum::<u16>()
            }
    }

    pub(super) fn style_text<'t>(
        &self,
        mut t: ratatui::text::Text<'t>,
        x: usize,
        is_current_row: bool,
    ) -> ratatui::text::Text<'t> {
        let is_active_col = self.effective_active_col == x;
        match self.row_connection {
            RowConnectionStyle::Disjoint => {
                if is_active_col {
                    t = t.style(if is_current_row {
                        self.current_style
                    } else {
                        self.style
                    });
                } else {
                    t = t.style(if is_current_row {
                        self.inactive_current_style
                    } else {
                        self.inactive_style
                    });
                }
            }
            RowConnectionStyle::Capped => {
                if is_active_col {
                    t = t.style(if is_current_row {
                        self.current_style
                    } else {
                        self.style
                    });
                }
            }
            RowConnectionStyle::Full => {}
        }
        t
    }
}
