// Original code from https://github.com/helix-editor/helix (MPL 2.0)
// Modified by Squirreljetpack, 2025

use std::{
    marker::PhantomData,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

use super::worker::{Column, Worker, WorkerError};
use super::{ColumnRanges, Indexed, Segmented};
use crate::{SSS, nucleo::SegmentableItem};

pub trait Injector {
    type InputItem;
    type Inner: Injector;
    type Context;

    fn new(injector: Self::Inner, data: Self::Context) -> Self;
    fn inner(&self) -> &Self::Inner;
    fn wrap(
        &self,
        item: Self::InputItem,
    ) -> Result<<Self::Inner as Injector>::InputItem, WorkerError>;

    fn push(&self, item: Self::InputItem) -> Result<(), WorkerError> {
        let item = self.wrap(item)?;
        self.inner().push(item)
    }

    fn extend(
        &self,
        items: impl ExactSizeIterator<Item = Self::InputItem>,
    ) -> Result<(), WorkerError> {
        let items = items
            .map(|item| self.wrap(item))
            .collect::<Result<Vec<<<Self as Injector>::Inner as Injector>::InputItem>, WorkerError>>()?;
        self.inner().extend(items.into_iter())
    }
}

impl Injector for () {
    fn inner(&self) -> &Self::Inner {
        unreachable!()
    }
    fn new(_: Self::Inner, _: Self::Context) -> Self {
        unreachable!()
    }
    fn wrap(
        &self,
        _: Self::InputItem,
    ) -> Result<<Self::Inner as Injector>::InputItem, WorkerError> {
        unreachable!()
    }

    type Context = ();
    type Inner = ();
    type InputItem = ();
}

pub struct WorkerInjector<T> {
    pub(super) inner: nucleo::Injector<T>,
    pub(super) columns: Arc<[Column<T>]>,
    pub(super) version: u32,
    pub(super) picker_version: Arc<AtomicU32>,
}

impl<T: SSS> Injector for WorkerInjector<T> {
    type InputItem = T;
    type Inner = ();
    type Context = Worker<T>;

    fn new(_: Self::Inner, data: Self::Context) -> Self {
        data.injector()
    }

    fn inner(&self) -> &Self::Inner {
        &()
    }

    fn wrap(
        &self,
        _: Self::InputItem,
    ) -> Result<<Self::Inner as Injector>::InputItem, WorkerError> {
        Ok(())
    }

    fn push(&self, item: T) -> Result<(), WorkerError> {
        if self.version != self.picker_version.load(Ordering::Relaxed) {
            return Err(WorkerError::InjectorShutdown);
        }
        push_impl(&self.inner, &self.columns, item);
        Ok(())
    }

    fn extend(&self, items: impl ExactSizeIterator<Item = T>) -> Result<(), WorkerError> {
        if self.version != self.picker_version.load(Ordering::Relaxed) {
            return Err(WorkerError::InjectorShutdown);
        }
        extend_impl(&self.inner, &self.columns, items);
        Ok(())
    }
}

pub(super) fn push_impl<T>(injector: &nucleo::Injector<T>, columns: &[Column<T>], item: T) {
    if let [col] = columns
        && col.filter
    {
        injector.push(item, |item, dst| {
            if let Some(text) = dst.first_mut() {
                *text = col.haystack(item);
            }
        });
    } else {
        injector.push(item, |item, dst| {
            for (column, text) in columns.iter().filter(|column| column.filter).zip(dst) {
                *text = column.haystack(item)
            }
        });
    }
}

pub(super) fn extend_impl<T, I>(injector: &nucleo::Injector<T>, columns: &[Column<T>], items: I)
where
    I: ExactSizeIterator<Item = T>,
{
    if let [col] = columns
        && col.filter
    {
        injector.extend(items, |item, dst| {
            if let Some(text) = dst.first_mut() {
                *text = col.haystack(item);
            }
        });
    } else {
        injector.extend(items, |item, dst| {
            for (column, text) in columns.iter().filter(|column| column.filter).zip(dst) {
                *text = column.haystack(item)
            }
        });
    }
}

// ----- Injectors

/// Wraps the injected item with an atomic index which is incremented on push.
#[derive(Clone)]
pub struct IndexedInjector<T, I: Injector<InputItem = Indexed<T>>> {
    injector: I,
    counter: &'static AtomicU32,
    input_type: PhantomData<T>,
}

// note that invalidation can be handled
impl<T, I: Injector<InputItem = Indexed<T>>> Injector for IndexedInjector<T, I> {
    type InputItem = T;
    type Inner = I;
    type Context = &'static AtomicU32;

    fn new(injector: Self::Inner, counter: Self::Context) -> Self {
        Self {
            injector,
            counter,
            input_type: PhantomData,
        }
    }

    fn wrap(
        &self,
        item: Self::InputItem,
    ) -> Result<<Self::Inner as Injector>::InputItem, WorkerError> {
        let index = self.counter.fetch_add(1, Ordering::SeqCst);
        Ok(Indexed { index, inner: item })
    }

    fn inner(&self) -> &Self::Inner {
        &self.injector
    }

    fn extend(
        &self,
        items: impl ExactSizeIterator<Item = Self::InputItem>,
    ) -> Result<(), WorkerError> {
        let count = items.len();
        if count == 0 {
            return Ok(());
        }
        let start_index = self.counter.fetch_add(count as u32, Ordering::SeqCst);
        let wrapped = items.enumerate().map(|(offset, item)| Indexed {
            index: start_index + offset as u32,
            inner: item,
        });
        self.injector.extend(wrapped)
    }
}

static GLOBAL_COUNTER: AtomicU32 = AtomicU32::new(0);

impl<T, I> IndexedInjector<T, I>
where
    I: Injector<InputItem = Indexed<T>>,
{
    pub fn new_globally_indexed(injector: <Self as Injector>::Inner) -> Self {
        Self::global_reset();
        Self::new(injector, &GLOBAL_COUNTER)
    }

    pub fn global_reset() {
        GLOBAL_COUNTER.store(0, Ordering::SeqCst);
    }
}

// ------------------------------------------------------------------------------------------------
pub type SplitterFn<T> = std::sync::Arc<dyn for<'a> Fn(&'a T) -> ColumnRanges + Send + Sync>;

pub struct SegmentedInjector<T, I: Injector<InputItem = Segmented<T>>> {
    injector: I,
    splitter: SplitterFn<T>,
    single_column: bool,
}

impl<T: SegmentableItem, I: Injector<InputItem = Segmented<T>>> SegmentedInjector<T, I> {
    pub fn with_single_column(mut self, single_column: bool) -> Self {
        self.single_column = single_column;
        self
    }
}

impl<T: SegmentableItem, I: Injector<InputItem = Segmented<T>>> Injector
    for SegmentedInjector<T, I>
{
    type InputItem = (Option<crate::nucleo::GroupLabel>, T);
    type Inner = I;
    type Context = SplitterFn<T>;

    fn new(injector: Self::Inner, data: Self::Context) -> Self {
        Self {
            injector,
            splitter: data,
            single_column: false,
        }
    }

    fn wrap(
        &self,
        (group, item): Self::InputItem,
    ) -> Result<<Self::Inner as Injector>::InputItem, WorkerError> {
        let ranges = if self.single_column
            && let Some(len) = item.byte_len()
        {
            ColumnRanges::single(0, len as u32)
        } else {
            (self.splitter)(&item)
        };
        Ok(Segmented::new(item, ranges, group))
    }

    fn inner(&self) -> &Self::Inner {
        &self.injector
    }

    fn extend(
        &self,
        items: impl ExactSizeIterator<Item = Self::InputItem>,
    ) -> Result<(), WorkerError> {
        if self.single_column {
            let splitter = &self.splitter;
            let wrapped = items.map(|(group, item)| {
                let ranges = match item.byte_len() {
                    Some(len) => ColumnRanges::single(0, len as u32),
                    None => (splitter)(&item),
                };
                Segmented::new(item, ranges, group)
            });
            return self.injector.extend(wrapped);
        }
        let splitter = &self.splitter;
        let wrapped = items.map(|(group, item)| {
            let ranges = (splitter)(&item);
            Segmented::new(item, ranges, group)
        });
        self.injector.extend(wrapped)
    }
}

mod ansi {
    use std::{borrow::Cow, ops::Range, sync::Arc};

    use crate::{
        nucleo::Text,
        utils::text::{scrub_text_styles, slice_ratatui_text},
    };
    use ansi_to_tui::IntoText;

    pub type PreprocessOptions = (bool, bool);

    pub use super::*;
    pub use crate::utils::Either;

    #[derive(Clone)]
    pub struct AnsiInjector<I> {
        pub injector: I,
        pub parse: bool,
        pub trim: bool,
    }

    impl<
        I: Injector<
            InputItem = (
                Option<crate::nucleo::GroupLabel>,
                Either<Arc<str>, Box<Text<'static>>>,
            ),
        >,
    > Injector for AnsiInjector<I>
    {
        type InputItem = (Option<crate::nucleo::GroupLabel>, Arc<str>);
        type Inner = I;
        type Context = PreprocessOptions;

        fn new(injector: Self::Inner, (parse, trim): Self::Context) -> Self {
            Self {
                injector,
                parse,
                trim,
            }
        }

        fn wrap(
            &self,
            (group, item): Self::InputItem,
        ) -> Result<<Self::Inner as Injector>::InputItem, WorkerError> {
            Ok((group, preprocess(item, self.parse, self.trim)))
        }

        fn inner(&self) -> &Self::Inner {
            &self.injector
        }

        fn extend(
            &self,
            items: impl ExactSizeIterator<Item = Self::InputItem>,
        ) -> Result<(), WorkerError> {
            let parse = self.parse;
            let trim = self.trim;

            if !parse && !trim {
                self.injector
                    .extend(items.map(|(group, item)| (group, Either::Left(item))))
            } else if items.len() >= 256 {
                use rayon::prelude::*;
                let items_vec: Vec<_> = items.collect();
                let wrapped: Vec<<Self::Inner as Injector>::InputItem> = items_vec
                    .into_par_iter()
                    .map(|(group, item)| (group, preprocess(item, parse, trim)))
                    .collect();
                self.injector.extend(wrapped.into_iter())
            } else {
                self.injector
                    .extend(items.map(|(group, item)| (group, preprocess(item, parse, trim))))
            }
        }
    }

    /// Trims and/or parses ANSI styles. Plain text stays the same `Arc<str>` (shared with the
    /// matcher); trimming only reallocates when there is whitespace to remove.
    fn preprocess(
        mut item: Arc<str>,
        parse: bool,
        trim: bool,
    ) -> Either<Arc<str>, Box<Text<'static>>> {
        if trim {
            let trimmed = item.trim();
            if trimmed.len() != item.len() {
                item = trimmed.into();
            }
        }
        if !parse {
            Either::Left(item)
        } else {
            let mut parsed = item
                .as_bytes()
                .into_text()
                .unwrap_or_else(|_| Text::from(item.to_string()));
            scrub_text_styles(&mut parsed);
            Either::Right(Box::new(parsed))
        }
    }

    impl SegmentableItem for Either<Arc<str>, Box<Text<'static>>> {
        fn slice(&self, range: Range<usize>) -> Text<'_> {
            match self {
                Either::Left(s) => ratatui::text::Text::from(&s[range]),
                Either::Right(text) => slice_ratatui_text(text, range),
            }
        }
        #[inline]
        fn slice_str(&self, range: Range<usize>) -> Cow<'_, str> {
            match self {
                Either::Left(s) => {
                    if range.start == 0 && range.end == s.len() {
                        Cow::Borrowed(s)
                    } else {
                        (&s[range]).into()
                    }
                }
                Either::Right(text) => text.to_string()[range].to_string().into(),
            }
        }
        #[inline]
        fn shared(&self) -> Option<&Arc<str>> {
            match self {
                Either::Left(s) => Some(s),
                Either::Right(_) => None,
            }
        }
        #[inline]
        fn byte_len(&self) -> Option<usize> {
            Some(Either::byte_len(self))
        }
    }
}
pub use ansi::*;

// ----------- CLONE ----------------------------
impl<T> Clone for WorkerInjector<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            columns: Arc::clone(&self.columns),
            version: self.version,
            picker_version: Arc::clone(&self.picker_version),
        }
    }
}

impl<T: SegmentableItem, I: Injector<InputItem = Segmented<T>> + Clone> Clone
    for SegmentedInjector<T, I>
{
    fn clone(&self) -> Self {
        Self {
            injector: self.injector.clone(),
            splitter: Arc::clone(&self.splitter),
            single_column: self.single_column,
        }
    }
}
