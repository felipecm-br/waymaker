// Original code from https://github.com/helix-editor/helix (MPL 2.0)
// Modified by Squirreljetpack, 2025

mod cell;
mod frizbee;
mod sort;
#[cfg(test)]
mod tests;

pub use frizbee::FrizbeeWorkerCache;

use super::{Style, Text};
use bitflags::bitflags;
use std::{
    borrow::Cow,
    sync::{
        Arc,
        atomic::{self, AtomicBool, AtomicU32, Ordering},
    },
};
use unicode_width::UnicodeWidthStr;

use super::{injector::WorkerInjector, query::PickerQuery};
use crate::{
    SSS,
    config::AutoscrollSettings,
    nucleo::Render,
    utils::text::{truncation_indicator, wrap_text},
};
#[cfg(test)]
use cell::render_cell;
use cell::render_cell_inner;
#[cfg(feature = "frizbee")]
use frizbee::compute_frizbee_highlights;
use sort::get_item_tier_and_clean_path;

type ColumnFormatFn<T> = Box<dyn for<'a> Fn(&'a T) -> Text<'a> + Send + Sync>;
type ColumnRawFn<T> = Box<dyn for<'a> Fn(&'a T) -> Cow<'a, str> + Send + Sync>;
type ColumnSharedFn<T> = Box<dyn Fn(&T) -> Option<Arc<str>> + Send + Sync>;
pub struct Column<T> {
    pub name: Arc<str>,
    pub(super) format: ColumnFormatFn<T>,
    pub(super) raw: Option<ColumnRawFn<T>>,
    /// The column text as a shared string, when the item can lend one; see [`Column::haystack`].
    pub(super) shared: Option<ColumnSharedFn<T>>,
    /// Direct function pointer fast path for `shared` when no closure state is captured (e.g. col 0).
    pub(super) shared_fn: Option<fn(&T) -> Option<Arc<str>>>,
    /// Whether the column should be passed to nucleo for matching and filtering.
    pub(super) filter: bool,
}

impl<T> Column<T> {
    pub fn new_boxed(name: impl Into<Arc<str>>, format: ColumnFormatFn<T>) -> Self {
        Self {
            name: name.into(),
            format,
            filter: true,
            raw: None,
            shared: None,
            shared_fn: None,
        }
    }

    pub fn new<F>(name: impl Into<Arc<str>>, f: F) -> Self
    where
        F: for<'a> Fn(&'a T) -> Text<'a> + SSS,
    {
        Self {
            name: name.into(),
            format: Box::new(f),
            filter: true,
            raw: None,
            shared: None,
            shared_fn: None,
        }
    }

    pub fn with_raw<F>(mut self, f: F) -> Self
    where
        F: for<'a> Fn(&'a T) -> Cow<'a, str> + SSS,
    {
        self.raw = Some(Box::new(f));
        self
    }

    /// Lets the matcher share the column text with the item instead of copying it, whenever
    /// `f` returns `Some` (it must be the same text `raw` returns).
    pub fn with_shared<F>(mut self, f: F) -> Self
    where
        F: Fn(&T) -> Option<Arc<str>> + SSS,
    {
        self.shared = Some(Box::new(f));
        self
    }

    /// Non-capturing function pointer version of [`Column::with_shared`].
    pub fn with_shared_fn(mut self, f: fn(&T) -> Option<Arc<str>>) -> Self {
        self.shared_fn = Some(f);
        self
    }

    /// Disable filtering.
    pub fn without_filtering(mut self) -> Self {
        self.filter = false;
        self
    }

    pub fn format<'a>(&self, item: &'a T) -> Text<'a> {
        (self.format)(item)
    }

    // Note: the characters should match the output of [`Self::format`]
    pub fn raw<'a>(&self, item: &'a T) -> Cow<'a, str> {
        if let Some(r) = &self.raw {
            (r)(item)
        } else {
            Cow::Owned((self.format)(item).to_string())
        }
    }

    /// The text the matcher stores for this column: the item's own shared string when it has
    /// one (so the text is stored once), otherwise a copy of [`Column::raw`].
    #[inline]
    pub(super) fn haystack(&self, item: &T) -> nucleo::Utf32String {
        if let Some(shared_fn) = self.shared_fn
            && let Some(text) = shared_fn(item)
        {
            return text.into();
        }
        if let Some(shared) = &self.shared
            && let Some(text) = shared(item)
        {
            return text.into();
        }
        self.raw(item).into()
    }
}

/// Worker: can instantiate, push, and get results. A view into computation.
///
/// Additionally, the worker can affect the computation via find and restart.
pub struct Worker<T>
where
    T: SSS,
{
    /// The inner `Nucleo` fuzzy matcher.
    pub nucleo: nucleo::Nucleo<T>,
    /// The last pattern that was matched against.
    pub query: PickerQuery,
    /// A pre-allocated buffer used to collect match indices when fetching the results
    /// from the matcher. This avoids having to re-allocate on each pass.
    pub col_indices_buffer: Vec<u32>,
    pub columns: Arc<[Column<T>]>,
    pub sort_threshold: crate::config::SortThreshold,
    pub depth_penalty: u32,
    pub frecency: bool,
    pub frecency_weight: u32,
    pub location_bias: u32,
    pub frecency_half_life_days: u32,
    pub sort_cap: usize,
    pub frecency_snapshot: Option<crate::frecency::FrecencySnapshot>,
    pub typo_tolerance: bool,
    pub engine: crate::config::MatcherEngineType,
    pub frizbee_cache: std::sync::Mutex<FrizbeeWorkerCache>,
    pub dir_first: bool,
    pub sort_order: Option<crate::action::SortOrder>,
    pub mode_index: usize,
    pub matcher_dirty: Arc<AtomicBool>,
    notify_callback: Arc<arc_swap::ArcSwapOption<NotifyFn>>,

    // Background tasks which push to the injector check their version matches this or exit
    pub(super) version: Arc<AtomicU32>,
    // pub settings: WorkerSettings,
    pub group_header: Option<Box<dyn for<'a> Fn(&'a T) -> Option<Arc<str>> + Send + Sync>>,
    column_options: Vec<ColumnOptions>,
}

struct NotifyFn(Box<dyn Fn() + Send + Sync>);

// #[derive(Debug, Default)]
// pub struct WorkerSettings {
//     pub stable: bool,
// }

bitflags! {
    #[derive(Default, Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    pub struct ColumnOptions: u8 {
        const Optional = 1 << 0;
        const OrUseDefault = 1 << 2;
    }
}

impl<T: SSS> Worker<T> {
    /// Column names must be distinct!
    pub fn new(columns: impl IntoIterator<Item = Column<T>>, default_column: usize) -> Self {
        let columns: Arc<[_]> = columns.into_iter().collect();
        let matcher_columns = columns.iter().filter(|col| col.filter).count() as u32;

        let matcher_dirty = Arc::new(AtomicBool::new(false));
        let notify_callback: Arc<arc_swap::ArcSwapOption<NotifyFn>> =
            Arc::new(arc_swap::ArcSwapOption::empty());

        let dirty_ref = matcher_dirty.clone();
        let cb_ref = notify_callback.clone();

        let inner = nucleo::Nucleo::new(
            nucleo::Config::DEFAULT,
            Arc::new(move || {
                dirty_ref.store(true, Ordering::Release);
                if let Some(cb) = cb_ref.load().as_ref() {
                    (cb.0)();
                }
            }),
            None,
            matcher_columns,
        );

        Self {
            nucleo: inner,
            matcher_dirty,
            notify_callback,
            col_indices_buffer: Vec::with_capacity(128),
            query: PickerQuery::new(columns.iter().map(|col| &col.name).cloned(), default_column),
            column_options: vec![ColumnOptions::default(); columns.len()],
            group_header: None,
            columns,
            sort_threshold: crate::config::SortThreshold::NEVER,
            depth_penalty: 0,
            frecency: false,
            frecency_weight: 1,
            location_bias: 30,
            frecency_half_life_days: 7,
            sort_cap: 1000,
            frecency_snapshot: None,
            typo_tolerance: false,
            engine: crate::config::MatcherEngineType::default(),
            frizbee_cache: std::sync::Mutex::new(FrizbeeWorkerCache::default()),
            dir_first: false,
            sort_order: None,
            mode_index: 0,
            version: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn set_mode_index(&mut self, mode_index: usize) {
        self.mode_index = mode_index;
    }

    pub fn set_sort_order(&mut self, order: Option<crate::action::SortOrder>) {
        self.sort_order = order;
    }

    pub fn get_sort_order(&self) -> Option<crate::action::SortOrder> {
        self.sort_order
    }

    pub fn set_notify<F>(&self, f: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.notify_callback
            .store(Some(Arc::new(NotifyFn(Box::new(f)))));
    }

    pub fn set_column_options(&mut self, index: usize, options: ColumnOptions) {
        if options.contains(ColumnOptions::Optional) {
            self.nucleo
                .pattern
                .configure_column(index, nucleo::pattern::Variant::Optional)
        }

        self.column_options[index] = options
    }

    pub fn reverse_items(&mut self, reverse_items: bool) {
        self.nucleo.reverse_items(reverse_items);
    }

    pub fn injector(&self) -> WorkerInjector<T> {
        WorkerInjector {
            inner: self.nucleo.injector(),
            columns: self.columns.clone(),
            version: self.version.load(atomic::Ordering::Relaxed),
            picker_version: self.version.clone(),
        }
    }

    pub fn find(&mut self, line: &str) {
        // With no query every item scores the same, and nucleo would break the tie by length,
        // listing the shortest first; like fzf, an empty query keeps the input order instead.
        // A query sorts by the configured threshold as before.
        if line.trim().is_empty() {
            self.nucleo.set_stability(u32::MAX);
        } else {
            self.nucleo
                .set_stability(self.sort_threshold.get_effective_threshold(line));
        }
        let old_query = self.query.parse(line);
        if self.query == old_query {
            return;
        }
        #[cfg(feature = "frizbee")]
        if self.engine == crate::config::MatcherEngineType::Frizbee {
            self.matcher_dirty.store(true, Ordering::Release);
            if let Some(cb) = self.notify_callback.load().as_ref() {
                (cb.0)();
            }
            return;
        }
        for (i, column) in self
            .columns
            .iter()
            .filter(|column| column.filter)
            .enumerate()
        {
            let pattern = self
                .query
                .get(&column.name)
                .map(|s| &**s)
                .unwrap_or_else(|| {
                    self.column_options[i]
                        .contains(ColumnOptions::OrUseDefault)
                        .then(|| self.query.primary_column_query())
                        .flatten()
                        .unwrap_or_default()
                });

            let old_pattern = old_query
                .get(&column.name)
                .map(|s| &**s)
                .unwrap_or_else(|| {
                    self.column_options[i]
                        .contains(ColumnOptions::OrUseDefault)
                        .then(|| {
                            let name = self.query.primary_column_name()?;
                            old_query.get(name).map(|s| &**s)
                        })
                        .flatten()
                        .unwrap_or_default()
                });

            // Fastlane: most columns will remain unchanged after each edit.
            if pattern == old_pattern {
                continue;
            }
            let is_append = pattern.starts_with(old_pattern);

            self.nucleo.pattern.reparse(
                i,
                pattern,
                nucleo::pattern::CaseMatching::Smart,
                nucleo::pattern::Normalization::Smart,
                is_append,
            );
        }
    }

    /// Force reparsing the pattern on all filter columns regardless of whether
    /// the query string changed. Essential when items in the worker are reloaded
    /// or replaced so nucleo immediately evaluates newly injected items.
    pub fn refind(&mut self) {
        #[cfg(feature = "frizbee")]
        if self.engine == crate::config::MatcherEngineType::Frizbee {
            self.matcher_dirty.store(true, Ordering::Release);
            if let Some(cb) = self.notify_callback.load().as_ref() {
                (cb.0)();
            }
            return;
        }
        for (i, column) in self
            .columns
            .iter()
            .filter(|column| column.filter)
            .enumerate()
        {
            let pattern = self
                .query
                .get(&column.name)
                .map(|s| &**s)
                .unwrap_or_else(|| {
                    self.column_options[i]
                        .contains(ColumnOptions::OrUseDefault)
                        .then(|| self.query.primary_column_query())
                        .flatten()
                        .unwrap_or_default()
                });

            self.nucleo.pattern.reparse(
                i,
                pattern,
                nucleo::pattern::CaseMatching::Smart,
                nucleo::pattern::Normalization::Smart,
                false,
            );
        }
    }
}

impl<T: SSS> Worker<T> {
    pub fn new_snapshot(nucleo: &mut nucleo::Nucleo<T>) -> (&nucleo::Snapshot<T>, Status) {
        let nucleo::Status { changed, running } = nucleo.tick(10);
        let snapshot = nucleo.snapshot();
        (
            snapshot,
            Status {
                item_count: snapshot.item_count(),
                matched_count: snapshot.matched_item_count(),
                running,
                changed,
            },
        )
    }

    pub fn raw_results(&self) -> impl ExactSizeIterator<Item = &T> + DoubleEndedIterator + '_ {
        let snapshot = self.nucleo.snapshot();
        snapshot.matched_items(..).map(|item| item.data)
    }

    /// matched item count, total item count
    pub fn counts(&self) -> (u32, u32) {
        #[cfg(feature = "frizbee")]
        if self.engine == crate::config::MatcherEngineType::Frizbee {
            self.ensure_frizbee_cache();
            let snapshot = self.nucleo.snapshot();
            let cache = self.frizbee_cache.lock().unwrap();
            return (cache.ranked_indices.len() as u32, snapshot.item_count());
        }
        let snapshot = self.nucleo.snapshot();
        (snapshot.matched_item_count(), snapshot.item_count())
    }

    pub fn set_stability(&mut self, threshold: crate::config::SortThreshold) {
        self.sort_threshold = threshold;
        let effective = threshold
            .get_effective_threshold(self.query.primary_column_query().unwrap_or_default());
        self.nucleo.set_stability(effective);
    }

    pub fn get_stability(&self) -> u32 {
        self.nucleo.get_stability()
    }

    pub fn restart(&mut self, clear_snapshot: bool) {
        #[cfg(feature = "frizbee")]
        if clear_snapshot {
            let mut cache = self.frizbee_cache.lock().unwrap();
            cache.snapshot_item_count = 0;
            cache.ranked_indices.clear();
        }
        self.nucleo.restart(clear_snapshot);
    }
}

#[derive(Debug, Default, Clone)]
pub struct Status {
    pub item_count: u32,
    pub matched_count: u32,
    pub running: bool,
    pub changed: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error("the matcher injector has been shut down")]
    InjectorShutdown,
    #[error("{0}")]
    Custom(&'static str),
}

#[derive(Clone, Debug, PartialEq)]
pub enum GroupHeader {
    Named(Arc<str>),
    TierSeparator,
}

/// A vec of ItemResult, each ItemResult being the Group Header (if any), Column Texts of the Item, and Item
pub type WorkerResults<'a, T> = Vec<(Option<GroupHeader>, Vec<Text<'a>>, &'a T)>;

impl<T: SSS> Worker<T> {
    /// Returns:
    /// 1. Table of (Row, item, height)
    /// 2. Final column widths
    /// 3. Status
    ///
    /// # Notes
    /// - Final column width is at least header width
    pub fn results(
        &mut self,
        start: u32,
        end: u32,
        width_limits: &[u16],
        wrap: bool,
        max_height: usize,
        highlight_style: Style,
        matcher: &mut nucleo::Matcher,
        autoscroll: AutoscrollSettings,
        hscroll_offset: i8,
        vscroll: (u8, bool),
        show_skipped: bool,
        freeze_snapshot: bool,
    ) -> (WorkerResults<'_, T>, Vec<u16>, Vec<u16>, Status) {
        let (nucleo_changed, nucleo_running) = if freeze_snapshot {
            (false, false)
        } else {
            let nucleo::Status { changed, running } = self.nucleo.tick(10);
            (changed, running)
        };

        #[cfg(feature = "frizbee")]
        let frizbee_cache_updated = if self.engine == crate::config::MatcherEngineType::Frizbee {
            self.ensure_frizbee_cache()
        } else {
            false
        };

        let snapshot = self.nucleo.snapshot();
        let status = Status {
            item_count: snapshot.item_count(),
            matched_count: if self.engine == crate::config::MatcherEngineType::Frizbee {
                #[cfg(feature = "frizbee")]
                {
                    self.frizbee_cache.lock().unwrap().ranked_indices.len() as u32
                }
                #[cfg(not(feature = "frizbee"))]
                {
                    snapshot.matched_item_count()
                }
            } else {
                snapshot.matched_item_count()
            },
            running: nucleo_running,
            changed: nucleo_changed || {
                #[cfg(feature = "frizbee")]
                {
                    frizbee_cache_updated
                }
                #[cfg(not(feature = "frizbee"))]
                {
                    false
                }
            },
        };

        let mut widths = vec![0u16; self.columns.len()];
        let mut raw_widths = vec![vec![]; self.columns.len()];
        let total_width_limit: u16 = width_limits.iter().sum();
        let last_nonzero_idx = width_limits.iter().rposition(|&w| w != 0);
        let query_str = self.query.primary_column_query().unwrap_or_default();
        let is_query_empty = query_str.is_empty();
        let query_len = query_str.len();
        let effective_dir_first = self.dir_first && self.mode_index == 0;
        let effective_sort_order = if self.mode_index == 0 {
            self.sort_order
        } else {
            None
        };
        let should_sort = effective_sort_order.is_some()
            || (!is_query_empty
                && ((self.frecency && self.frecency_snapshot.is_some())
                    || (self.depth_penalty > 0 && self.mode_index == 0)))
            || effective_dir_first;

        let (items_buf, initial_prev_tier) = self.window_items(
            snapshot,
            &status,
            start,
            end,
            should_sort,
            effective_sort_order,
            effective_dir_first,
            is_query_empty,
            query_len,
        );

        let (vscroll_offset, stacked) = vscroll;

        let mut table = Vec::new();
        let mut last_emitted_group: Option<Arc<str>> = None;
        let mut last_tier: Option<u8> = initial_prev_tier;
        let group_header = &self.group_header;

        for (item, item_tier) in &items_buf {
            let Some(row) = self.format_row_cells(
                item,
                vscroll_offset,
                stacked,
                show_skipped,
                max_height,
                width_limits,
                &mut raw_widths,
            ) else {
                continue;
            };

            let col_indices_buffer = &mut self.col_indices_buffer;
            let columns = &self.columns;
            #[cfg(feature = "frizbee")]
            let (query, engine, typo_tolerance) = (&self.query, self.engine, self.typo_tolerance);

            let row: Vec<Text> = row
                .into_iter()
                .enumerate()
                .zip(width_limits.iter().chain(std::iter::repeat(&u16::MAX)))
                .map(|((col_idx, cell), &width_limit)| {
                    let column = &columns[col_idx];

                    let effective_limit = if Some(col_idx) == last_nonzero_idx {
                        total_width_limit.saturating_sub(width_limits.iter().take(col_idx).sum())
                    } else {
                        width_limit
                    };

                    let (cell, computed_width) = if effective_limit == 0 {
                        (Default::default(), 0)
                    } else if column.filter {
                        #[cfg(feature = "frizbee")]
                        let custom_indices = if engine == crate::config::MatcherEngineType::Frizbee
                            && !query_str.is_empty()
                        {
                            let query_col =
                                query.get(&column.name).map(|s| &**s).unwrap_or(query_str);
                            Some(compute_frizbee_highlights(
                                query_col,
                                column.raw(item.data).as_ref(),
                                typo_tolerance,
                            ))
                        } else {
                            None
                        };
                        #[cfg(not(feature = "frizbee"))]
                        let custom_indices: Option<Vec<u32>> = None;

                        render_cell_inner(
                            cell,
                            col_idx,
                            snapshot,
                            item,
                            matcher,
                            highlight_style,
                            wrap,
                            effective_limit,
                            col_indices_buffer,
                            autoscroll.clone(),
                            hscroll_offset,
                            custom_indices.as_deref(),
                        )
                    } else if wrap {
                        let (cell, wrapped) = wrap_text(cell, effective_limit);

                        let width = if wrapped {
                            effective_limit as usize
                        } else {
                            cell.width()
                        };
                        (cell, width)
                    } else {
                        let width = cell.width();
                        (cell, width)
                    };

                    if col_idx < widths.len() {
                        widths[col_idx] = widths[col_idx].max(computed_width as u16)
                    }

                    cell
                })
                .collect();

            let mut header_to_emit = None;
            if let Some(f) = group_header
                && let Some(group) = f(item.data)
            {
                if Some(&group) != last_emitted_group.as_ref() {
                    header_to_emit = Some(GroupHeader::Named(group.clone()));
                    last_emitted_group = Some(group);
                }
            } else if effective_dir_first {
                if let Some(last) = last_tier
                    && *item_tier != last
                {
                    header_to_emit = Some(GroupHeader::TierSeparator);
                }
                last_tier = Some(*item_tier);
            }

            table.push((header_to_emit, row, item.data));
        }

        // Nonempty columns should have width at least their header
        for (w, c) in widths.iter_mut().zip(self.columns.iter()) {
            let name_width = c.name.width() as u16;
            if *w != 0 {
                *w = (*w).max(name_width);
            }
        }

        let medians = raw_widths
            .into_iter()
            .map(|mut v| {
                if v.is_empty() {
                    0
                } else {
                    v.sort_unstable();
                    v[v.len() / 2]
                }
            })
            .collect();

        (table, widths, medians, status)
    }

    #[allow(clippy::too_many_arguments)]
    fn window_items<'s>(
        &self,
        snapshot: &'s nucleo::Snapshot<T>,
        status: &Status,
        start: u32,
        end: u32,
        should_sort: bool,
        effective_sort_order: Option<crate::action::SortOrder>,
        effective_dir_first: bool,
        is_query_empty: bool,
        query_len: usize,
    ) -> (Vec<(nucleo::Item<'s, T>, u8)>, Option<u8>) {
        if self.engine == crate::config::MatcherEngineType::Frizbee {
            let cache = self.frizbee_cache.lock().unwrap();
            let total = cache.ranked_indices.len();
            let range_start = (start as usize).min(total);
            let range_end = (end as usize).min(total);
            let prev_tier = if range_start > 0 && range_start <= total {
                cache
                    .ranked_indices
                    .get(range_start - 1)
                    .map(|&(_, tier)| tier)
            } else {
                None
            };
            let items: Vec<_> = cache.ranked_indices[range_start..range_end]
                .iter()
                .filter_map(|&(idx, tier)| snapshot.get_item(idx).map(|item| (item, tier)))
                .collect();
            (items, prev_tier)
        } else if should_sort {
            self.sorted_window(
                snapshot,
                status,
                start,
                end,
                effective_sort_order,
                effective_dir_first,
                is_query_empty,
                query_len,
            )
        } else if is_query_empty && self.mode_index != 0 {
            let items: Vec<_> = (start.min(status.matched_count)..end.min(status.matched_count))
                .filter_map(|idx| snapshot.get_item(idx).map(|item| (item, 2u8)))
                .collect();
            (items, None)
        } else {
            let col0 = &self.columns[0];
            let items: Vec<_> = snapshot
                .matched_items(start.min(status.matched_count)..end.min(status.matched_count))
                .map(|item| {
                    let tier = if effective_dir_first {
                        let raw_path = col0.raw(item.data);
                        let (tier, _) = get_item_tier_and_clean_path(raw_path.as_ref(), true);
                        tier
                    } else {
                        2
                    };
                    (item, tier)
                })
                .collect();
            (items, None)
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn format_row_cells<'a>(
        &self,
        item: &nucleo::Item<'a, T>,
        vscroll_offset: u8,
        stacked: bool,
        show_skipped: bool,
        max_height: usize,
        width_limits: &[u16],
        raw_widths: &mut [Vec<u16>],
    ) -> Option<Vec<Text<'a>>> {
        let mut row = vec![];

        let mut to_skip = vscroll_offset as usize;
        let mut skip = !show_skipped;
        for (i, c) in self.columns.iter().enumerate() {
            let mut t = c.format(item.data);
            if stacked {
                if to_skip >= t.height() {
                    to_skip -= t.height();
                    t.lines.clear();
                } else {
                    skip = false;
                    t.lines.drain(..to_skip);
                    to_skip = 0;
                    if max_height > 0 && t.height() > max_height {
                        t.lines.truncate(max_height);
                        if let Some(last_line) = t.lines.last_mut() {
                            last_line.spans.push(truncation_indicator());
                        }
                    }
                }
            } else {
                if t.height() > to_skip {
                    skip = false;
                }
                t.lines.drain(..to_skip);
                if max_height > 0 && t.height() > max_height {
                    t.lines.truncate(max_height);
                    if let Some(last_line) = t.lines.last_mut() {
                        last_line.spans.push(truncation_indicator());
                    }
                }

                if width_limits.get(i).cloned() != Some(0) && !skip {
                    raw_widths[i].push(t.width() as u16);
                }
            }
            row.push(t);
        }
        if skip {
            return None;
        }
        Some(row)
    }

    pub fn exact_column_match(&mut self, column: &str) -> Option<&T> {
        let (i, col) = self
            .columns
            .iter()
            .enumerate()
            .find(|(_, c)| column == &*c.name)?;

        let query = self.query.get(column).map(|s| &**s).or_else(|| {
            self.column_options[i]
                .contains(ColumnOptions::OrUseDefault)
                .then(|| self.query.primary_column_query())
                .flatten()
        })?;

        let snapshot = self.nucleo.snapshot();
        snapshot.matched_items(..).find_map(|item| {
            let content = col.raw(item.data);
            if content.as_str() == query {
                Some(item.data)
            } else {
                None
            }
        })
    }

    pub fn format_with<'a>(&'a self, item: &'a T, col: &str) -> Option<Cow<'a, str>> {
        self.columns
            .iter()
            .find(|c| &*c.name == col)
            .map(|c| c.raw(item))
    }
}
