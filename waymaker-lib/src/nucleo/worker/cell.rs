use std::mem::take;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{
    SSS,
    config::AutoscrollSettings,
    nucleo::{Line, Span, Style, Text},
    utils::text::wrapping_indicator,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn render_cell<T: SSS>(
    cell: Text<'_>,
    col_idx: usize,
    snapshot: &nucleo::Snapshot<T>,
    item: &nucleo::Item<T>,
    matcher: &mut nucleo::Matcher,
    highlight_style: Style,
    wrap: bool,
    width_limit: u16,
    col_indices_buffer: &mut Vec<u32>,
    mut autoscroll: AutoscrollSettings,
    hscroll_offset: i8,
) -> (Text<'static>, usize) {
    if !autoscroll.always {
        autoscroll.enabled &= !wrap;
    }

    let mut cell_width = 0;
    let mut wrapped = false;

    // get indices
    let indices_buffer = col_indices_buffer;
    indices_buffer.clear();
    snapshot.pattern().column_pattern(col_idx).indices(
        item.matcher_columns[col_idx].slice(..),
        matcher,
        indices_buffer,
    );
    indices_buffer.sort_unstable();
    indices_buffer.dedup();
    let mut indices = indices_buffer.drain(..);

    let mut lines = vec![];
    let mut next_highlight_idx = indices.next().unwrap_or(u32::MAX);
    let mut grapheme_idx = 0u32;

    let mut line_graphemes = Vec::new();

    for line in &cell {
        // 1: Collect graphemes, compute styles, and find the relevant match on this line.
        line_graphemes.clear();
        let mut match_idx = None;

        for span in line {
            // this looks like a bug on first glance, we are iterating
            // graphemes but treating them as char indices. The reason that
            // this is correct is that nucleo will only ever consider the first char
            // of a grapheme (and discard the rest of the grapheme) so the indices
            // returned by nucleo are essentially grapheme indecies
            for grapheme in span.content.graphemes(true) {
                let is_match = grapheme_idx == next_highlight_idx;

                let style = if is_match {
                    next_highlight_idx = indices.next().unwrap_or(u32::MAX);
                    span.style.patch(highlight_style)
                } else {
                    span.style
                };

                if is_match && (autoscroll.end || match_idx.is_none()) {
                    match_idx = Some(line_graphemes.len());
                }

                line_graphemes.push((grapheme, style));
                grapheme_idx += 1;
            }
        }

        // 2: Calculate where to start rendering this line
        let i = autoscroll_start_idx(
            &line_graphemes,
            match_idx,
            &autoscroll,
            width_limit,
            hscroll_offset,
        );

        // 3: Apply the standard wrapping and Span generation logic to the visible slice
        (cell_width, wrapped) = render_visible_slice(
            &mut line_graphemes,
            i,
            &autoscroll,
            width_limit,
            wrap,
            &mut lines,
            cell_width,
            wrapped,
        );

        grapheme_idx += 1; // newline
    }

    (
        Text::from(lines),
        if wrapped {
            width_limit as usize
        } else {
            cell_width
        },
    )
}

fn autoscroll_start_idx(
    line_graphemes: &[(&str, Style)],
    match_idx: Option<usize>,
    autoscroll: &AutoscrollSettings,
    width_limit: u16,
    hscroll_offset: i8,
) -> usize {
    let mut i; // start_idx

    if autoscroll.enabled && autoscroll.end {
        i = match_idx.unwrap_or(line_graphemes.len().saturating_sub(1));

        let preserved_width = line_graphemes
            [..autoscroll.initial_preserved.min(line_graphemes.len())]
            .iter()
            .map(|(g, _)| g.width())
            .sum::<usize>();

        let target_width = if let Some(x) = match_idx {
            (width_limit as usize)
                .saturating_sub(autoscroll.context.min(line_graphemes.len() - x - 1))
        } else {
            width_limit as usize
        }
        .saturating_sub(preserved_width);

        let mut current_width = 0;

        while i > autoscroll.initial_preserved {
            let w = line_graphemes[i - 1].0.width();
            let indicator_width = if i - 1 > autoscroll.initial_preserved {
                autoscroll.indicator.as_str().width()
            } else {
                0
            };

            if current_width + w + indicator_width < target_width {
                i -= 1;
                current_width += w;
            } else {
                break;
            }
        }

        i = i.saturating_add_signed(hscroll_offset as isize);

        if i <= autoscroll.initial_preserved {
            i = 0;
        }
    } else if autoscroll.enabled
        && let Some(m_idx) = match_idx
    {
        i = (m_idx as i32 - autoscroll.context as i32).max(0) as usize;

        let mut tail_width: usize = line_graphemes[i..].iter().map(|(g, _)| g.width()).sum();

        let preserved_width = line_graphemes
            [..autoscroll.initial_preserved.min(line_graphemes.len())]
            .iter()
            .map(|(g, _)| g.width())
            .sum::<usize>();

        // Expand leftwards as long as the total rendered width <= width_limit
        while i > autoscroll.initial_preserved {
            let prev_width = line_graphemes[i - 1].0.width();
            // Only reserve space for "..." if we aren't reaching the very start
            let indicator_width = if i - 1 > autoscroll.initial_preserved {
                autoscroll.indicator.as_str().width()
            } else {
                0
            };

            if tail_width + preserved_width + indicator_width + prev_width <= width_limit as usize {
                i -= 1;
                tail_width += prev_width;
            } else {
                break;
            }
        }

        i = i.saturating_add_signed(hscroll_offset as isize);

        if i <= autoscroll.initial_preserved {
            i = 0;
        }
    } else {
        i = hscroll_offset.max(0) as usize;
    };
    i
}

#[allow(clippy::too_many_arguments)]
fn render_visible_slice(
    line_graphemes: &mut Vec<(&str, Style)>,
    mut i: usize,
    autoscroll: &AutoscrollSettings,
    width_limit: u16,
    wrap: bool,
    lines: &mut Vec<Line<'static>>,
    mut cell_width: usize,
    mut wrapped: bool,
) -> (usize, bool) {
    let mut current_spans = Vec::new();
    let mut current_span = String::new();
    let mut current_style = Style::default();
    let mut current_width = 0;

    // Add preserved prefix and ellipsis if needed
    if i > 0 && autoscroll.enabled {
        for (g, s) in line_graphemes.drain(..autoscroll.initial_preserved.min(line_graphemes.len()))
        {
            if s != current_style {
                if !current_span.is_empty() {
                    current_spans.push(Span::styled(current_span, current_style));
                }
                current_span = String::new();
                current_style = s;
            }
            current_span.push_str(g);
        }
        if !current_span.is_empty() {
            current_spans.push(Span::styled(current_span, current_style));
        }
        i -= autoscroll.initial_preserved;

        current_width += current_spans.iter().map(|x| x.width()).sum::<usize>();
        current_spans.push(Span::styled(
            autoscroll.indicator.clone(),
            Style::from(autoscroll.indicator_style),
        ));
        current_width += autoscroll.indicator.as_str().width();

        current_span = String::new();
        current_style = Style::default();
    }

    // prevent stuck invisible columns
    if !line_graphemes.is_empty() {
        cell_width = cell_width.max(1);
        i = i.min(line_graphemes.len())
    }

    let mut graphemes = line_graphemes.drain(i..);

    while let Some((mut grapheme, mut style)) = graphemes.next() {
        if current_width + grapheme.width() > width_limit as usize {
            if !current_span.is_empty() {
                current_spans.push(Span::styled(current_span, current_style));
                current_span = String::new();
            }
            if wrap {
                current_spans.push(wrapping_indicator());
                lines.push(Line::from(take(&mut current_spans)));

                current_width = 0;
                wrapped = true;
            } else {
                break;
            }
        } else if current_width + grapheme.width() == width_limit as usize {
            if wrap {
                let mut new = grapheme.to_string();
                if current_style != style {
                    current_spans.push(Span::styled(take(&mut current_span), current_style));
                    current_style = style;
                };
                for (grapheme2, style2) in graphemes.by_ref() {
                    if grapheme2.width() == 0 {
                        new.push_str(grapheme2);
                    } else {
                        if !current_span.is_empty() {
                            current_spans.push(Span::styled(current_span, current_style));
                        }
                        current_spans.push(wrapping_indicator());
                        lines.push(Line::from(take(&mut current_spans)));

                        // new line starts from last char
                        current_span = new.clone(); // rust can't tell that clone is unnecessary here
                        current_width = grapheme.width();
                        wrapped = true;

                        grapheme = grapheme2;
                        style = style2;
                        break; // continue normal processing
                    }
                }
                if !wrapped {
                    current_span.push_str(&new);
                    // we reached the end of the line exactly, end line
                    current_spans.push(Span::styled(take(&mut current_span), style));
                    current_style = style;
                    current_width += grapheme.width();
                    break;
                }
            } else {
                if style != current_style {
                    if !current_span.is_empty() {
                        current_spans.push(Span::styled(current_span, current_style));
                    }
                    current_span = String::new();
                    current_style = style;
                }
                current_span.push_str(grapheme);
                current_width += grapheme.width();
                break;
            }
        }

        // normal processing
        if style != current_style {
            if !current_span.is_empty() {
                current_spans.push(Span::styled(current_span, current_style))
            }
            current_span = String::new();
            current_style = style;
        }
        current_span.push_str(grapheme);
        current_width += grapheme.width();
    }

    current_spans.push(Span::styled(current_span, current_style));
    lines.push(Line::from(current_spans));
    cell_width = cell_width.max(current_width);
    (cell_width, wrapped)
}
