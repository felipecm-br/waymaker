use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    config::RowConnectionStyle,
    ui::results::ResultsUI,
    utils::{string::substitute_escaped, text::expand_indents},
};

impl ResultsUI {
    pub fn make_status(&self, full_width: u16) -> Paragraph<'_> {
        let status_config = &self.status_config;
        let replacements = [
            ('r', self.index().to_string()),
            ('m', self.status.matched_count.to_string()),
            ('t', self.status.item_count.to_string()),
        ];

        // sub replacements into line
        let mut new_spans = Vec::new();

        if status_config.match_indent {
            new_spans.push(Span::raw(" ".repeat(self.indentation())));
        }

        for span in &self.status_template {
            let subbed = substitute_escaped(&span.content, &replacements);
            new_spans.push(Span::styled(subbed, span.style));
        }

        let substituted_line = Line::from(new_spans);

        // sub whitespace expansions
        let effective_width = match self.status_config.row_connection {
            RowConnectionStyle::Full => full_width,
            _ => self.width,
        } as usize;
        let expanded = expand_indents(substituted_line, r"\s", r"\S", effective_width)
            .style(status_config.style);

        Paragraph::new(expanded)
    }

    /// Returns just the substituted status spans as a `Line`, without width
    /// expansion or indentation — suitable for embedding inline in the input bar.
    pub fn status_line(&self) -> Line<'_> {
        let replacements = [
            ('r', self.index().to_string()),
            ('m', self.status.matched_count.to_string()),
            ('t', self.status.item_count.to_string()),
        ];

        let spans: Vec<Span<'_>> = self
            .status_template
            .iter()
            .map(|span| {
                let subbed = substitute_escaped(&span.content, &replacements);
                Span::styled(subbed, span.style)
            })
            .collect();

        Line::from(spans).style(self.status_config.style)
    }

    /// The style from the config overrides the Line style (but not the span styles).
    /// None restores the prompt defined in the config.
    pub fn set_status_line(&mut self, template: Option<Line<'static>>) {
        let status_config = &self.status_config;
        log::trace!("status line: {template:?}");

        self.status_template = template
            .unwrap_or(status_config.template.clone().into())
            .style(status_config.style)
    }
}

pub struct StatusUI {}

impl StatusUI {
    pub fn parse_template_to_status_line(s: &str) -> Line<'static> {
        let parts = match split_on_nesting(s, ['{', '}']) {
            Ok(x) => x,
            Err(n) => {
                if n > 0 {
                    log::error!("Encountered {} unclosed parentheses", n)
                } else {
                    log::error!("Extra closing parenthesis at index {}", -n)
                }
                return Line::from(s.to_string());
            }
        };

        let mut spans = Vec::new();
        let mut current_style = Style::default();

        for part in parts {
            let content = part.as_str();

            // Groups and text need not alternate ("{bold}{red:x}"), but only a
            // group chunk starts with an unescaped '{'.
            if content.starts_with('{') {
                let inner = &content[1..content.len() - 1];

                if inner.contains(':') {
                    // perform replacement style:content
                    spans.push(Self::span_from_template(inner));
                } else if inner.trim().eq_ignore_ascii_case("reset") {
                    current_style = Style::default();
                } else if let Some(style) = Self::parse_style(inner) {
                    current_style = style;
                } else {
                    spans.push(Span::styled(content.to_string(), current_style));
                }
            } else if !content.is_empty() {
                spans.push(Span::styled(content.to_string(), current_style));
            }
        }

        Line::from(spans)
    }

    /// Parses a comma-separated style specification (e.g. "red,bold" or "bg=blue,italic") into a `Style`.
    pub fn parse_style(style_part: &str) -> Option<Style> {
        use std::str::FromStr;

        let mut style = Style::default();
        let mut fg_set = false;
        let mut bg_set = false;
        let mut matched_any = false;

        for token in style_part.split(',') {
            let token = token.trim();
            if token.is_empty() {
                continue;
            }

            if token.eq_ignore_ascii_case("reset") {
                return Some(Style::default());
            }

            if let Some(bg_token) = token
                .strip_prefix("bg=")
                .or_else(|| token.strip_prefix("bg:"))
                && let Ok(color) = Color::from_str(bg_token)
            {
                style = style.bg(color);
                bg_set = true;
                matched_any = true;
                continue;
            }

            if let Some(fg_token) = token
                .strip_prefix("fg=")
                .or_else(|| token.strip_prefix("fg:"))
                && let Ok(color) = Color::from_str(fg_token)
            {
                style = style.fg(color);
                fg_set = true;
                matched_any = true;
                continue;
            }

            if !fg_set && let Ok(color) = Color::from_str(token) {
                style = style.fg(color);
                fg_set = true;
                matched_any = true;
                continue;
            }

            if !bg_set && let Ok(color) = Color::from_str(token) {
                style = style.bg(color);
                bg_set = true;
                matched_any = true;
                continue;
            }

            match token.to_lowercase().as_str() {
                "bold" => {
                    style = style.add_modifier(Modifier::BOLD);
                    matched_any = true;
                }
                "dim" => {
                    style = style.add_modifier(Modifier::DIM);
                    matched_any = true;
                }
                "italic" => {
                    style = style.add_modifier(Modifier::ITALIC);
                    matched_any = true;
                }
                "underlined" => {
                    style = style.add_modifier(Modifier::UNDERLINED);
                    matched_any = true;
                }
                "slow_blink" => {
                    style = style.add_modifier(Modifier::SLOW_BLINK);
                    matched_any = true;
                }
                "rapid_blink" => {
                    style = style.add_modifier(Modifier::RAPID_BLINK);
                    matched_any = true;
                }
                "reversed" => {
                    style = style.add_modifier(Modifier::REVERSED);
                    matched_any = true;
                }
                "hidden" => {
                    style = style.add_modifier(Modifier::HIDDEN);
                    matched_any = true;
                }
                "crossed_out" => {
                    style = style.add_modifier(Modifier::CROSSED_OUT);
                    matched_any = true;
                }
                _ => return None,
            };
        }

        if matched_any { Some(style) } else { None }
    }

    /// Converts a template string into a `Span` with colors and modifiers.
    ///
    /// The template string format is:
    /// ```text
    /// "style1,style2,...:text"
    /// ```
    /// - The **first valid color** token is used as foreground (fg).
    /// - The **second valid color** token is used as background (bg).
    /// - Remaining tokens are interpreted as **modifiers**: bold, dim, italic, underlined,
    ///   slow_blink, rapid_blink, reversed, hidden, crossed_out.
    /// - Empty tokens are ignored.
    /// - Unrecognized tokens are ignored.
    ///
    /// # Examples
    ///
    /// ```
    /// use waymaker::ui::StatusUI;
    /// StatusUI::span_from_template("red,bg=blue,bold,italic:Hello");
    /// StatusUI::span_from_template("green,,underlined:World");
    /// StatusUI::span_from_template(",,dim:OnlyDim");
    /// ```
    ///
    /// Returns a `Span` with the specified styles applied to the text.
    pub fn span_from_template(inner: &str) -> Span<'static> {
        let (style_part, text) = inner.split_once(':').unwrap_or(("", inner));
        let style = Self::parse_style(style_part).unwrap_or_default();
        Span::styled(text.to_string(), style)
    }
}

/// Split text on entering and exiting nesting level 1, keeping the brackets.
/// `\` before a bracket escapes it.
///
/// Replaces `cba::bring::split::split_on_nesting`, which indexed the input
/// with char counts as byte offsets and panicked on non-ASCII text before a `\`.
/// Errors are the unclosed depth (positive) or the negated byte offset of an
/// extra closing bracket.
fn split_on_nesting(input: &str, [open, close]: [char; 2]) -> Result<Vec<String>, i32> {
    let mut results = Vec::new();
    let mut current_chunk = String::new();
    let mut level = 0;
    let mut chars = input.char_indices().peekable();

    while let Some((i, c)) = chars.next() {
        if c == '\\'
            && let Some(&(_, next)) = chars.peek()
            && (next == open || next == close)
        {
            current_chunk.push(c);
            current_chunk.push(next);
            chars.next();
            continue;
        }

        if c == open {
            if level == 0 && !current_chunk.is_empty() {
                results.push(std::mem::take(&mut current_chunk));
            }
            level += 1;
        } else if c == close {
            level -= 1;
            if level < 0 {
                return Err(-(i as i32));
            }
        }

        current_chunk.push(c);

        // Split after pushing the closing bracket if we are back at ground level
        if c == close && level == 0 {
            results.push(std::mem::take(&mut current_chunk));
        }
    }

    if level > 0 {
        return Err(level);
    }

    if !current_chunk.is_empty() {
        results.push(current_chunk);
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adjacent_template_groups() {
        let line = StatusUI::parse_template_to_status_line("{bold}{red:x} tail");
        let spans: Vec<_> = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(spans, ["x", " tail"]);
        assert_eq!(line.spans[0].style.fg, Some(Color::Red));
        assert!(line.spans[1].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_split_on_nesting() {
        let b = ['{', '}'];
        assert_eq!(
            split_on_nesting("a {b} c{d{e}}", b).unwrap(),
            ["a ", "{b}", " c", "{d{e}}"]
        );
        assert_eq!(split_on_nesting(r"x \{y\} z", b).unwrap(), [r"x \{y\} z"]);
        assert_eq!(split_on_nesting("{a", b), Err(1));
        assert_eq!(split_on_nesting("abc}", b), Err(-3));
    }

    #[test]
    fn test_split_on_nesting_non_ascii_before_escape() {
        let b = ['{', '}'];
        assert_eq!(split_on_nesting("\u{800}\\", b).unwrap(), ["\u{800}\\"]);
        assert_eq!(
            split_on_nesting("\u{f07b} \\{x\\} {red:y}", b).unwrap(),
            ["\u{f07b} \\{x\\} ", "{red:y}"]
        );
    }

    #[test]
    fn test_status_ui_parse_style() {
        assert_eq!(StatusUI::parse_style("reset"), Some(Style::default()));
        assert_eq!(StatusUI::parse_style(""), None);
        assert_eq!(StatusUI::parse_style("unknown_token"), None);

        let style = StatusUI::parse_style("red,bg=blue,bold,italic").unwrap();
        assert_eq!(style.fg, Some(Color::Red));
        assert_eq!(style.bg, Some(Color::Blue));
        assert!(style.add_modifier.contains(Modifier::BOLD));
        assert!(style.add_modifier.contains(Modifier::ITALIC));

        let style2 = StatusUI::parse_style("fg:green,bg:yellow,dim,underlined").unwrap();
        assert_eq!(style2.fg, Some(Color::Green));
        assert_eq!(style2.bg, Some(Color::Yellow));
        assert!(style2.add_modifier.contains(Modifier::DIM));
        assert!(style2.add_modifier.contains(Modifier::UNDERLINED));

        let style_blinks =
            StatusUI::parse_style("slow_blink,rapid_blink,reversed,hidden,crossed_out").unwrap();
        assert!(style_blinks.add_modifier.contains(Modifier::SLOW_BLINK));
        assert!(style_blinks.add_modifier.contains(Modifier::RAPID_BLINK));
        assert!(style_blinks.add_modifier.contains(Modifier::REVERSED));
        assert!(style_blinks.add_modifier.contains(Modifier::HIDDEN));
        assert!(style_blinks.add_modifier.contains(Modifier::CROSSED_OUT));
    }

    #[test]
    fn test_status_ui_span_from_template() {
        let span = StatusUI::span_from_template("cyan,bold:Prompt");
        assert_eq!(span.content, "Prompt");
        assert_eq!(span.style.fg, Some(Color::Cyan));
        assert!(span.style.add_modifier.contains(Modifier::BOLD));

        let plain = StatusUI::span_from_template("NoColonHere");
        assert_eq!(plain.content, "NoColonHere");
        assert_eq!(plain.style, Style::default());

        let invalid = StatusUI::span_from_template("bad_style:Sample");
        assert_eq!(invalid.content, "Sample");
        assert_eq!(invalid.style, Style::default());
    }
}
