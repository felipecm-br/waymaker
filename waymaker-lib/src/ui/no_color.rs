//! Monochrome rendering for <https://no-color.org>.
//!
//! crossterm already drops colour escapes when `NO_COLOR` is set, which also
//! erases every highlight drawn only with a background (the cursor row,
//! selections, badges). [`strip_colors`] turns those backgrounds into reverse
//! video first so they stay visible.

use std::sync::LazyLock;

use ratatui::{
    buffer::Buffer,
    style::{Color, Modifier},
};

static NO_COLOR: LazyLock<bool> =
    LazyLock::new(|| std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()));

/// Whether a non-empty `NO_COLOR` is set in the environment.
pub fn no_color() -> bool {
    *NO_COLOR
}

/// Drop foreground and background colours from every cell, keeping modifiers.
/// A cell that had a background is drawn reversed instead.
pub fn strip_colors(buf: &mut Buffer) {
    for cell in buf.content.iter_mut() {
        if cell.bg != Color::Reset {
            cell.modifier.insert(Modifier::REVERSED);
        }
        cell.fg = Color::Reset;
        cell.bg = Color::Reset;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    #[test]
    fn strips_colors_and_keeps_highlights() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 2, 1));
        buf[(0, 0)].set_symbol("a").set_fg(Color::Red);
        buf[(1, 0)]
            .set_symbol("b")
            .set_bg(Color::Blue)
            .modifier
            .insert(Modifier::BOLD);

        strip_colors(&mut buf);

        assert_eq!(buf[(0, 0)].fg, Color::Reset);
        assert!(!buf[(0, 0)].modifier.contains(Modifier::REVERSED));

        assert_eq!(buf[(1, 0)].bg, Color::Reset);
        assert!(
            buf[(1, 0)]
                .modifier
                .contains(Modifier::REVERSED | Modifier::BOLD)
        );
    }
}
