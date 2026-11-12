use std::borrow::Cow;

use cba::define_either;

use crate::nucleo::SharedStr;
pub use ratatui::text::Text;

define_either! {
    #[derive(serde::Serialize, serde::Deserialize)]
    #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
    pub enum Either<L, R = L> {
        Left,
        Right
    }
}

/// `text.to_string().len()`, without building the string: lines are joined by `\n`, and a
/// span is shown without the line breaks in its content (see ratatui's `Display` impls).
pub fn text_byte_len(text: &Text<'_>) -> usize {
    let spans: usize = text
        .lines
        .iter()
        .flat_map(|line| &line.spans)
        .map(|span| span.content.lines().map(str::len).sum::<usize>())
        .sum();
    spans + text.lines.len().saturating_sub(1)
}

impl Either<SharedStr, Box<Text<'static>>> {
    #[inline]
    pub fn byte_len(&self) -> usize {
        match self {
            Either::Left(s) => s.len(),
            Either::Right(t) => text_byte_len(t),
        }
    }

    pub fn to_cow(&self) -> Cow<'_, str> {
        match self {
            Either::Left(s) => Cow::Borrowed(s),
            Either::Right(t) => Cow::Owned(t.to_string()),
        }
    }

    pub fn to_text(self) -> Text<'static> {
        match self {
            Either::Left(s) => Text::from(s.to_string()),
            Either::Right(t) => *t,
        }
    }

    pub fn as_text(&self) -> Text<'_> {
        match self {
            Either::Left(s) => Text::from(s.as_ref()),
            Either::Right(t) => (**t).clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_byte_len_matches_to_string() {
        use ratatui::text::{Line, Span};
        let texts = [
            Text::default(),
            Text::from(""),
            Text::from("plain"),
            Text::from("two\nlines"),
            Text::from(vec![Line::from(""), Line::from("")]),
            Text::from(Line::from(vec![Span::raw("a\nb"), Span::raw("c\r\nd\n")])),
            Text::from(vec![
                Line::from(vec![Span::raw("ü"), Span::raw("→x")]),
                Line::default(),
                Line::from(Span::raw("\n")),
            ]),
        ];
        for text in texts {
            assert_eq!(text_byte_len(&text), text.to_string().len(), "{text:?}");
        }
    }

    #[test]
    fn test_either_left_and_right() {
        assert_eq!(
            std::mem::size_of::<Either<SharedStr, Box<Text<'static>>>>(),
            16
        );

        let left: Either<SharedStr, Box<Text<'static>>> = Either::Left("sample left".into());
        assert_eq!(left.to_cow(), Cow::Borrowed("sample left"));
        assert_eq!(left.as_text(), Text::from("sample left"));
        assert_eq!(left.to_text(), Text::from("sample left"));

        let right: Either<SharedStr, Box<Text<'static>>> =
            Either::Right(Box::new(Text::from("sample right")));
        assert_eq!(right.to_cow(), Cow::Borrowed("sample right"));
        assert_eq!(right.as_text(), Text::from("sample right"));
        assert_eq!(right.to_text(), Text::from("sample right"));
    }
}
