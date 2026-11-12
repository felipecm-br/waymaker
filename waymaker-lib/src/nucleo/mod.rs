pub mod injector;
pub mod query;
pub mod variants;
mod worker;

use std::{
    borrow::Cow,
    fmt::{self, Display, Formatter},
    hash::{Hash, Hasher},
    ops::Range,
    sync::Arc,
};

use crate::SSS;
pub use variants::*;
pub use worker::*;

pub use nucleo;
pub use nucleo::{SharedChunk, SharedStr};
pub use ratatui::prelude::*;

// ------------- Wrapper structs
pub trait SegmentableItem: SSS {
    fn slice(&self, range: Range<usize>) -> ratatui::text::Text<'_>;
    fn slice_str(&self, range: Range<usize>) -> Cow<'_, str> {
        self.slice(range).to_string().into()
    }
    /// The item's whole text, when it is held in a [`SharedStr`] that the matcher can share
    /// instead of copying (see [`ColumnIndexable::get_shared`]).
    fn shared(&self) -> Option<&SharedStr> {
        None
    }
    /// Byte length of the full item when known cheaply without splitting.
    #[inline]
    fn byte_len(&self) -> Option<usize> {
        self.shared().map(|s| s.len())
    }
}

impl SegmentableItem for String {
    fn slice(&self, range: Range<usize>) -> ratatui::text::Text<'_> {
        ratatui::text::Text::from(&self[range])
    }
    fn slice_str(&self, range: Range<usize>) -> Cow<'_, str> {
        (&self[range]).into()
    }
    #[inline]
    fn byte_len(&self) -> Option<usize> {
        Some(self.len())
    }
}

/// The byte range of each column of an item.
///
/// Every item carries one, so at millions of items each word matters: this is one pointer
/// (8 bytes). An item that is one column spanning its whole text, the common case, stores
/// nothing ([`ColumnRanges::Whole`]); [`Segmented`] works out that range from the item's
/// length. Any other ranges are boxed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ColumnRanges {
    /// One column, the item's whole text. Only for items whose
    /// [`SegmentableItem::byte_len`] is known; otherwise the column reads as empty.
    Whole,
    /// Explicit ranges, behind a thin pointer so the type stays one word.
    Listed(Box<Box<[(u32, u32)]>>),
}

const _: () = assert!(std::mem::size_of::<ColumnRanges>() == 8);

impl ColumnRanges {
    #[inline]
    pub fn single(start: u32, end: u32) -> Self {
        Self::from(&[(start, end)][..])
    }

    #[inline]
    pub fn pair(first: (u32, u32), second: (u32, u32)) -> Self {
        Self::from(&[first, second][..])
    }

    #[inline]
    pub fn from_array_vec(arr: arrayvec::ArrayVec<(u32, u32), 2>) -> Self {
        Self::from(arr.as_slice())
    }

    #[inline]
    pub fn multi(ranges: Box<[(u32, u32)]>) -> Self {
        Self::from(ranges)
    }

    /// The ranges, given the byte length of the whole item (`None` if unknown).
    #[inline]
    pub fn resolve(&self, whole_len: Option<usize>) -> std::borrow::Cow<'_, [(u32, u32)]> {
        match self {
            Self::Whole => match whole_len {
                Some(len) => std::borrow::Cow::Owned(vec![(0, len as u32)]),
                None => std::borrow::Cow::Borrowed(&[]),
            },
            Self::Listed(ranges) => std::borrow::Cow::Borrowed(ranges),
        }
    }

    /// Column `i`, given the byte length of the whole item (`None` if unknown).
    #[inline]
    pub fn get(&self, i: usize, whole_len: Option<usize>) -> Option<(u32, u32)> {
        match self {
            Self::Whole => whole_len.filter(|_| i == 0).map(|len| (0, len as u32)),
            Self::Listed(ranges) => ranges.get(i).copied(),
        }
    }
}

impl From<&[(u32, u32)]> for ColumnRanges {
    #[inline]
    fn from(ranges: &[(u32, u32)]) -> Self {
        Self::Listed(Box::new(ranges.into()))
    }
}

impl From<arrayvec::ArrayVec<(u32, u32), 2>> for ColumnRanges {
    #[inline]
    fn from(arr: arrayvec::ArrayVec<(u32, u32), 2>) -> Self {
        Self::from(arr.as_slice())
    }
}

impl From<Box<[(u32, u32)]>> for ColumnRanges {
    #[inline]
    fn from(b: Box<[(u32, u32)]>) -> Self {
        Self::Listed(Box::new(b))
    }
}

impl From<Vec<(u32, u32)>> for ColumnRanges {
    #[inline]
    fn from(v: Vec<(u32, u32)>) -> Self {
        Self::from(v.into_boxed_slice())
    }
}

/// The label of a run of items (`--group-prefix` lines). A thin pointer, so an item pays 8
/// bytes for it rather than 16; the items of one group share one allocation.
pub type GroupLabel = Arc<Arc<str>>;

/// Collects the texts of a batch of items into one buffer, so the whole batch becomes one
/// [`SharedChunk`] instead of one allocation per item.
#[derive(Default)]
pub struct ChunkBuilder {
    text: String,
    items: Vec<(Option<GroupLabel>, usize, usize)>,
}

impl ChunkBuilder {
    pub fn with_capacity(items: usize) -> Self {
        Self {
            text: String::new(),
            items: Vec::with_capacity(items),
        }
    }

    pub fn push(&mut self, group: Option<GroupLabel>, text: &str) {
        let start = self.text.len();
        self.text.push_str(text);
        self.items.push((group, start, self.text.len()));
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The batch's items, each a slice of one new chunk. Leaves the builder empty.
    pub fn take(
        &mut self,
    ) -> impl ExactSizeIterator<Item = (Option<GroupLabel>, SharedStr)> + use<> {
        let chunk = SharedChunk::new(std::mem::take(&mut self.text));
        std::mem::take(&mut self.items)
            .into_iter()
            .map(move |(group, start, end)| (group, chunk.slice(start..end)))
    }
}

/// This struct implements ColumnIndexable, and can instantiate a worker with columns.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct Segmented<T> {
    pub inner: T,
    pub ranges: ColumnRanges,
    pub group: Option<GroupLabel>,
}

impl<T: SegmentableItem + std::fmt::Debug> ColumnIndexable for Segmented<T> {
    fn get_str(&self, i: usize) -> std::borrow::Cow<'_, str> {
        if let Some((start, end)) = self.range(i) {
            self.inner.slice_str(start as usize..end as usize)
        } else {
            "".into()
        }
    }

    fn get_text(&self, i: usize) -> Text<'_> {
        if let Some((start, end)) = self.range(i) {
            self.inner.slice(start as usize..end as usize)
        } else {
            Text::default()
        }
    }

    #[inline]
    fn get_shared(&self, i: usize) -> Option<SharedStr> {
        let text = self.inner.shared()?;
        match self.range(i) {
            Some((0, end)) if end as usize == text.len() => Some(text.clone()),
            // A column is a slice of the same chunk, so it is shared too.
            Some((start, end)) => {
                let range = start as usize..end as usize;
                text.get(range.clone()).is_some().then(|| text.slice(range))
            }
            _ => None,
        }
    }
}

impl<T: SegmentableItem> Segmented<T> {
    #[inline]
    pub fn new(inner: T, ranges: impl Into<ColumnRanges>, group: Option<GroupLabel>) -> Self {
        Self {
            inner,
            ranges: ranges.into(),
            group,
        }
    }

    pub fn from_ranges(inner: T, ranges: impl IntoIterator<Item = (usize, usize)>) -> Self {
        let ranges: Vec<(u32, u32)> = ranges
            .into_iter()
            .map(|(s, e)| (s as u32, e as u32))
            .collect();
        Self {
            inner,
            ranges: ranges.into(),
            group: None,
        }
    }

    /// Byte range of column `i`.
    #[inline]
    pub fn range(&self, i: usize) -> Option<(u32, u32)> {
        self.ranges.get(i, self.inner.byte_len())
    }

    pub fn len(&self) -> usize {
        // Find the last range that is nonempty (start != end)
        self.ranges
            .resolve(self.inner.byte_len())
            .iter()
            .rposition(|&(start, end)| start != end)
            .map_or(0, |idx| idx + 1)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn map_to_vec<U, F>(&self, f: F) -> Vec<U>
    where
        F: Fn(&T, usize, usize) -> U,
    {
        self.ranges
            .resolve(self.inner.byte_len())
            .iter()
            .take(self.len()) // only map the "active" ranges
            .map(|&(start, end)| f(&self.inner, start as usize, end as usize))
            .collect()
    }
}

impl<T> std::ops::Deref for Segmented<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

// ------------------------------------------------

#[derive(Debug, Clone)]
pub struct Indexed<T> {
    pub index: u32,
    pub inner: T,
}

impl<T: Clone> Indexed<T> {
    /// Matchmaker requires a way to identify and store selected items from their references in the nucleo matcher. This method simply identifies them by their insertion index and stores the clones of the items.
    pub fn identifier(&self) -> (u32, T) {
        (self.index, self.inner.clone())
    }
}

impl<T> Indexed<T> {
    /// Matchmaker requires a way to identify and store selected items from their references in the nucleo matcher. This method simply identifies them by their insertion index and is intended when the output type is not needed (i.e. externally managed).
    pub fn dummy_identifier(&self) -> (u32, ()) {
        (self.index, ())
    }
}

impl<T: ColumnIndexable> ColumnIndexable for Indexed<T> {
    fn get_str(&self, index: usize) -> Cow<'_, str> {
        self.inner.get_str(index)
    }

    fn get_text(&self, i: usize) -> Text<'_> {
        self.inner.get_text(i)
    }

    #[inline]
    fn get_shared(&self, i: usize) -> Option<SharedStr> {
        self.inner.get_shared(i)
    }
}

impl<T: Render> Render for Indexed<T> {
    fn as_str(&self) -> Cow<'_, str> {
        self.inner.as_str()
    }
    fn as_text(&self) -> Text<'_> {
        self.inner.as_text()
    }
}

impl<T> std::ops::Deref for Indexed<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

// ------------------------------------------
impl<T: Display + SegmentableItem> Display for Segmented<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl<T: Display> Display for Indexed<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl<T> PartialEq for Indexed<T> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}

impl<T> Eq for Indexed<T> {}

impl<T> Hash for Indexed<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.index.hash(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_column_ranges_listed() {
        let none = None;
        let cr = ColumnRanges::single(0, 10);
        assert_eq!(cr.resolve(none).as_ref(), &[(0, 10)]);
        assert_eq!(cr.get(0, none), Some((0, 10)));
        assert_eq!(cr.get(1, none), None);

        let cr = ColumnRanges::pair((0, 5), (6, 12));
        assert_eq!(cr.resolve(none).as_ref(), &[(0, 5), (6, 12)]);
        assert_eq!(cr.get(1, none), Some((6, 12)));

        let cr = ColumnRanges::from(vec![(0, 2), (3, 5), (6, 9)]);
        assert_eq!(cr.resolve(none).as_ref(), &[(0, 2), (3, 5), (6, 9)]);
    }

    #[test]
    fn test_column_ranges_equal_by_content() {
        assert_eq!(
            ColumnRanges::from(vec![(0, 10)]),
            ColumnRanges::single(0, 10)
        );
        assert_eq!(
            ColumnRanges::multi(Box::new([(0, 10)])),
            ColumnRanges::single(0, 10)
        );
        assert_ne!(ColumnRanges::single(0, 10), ColumnRanges::single(0, 9));
        assert!(ColumnRanges::from(Vec::new()).resolve(None).is_empty());
    }

    #[test]
    fn test_column_ranges_whole() {
        let cr = ColumnRanges::Whole;
        assert_eq!(cr.resolve(Some(7)).as_ref(), &[(0, 7)]);
        assert_eq!(cr.get(0, Some(7)), Some((0, 7)));
        assert_eq!(cr.get(1, Some(7)), None);
        // Without a known length there is no column to show.
        assert!(cr.resolve(None).is_empty());
        assert_eq!(cr.get(0, None), None);
    }

    /// The point of [`ColumnRanges::Whole`]: an item of the default picker is 40 bytes.
    #[test]
    fn test_config_item_size() {
        assert_eq!(std::mem::size_of::<crate::ConfigMMInnerItem>(), 32);
        assert_eq!(std::mem::size_of::<crate::ConfigMMItem>(), 40);
    }

    #[test]
    fn test_segmented_whole_reads_like_its_full_range() {
        for text in ["hello world", ""] {
            let whole = Segmented::new(text.to_string(), ColumnRanges::Whole, None);
            let listed = Segmented::new(
                text.to_string(),
                ColumnRanges::single(0, text.len() as u32),
                None,
            );
            assert_eq!(whole.get_str(0), listed.get_str(0));
            assert_eq!(whole.get_str(1), listed.get_str(1));
            assert_eq!(whole.get_text(0), listed.get_text(0));
            assert_eq!(whole.len(), listed.len());
            assert_eq!(whole.is_empty(), listed.is_empty());
            let cols = |s: &Segmented<String>| s.map_to_vec(|t, a, b| t[a..b].to_string());
            assert_eq!(cols(&whole), cols(&listed));
        }
        let shared = SharedStr::from("abc");
        let whole = Segmented::new(
            crate::nucleo::injector::Either::<SharedStr, Box<Text<'static>>>::Left(shared.clone()),
            ColumnRanges::Whole,
            None,
        );
        assert_eq!(whole.get_shared(0).as_deref(), Some("abc"));
        assert_eq!(whole.get_shared(1), None);
    }

    #[test]
    fn test_segmented_with_column_ranges() {
        let seg = Segmented::new("hello world".to_string(), ColumnRanges::single(0, 5), None);
        assert_eq!(seg.get_str(0), "hello");
        assert_eq!(seg.get_str(1), "");
        assert_eq!(seg.len(), 1);
        assert!(!seg.is_empty());
    }
}
