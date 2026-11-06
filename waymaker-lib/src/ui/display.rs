use cba::bait::TransformExt;
use ratatui::{
    layout::Constraint,
    text::{Line, Text},
    widgets::{Cell, Paragraph, Row, Table},
};

use crate::{
    config::{DisplayConfig, RowConnectionStyle},
    utils::{
        serde::StringOrVec,
        text::{wrap_line, wrap_text, wrapping_indicator},
    },
};
pub type HeaderTable = Vec<Vec<Line<'static>>>;
#[derive(Debug, Default)]
pub struct DisplayUI {
    width: u16,
    height: u16,
    text: Vec<Text<'static>>,
    lines: HeaderTable, // lines from input
    pub show: bool,
    pub config: DisplayConfig,
}

impl DisplayUI {
    pub fn new(config: DisplayConfig) -> Self {
        let mut ret = Self {
            config,
            ..Default::default()
        };
        ret.init();

        ret
    }

    /// Refresh content and interactions from config.
    pub fn init(&mut self) {
        let (text, height) = match &self.config.content {
            Some(StringOrVec::String(s)) => {
                let text = parse_content_to_text(s);
                let height = text.height() as u16;
                (vec![text], height)
            }
            Some(StringOrVec::Vec(s)) => {
                let text: Vec<_> = s.iter().map(|s| parse_content_to_text(s)).collect();
                let height = text.iter().map(|t| t.height()).max().unwrap_or_default() as u16;
                (text, height)
            }
            _ => (vec![], 0),
        };

        for line in &mut self.config.interactions {
            line.sort_by_key(|(i, _)| *i);
        }

        self.text = text;
        self.height = height;
        self.show = self.config.content.is_some() || self.config.header_lines > 0;
    }

    pub fn update_width(&mut self, width: u16) {
        let border_w = self.config.border.width();
        let new_w = width.saturating_sub(border_w);
        self.width = new_w;
    }

    /// Return the content height in rows (text lines + border height), excluding any separator line.
    pub fn content_height(&self) -> u16 {
        if !self.show {
            return 0;
        }
        self.height + self.config.border.height()
    }

    pub fn height(&self) -> u16 {
        if !self.show {
            return 0;
        }
        let mut height = self.height;
        height += self.config.border.height();
        if self.config.separator != crate::config::HorizontalSeparator::None {
            height += 1;
        }

        height
    }

    /// Set text (single column) and show. The base style is applied "under" the text's styling.
    pub fn set(&mut self, text: impl Into<Text<'static>>) {
        self.text = vec![text.into()];
        self.height = self.text_height();

        self.show = true;
    }

    /// Rows the texts need before wrapping. Drawing refines it, but the layout of the frame
    /// right after a set/push uses it: without it a new header showed one event late.
    fn text_height(&self) -> u16 {
        self.text
            .iter()
            .map(|t| t.height())
            .max()
            .unwrap_or_default() as u16
    }

    /// Set text from a style template (`{cyan,bold:Key} {darkgray:Label}`, one line per
    /// line), as the config's `content` is parsed, and show.
    pub fn set_styled(&mut self, template: &str) {
        self.set(parse_content_to_text(template));
    }

    /// Add a column and show.
    pub fn push(&mut self, text: impl Into<Text<'static>>) {
        self.text.push(text.into());
        self.height = self.height.max(self.text_height());

        self.show = true;
    }

    pub fn clear(&mut self, keep_header: bool) {
        if !keep_header {
            self.lines.clear();
            self.show = false;
        } else if self.lines.is_empty() {
            self.show = false;
        }

        self.text.clear();
    }

    /// Whether this is table has just one column
    pub fn is_single_column(&self) -> bool {
        self.text.len() == 1
    }

    pub fn header_table(&mut self, table: HeaderTable) {
        self.lines = table;
        self.show = true;
    }

    // lowpri: how much to be gained by caching texts to not have to always rewrap?
    pub fn make_display(
        &mut self,
        result_indentation: u16,
        mut widths: Vec<u16>,
        col_spacing: u16,
    ) -> Table<'_> {
        if self.text.is_empty() && self.lines.is_empty() || widths.is_empty() {
            return Table::default();
        }

        let block = {
            let b = self.config.border.as_block();
            if self.config.match_indent {
                let mut padding = self.config.border.padding;

                padding.left = result_indentation.saturating_sub(self.config.border.left());
                if !widths.is_empty() {
                    widths[0] = widths[0].saturating_sub(result_indentation);
                }
                b.padding(padding.0)
            } else {
                b
            }
        };

        let (cells, height) = if self.is_single_column() {
            // Single Cell (Full Width)
            let mut text = wrap_text(
                self.text[0].clone(),
                if self.config.wrap { self.width } else { 0 },
            )
            .0;
            // The table cell is as wide as the results' first column, which can be narrower than
            // the footer; fit to that too, so this copy never shows past the full-width row
            // drawn over it (make_full_width_row).
            let cell_width = widths
                .first()
                .copied()
                .filter(|w| *w > 0)
                .unwrap_or(self.width);
            let limit = self.width.min(cell_width);
            if !self.config.wrap && limit > 0 {
                for line in &mut text.lines {
                    *line = fit_line(std::mem::take(line), limit as usize);
                }
            }
            let cells = vec![Cell::from(text)];
            let height = self.text[0].height() as u16;

            (cells, height)
        } else
        // Multiple (multi-line) columns
        {
            let mut height = 0;
            // wrap text according to result column widths
            let cells = self
                .text
                .iter()
                .cloned()
                .zip(widths.iter().copied())
                .map(|(text, width)| {
                    let ret = wrap_text(text, if self.config.wrap { width } else { 0 }).0;
                    height = height.max(ret.height() as u16);

                    Cell::from(ret.transform_if(
                        matches!(self.config.row_connection, RowConnectionStyle::Disjoint),
                        |t| t.style(self.config.style),
                    ))
                })
                .collect();

            (cells, height)
        };

        let row = Row::new(cells).style(self.config.style).height(height);
        let mut rows = vec![row];
        self.height = height;

        // add header_line cells
        if !self.lines.is_empty() {
            // todo: support wrapping on header lines
            rows.extend(self.lines.iter().map(|row| {
                let cells: Vec<Cell> = row
                    .iter()
                    .cloned()
                    .enumerate()
                    .map(|(i, l)| {
                        wrap_line(
                            l,
                            self.config
                                .wrap
                                .then_some(widths.get(i).cloned())
                                .flatten()
                                .unwrap_or_default(),
                            &wrapping_indicator(),
                        )
                    })
                    .map(Cell::from)
                    .collect();
                Row::new(cells)
            }));

            self.height += self.lines.len() as u16;
        }

        let widths = if self.is_single_column() && self.lines.iter().all(|x| x.len() == 1) {
            vec![Constraint::Percentage(100)]
        } else {
            let surplus = widths.iter().sum::<u16>().saturating_sub(self.width);
            if let Some(s) = widths.last_mut() {
                *s -= surplus;
            }
            widths.into_iter().map(Constraint::Length).collect()
        };

        Table::new(rows, widths)
            .block(block)
            .column_spacing(col_spacing)
            .transform_if(
                !matches!(self.config.row_connection, RowConnectionStyle::Disjoint),
                |t| t.style(self.config.style),
            )
    }

    /// Draw in the same area as display when self.single() to produce a full width row over the table area
    pub fn make_full_width_row(&self, result_indentation: u16) -> Paragraph<'_> {
        // Compute padding
        let left = if self.config.match_indent {
            result_indentation.saturating_sub(self.config.border.left())
        } else {
            self.config.border.left()
        };
        let top = self.config.border.top();
        let right = self.config.border.width().saturating_sub(left);
        let bottom = self.config.border.height() - top;

        let block = ratatui::widgets::Block::default().padding(ratatui::widgets::Padding {
            left,
            top,
            right,
            bottom,
        });

        let mut text = self.text[0].clone();
        let available = (self.width + self.config.border.width()).saturating_sub(left + right);
        if !self.config.wrap && available > 0 {
            for line in &mut text.lines {
                *line = fit_line(std::mem::take(line), available as usize);
            }
        }
        Paragraph::new(text).block(block).style(self.config.style)
    }
}

/// Shortens a line that does not fit in `width`. When it is a hint bar with `│` (or `|`)
/// separators, whole segments are dropped from the end, so the bar ends on a complete hint
/// (key and label); otherwise whole spans are. Trailing blank and separator spans go too, so
/// the result never ends in a word cut in half or a dangling separator.
fn fit_line(mut line: Line<'static>, width: usize) -> Line<'static> {
    if line.width() <= width {
        return line;
    }
    let is_sep = |span: &ratatui::text::Span| matches!(span.content.trim(), "│" | "|");
    let mut used = 0;
    let mut keep = 0;
    if line.spans.iter().any(is_sep) {
        // Keep spans up to the last separator before the overflow: whole segments only.
        for (i, span) in line.spans.iter().enumerate() {
            used += span.width();
            if used > width {
                break;
            }
            if is_sep(span) {
                keep = i;
            }
        }
    } else {
        keep = line
            .spans
            .iter()
            .take_while(|span| {
                used += span.width();
                used <= width
            })
            .count();
    }
    line.spans.truncate(keep);
    while line
        .spans
        .last()
        .is_some_and(|span| span.content.trim().is_empty() || is_sep(span))
    {
        line.spans.pop();
    }
    line
}

fn parse_content_to_text(s: &str) -> Text<'static> {
    let lines: Vec<Line<'static>> = s
        .lines()
        .map(crate::ui::StatusUI::parse_template_to_status_line)
        .collect();
    Text::from(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::{Color, Modifier};

    #[test]
    fn test_fit_line_drops_whole_hints_and_trailing_separators() {
        let line = crate::ui::StatusUI::parse_template_to_status_line(
            " {cyan,bold:enter} {darkgray:accept} {darkgray:│} {cyan,bold:tab} {darkgray:mark} {darkgray:│} {cyan,bold:ctrl-p} {darkgray:preview}",
        );
        let text = |l: &Line| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        };
        assert_eq!(text(&fit_line(line.clone(), 100)), text(&line));
        // 30 columns reach "ctrl-p" but not its label: the whole "ctrl-p preview" hint goes.
        assert_eq!(
            text(&fit_line(line.clone(), 30)),
            " enter accept │ tab mark"
        );
        assert_eq!(text(&fit_line(line.clone(), 3)), "");
        // Without separators, whole spans are dropped.
        let plain =
            crate::ui::StatusUI::parse_template_to_status_line("{cyan:one} {cyan:two} three");
        assert_eq!(text(&fit_line(plain, 9)), "one two");
    }

    #[test]
    fn test_set_styled_parses_the_template_like_the_config_content() {
        let mut ui = DisplayUI::new(DisplayConfig::default());
        ui.set_styled(" {cyan,bold:C-h/l} {darkgray:Effort} {white,bold:◑ medium}");
        assert!(ui.show);
        let line = &ui.text[0].lines[0];
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, " C-h/l Effort ◑ medium");
        let key = line.spans.iter().find(|s| s.content == "C-h/l").unwrap();
        assert_eq!(key.style.fg, Some(Color::Cyan));
        assert!(key.style.add_modifier.contains(Modifier::BOLD));
    }
}
