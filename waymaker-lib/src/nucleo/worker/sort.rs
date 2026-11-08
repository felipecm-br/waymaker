use std::borrow::Cow;

use super::{Status, Worker};
use crate::SSS;

pub(super) fn get_item_tier_and_clean_path(raw_str: &str, dir_first: bool) -> (u8, &str) {
    if !dir_first {
        return (2, raw_str);
    }

    let trimmed = raw_str.strip_prefix("./").unwrap_or(raw_str);
    if trimmed.starts_with('/') || trimmed.starts_with('\\') {
        let clean = if trimmed == "/" || trimmed == "\\" {
            trimmed
        } else {
            trimmed.trim_end_matches(['/', '\\'])
        };
        return (2, clean);
    }

    let clean = trimmed.trim_end_matches(['/', '\\']);
    let slash_count = clean.bytes().filter(|&b| b == b'/' || b == b'\\').count();

    if slash_count == 0 && !clean.is_empty() {
        let is_dir = raw_str.ends_with('/')
            || raw_str.ends_with('\\')
            || std::path::Path::new(clean).is_dir();
        if is_dir { (0, clean) } else { (1, clean) }
    } else {
        (2, clean)
    }
}

#[inline]
pub(super) fn cmp_ascii_case_insensitive(a: &str, b: &str) -> std::cmp::Ordering {
    let mut a_bytes = a.bytes().map(|b| b.to_ascii_lowercase());
    let mut b_bytes = b.bytes().map(|b| b.to_ascii_lowercase());
    loop {
        match (a_bytes.next(), b_bytes.next()) {
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
            }
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (None, None) => return std::cmp::Ordering::Equal,
        }
    }
}

#[inline]
pub(super) fn count_slashes(s: &str) -> u64 {
    let mut count = 0u64;
    for &b in s.as_bytes() {
        if b == b'/' || b == b'\\' {
            count += 1;
        }
    }
    count
}

pub(super) fn compute_item_score(
    total: u32,
    idx: usize,
    raw_path: &str,
    is_query_empty: bool,
    query_len: usize,
    snapshot_ref: Option<&crate::frecency::FrecencySnapshot>,
    frec_weight: u32,
    location_bias: u32,
    penalty: u32,
) -> u64 {
    let base_score = total.saturating_sub(idx as u32) as u64;
    let frecency_bonus = if !is_query_empty {
        snapshot_ref
            .map(|snap| snap.get_bonus_with_bias(raw_path, location_bias) * frec_weight)
            .unwrap_or(0) as u64
    } else {
        0
    };
    let effective_penalty = if is_query_empty || penalty == 0 {
        0
    } else if query_len <= 2 {
        penalty.saturating_mul(2)
    } else {
        penalty
    };
    let depth = if effective_penalty > 0 {
        count_slashes(raw_path)
    } else {
        0
    };

    base_score
        .saturating_add(frecency_bonus)
        .saturating_sub(depth * effective_penalty as u64)
}

pub(super) struct DecoratedItem<'a, T> {
    item: nucleo::Item<'a, T>,
    tier: u8,
    raw_path: Cow<'a, str>,
    clean_range: (usize, usize),
    score: u64,
    mtime: Option<std::time::SystemTime>,
    btime: Option<std::time::SystemTime>,
    size: Option<u64>,
    ext_range: Option<(usize, usize)>,
}

impl<'a, T> DecoratedItem<'a, T> {
    #[inline]
    fn clean(&self) -> &str {
        &self.raw_path[self.clean_range.0..self.clean_range.1]
    }

    #[inline]
    fn ext(&self) -> &str {
        if let Some((start, end)) = self.ext_range {
            &self.raw_path[start..end]
        } else {
            ""
        }
    }
}

fn cmp_decorated_items<T>(
    a: &DecoratedItem<'_, T>,
    b: &DecoratedItem<'_, T>,
    effective_sort_order: Option<crate::action::SortOrder>,
    effective_dir_first: bool,
    is_query_empty: bool,
) -> std::cmp::Ordering {
    if let Some(sort_order) = effective_sort_order {
        use crate::action::SortOrder;
        if effective_dir_first && a.tier != b.tier {
            return a.tier.cmp(&b.tier);
        }

        let ord = match sort_order {
            SortOrder::Alphabetical => cmp_ascii_case_insensitive(a.clean(), b.clean())
                .then_with(|| a.clean().cmp(b.clean())),
            SortOrder::AlphabeticalReverse => cmp_ascii_case_insensitive(b.clean(), a.clean())
                .then_with(|| b.clean().cmp(a.clean())),
            SortOrder::Natural => crate::utils::string::natural_cmp(a.clean(), b.clean())
                .then_with(|| a.clean().cmp(b.clean())),
            SortOrder::NaturalReverse => crate::utils::string::natural_cmp(b.clean(), a.clean())
                .then_with(|| b.clean().cmp(a.clean())),
            SortOrder::Modified => {
                let a_time = a.mtime.unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                let b_time = b.mtime.unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                a_time
                    .cmp(&b_time)
                    .then_with(|| crate::utils::string::natural_cmp(a.clean(), b.clean()))
            }
            SortOrder::ModifiedReverse => {
                let a_time = a.mtime.unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                let b_time = b.mtime.unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                b_time
                    .cmp(&a_time)
                    .then_with(|| crate::utils::string::natural_cmp(a.clean(), b.clean()))
            }
            SortOrder::Created => {
                let a_time = a.btime.unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                let b_time = b.btime.unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                a_time
                    .cmp(&b_time)
                    .then_with(|| crate::utils::string::natural_cmp(a.clean(), b.clean()))
            }
            SortOrder::CreatedReverse => {
                let a_time = a.btime.unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                let b_time = b.btime.unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                b_time
                    .cmp(&a_time)
                    .then_with(|| crate::utils::string::natural_cmp(a.clean(), b.clean()))
            }
            SortOrder::Size => {
                let a_size = a.size.unwrap_or(0);
                let b_size = b.size.unwrap_or(0);
                a_size
                    .cmp(&b_size)
                    .then_with(|| crate::utils::string::natural_cmp(a.clean(), b.clean()))
            }
            SortOrder::SizeReverse => {
                let a_size = a.size.unwrap_or(0);
                let b_size = b.size.unwrap_or(0);
                b_size
                    .cmp(&a_size)
                    .then_with(|| crate::utils::string::natural_cmp(a.clean(), b.clean()))
            }
            SortOrder::Extension => cmp_ascii_case_insensitive(a.ext(), b.ext())
                .then_with(|| crate::utils::string::natural_cmp(a.clean(), b.clean())),
            SortOrder::ExtensionReverse => cmp_ascii_case_insensitive(b.ext(), a.ext())
                .then_with(|| crate::utils::string::natural_cmp(a.clean(), b.clean())),
        };

        if ord != std::cmp::Ordering::Equal {
            return ord;
        }
        return b.score.cmp(&a.score);
    }

    if a.tier != b.tier {
        return a.tier.cmp(&b.tier);
    }

    if is_query_empty && a.tier < 2 {
        let cmp = cmp_ascii_case_insensitive(a.clean(), b.clean());
        if cmp != std::cmp::Ordering::Equal {
            return cmp;
        }
    }

    b.score
        .cmp(&a.score)
        .then_with(|| cmp_ascii_case_insensitive(a.clean(), b.clean()))
}

impl<T: SSS> Worker<T> {
    #[allow(clippy::too_many_arguments)]
    fn decorate_item<'a>(
        &self,
        col0: &super::Column<T>,
        idx: usize,
        item: nucleo::Item<'a, T>,
        total: u32,
        is_query_empty: bool,
        query_len: usize,
        snapshot_ref: Option<&crate::frecency::FrecencySnapshot>,
        frec_weight: u32,
        penalty: u32,
        effective_dir_first: bool,
        effective_sort_order: Option<crate::action::SortOrder>,
    ) -> DecoratedItem<'a, T> {
        let raw_path = col0.raw(item.data);
        let score = compute_item_score(
            total,
            idx,
            raw_path.as_ref(),
            is_query_empty,
            query_len,
            snapshot_ref,
            frec_weight,
            self.location_bias,
            penalty,
        );
        let (tier, clean) = get_item_tier_and_clean_path(raw_path.as_ref(), effective_dir_first);
        let clean_start = clean.as_ptr() as usize - raw_path.as_ref().as_ptr() as usize;
        let clean_range = (clean_start, clean_start + clean.len());
        let (mtime, btime, size, ext_range) = match effective_sort_order {
            Some(
                crate::action::SortOrder::Modified | crate::action::SortOrder::ModifiedReverse,
            ) => {
                let m = std::fs::metadata(clean)
                    .or_else(|_| std::fs::symlink_metadata(clean))
                    .and_then(|meta| meta.modified())
                    .ok();
                (m, None, None, None)
            }
            Some(crate::action::SortOrder::Created | crate::action::SortOrder::CreatedReverse) => {
                let meta = std::fs::metadata(clean)
                    .or_else(|_| std::fs::symlink_metadata(clean))
                    .ok();
                let b = meta
                    .as_ref()
                    .and_then(|m| m.created().ok())
                    .or_else(|| meta.as_ref().and_then(|m| m.modified().ok()));
                (None, b, None, None)
            }
            Some(crate::action::SortOrder::Size | crate::action::SortOrder::SizeReverse) => {
                let s = std::fs::metadata(clean)
                    .or_else(|_| std::fs::symlink_metadata(clean))
                    .map(|meta| meta.len())
                    .ok();
                (None, None, s, None)
            }
            Some(
                crate::action::SortOrder::Extension | crate::action::SortOrder::ExtensionReverse,
            ) => {
                let ext = std::path::Path::new(clean)
                    .extension()
                    .and_then(|e| e.to_str());
                let range = ext.map(|e| {
                    let start = e.as_ptr() as usize - raw_path.as_ref().as_ptr() as usize;
                    (start, start + e.len())
                });
                (None, None, None, range)
            }
            _ => (None, None, None, None),
        };
        DecoratedItem {
            item,
            tier,
            raw_path,
            clean_range,
            score,
            mtime,
            btime,
            size,
            ext_range,
        }
    }

    fn collect_sorted_decorated<'a>(
        &self,
        snapshot: &'a nucleo::Snapshot<T>,
        total: u32,
        effective_sort_order: Option<crate::action::SortOrder>,
        effective_dir_first: bool,
        is_query_empty: bool,
        query_len: usize,
    ) -> Vec<DecoratedItem<'a, T>> {
        let total_sort = if effective_sort_order.is_some() {
            total
        } else if self.sort_cap > 0 {
            total.min(self.sort_cap as u32)
        } else {
            total
        };
        let mut items: Vec<_> = snapshot.matched_items(0..total_sort).enumerate().collect();
        let penalty = if is_query_empty || self.mode_index != 0 {
            0
        } else {
            self.depth_penalty
        };
        let frec_weight = self.frecency_weight;
        let snapshot_ref = if self.frecency {
            self.frecency_snapshot.as_ref()
        } else {
            None
        };
        let col0 = &self.columns[0];
        let scan_end = total.min(total_sort + 5000);
        if scan_end > total_sort {
            for (idx, item) in snapshot.matched_items(total_sort..scan_end).enumerate() {
                let raw_path = col0.raw(item.data);
                let has_frecency =
                    snapshot_ref.is_some_and(|snap| snap.has_bonus_fast(raw_path.as_ref()));
                let is_direct = if effective_dir_first {
                    let (tier, _) = get_item_tier_and_clean_path(raw_path.as_ref(), true);
                    tier < 2
                } else {
                    false
                };
                if has_frecency || is_direct {
                    items.push((total_sort as usize + idx, item));
                }
            }
        }

        let mut decorated: Vec<DecoratedItem<'_, T>> = items
            .into_iter()
            .map(|(idx, item)| {
                self.decorate_item(
                    col0,
                    idx,
                    item,
                    total,
                    is_query_empty,
                    query_len,
                    snapshot_ref,
                    frec_weight,
                    penalty,
                    effective_dir_first,
                    effective_sort_order,
                )
            })
            .collect();

        decorated.sort_unstable_by(|a, b| {
            cmp_decorated_items(
                a,
                b,
                effective_sort_order,
                effective_dir_first,
                is_query_empty,
            )
        });

        decorated
    }

    pub(super) fn get_sorted_decorated<'a>(
        &'a self,
        snapshot: &'a nucleo::Snapshot<T>,
    ) -> Option<Vec<DecoratedItem<'a, T>>> {
        let total = snapshot.matched_item_count();
        if total == 0 {
            return None;
        }

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

        if !should_sort {
            return None;
        }

        Some(self.collect_sorted_decorated(
            snapshot,
            total,
            effective_sort_order,
            effective_dir_first,
            is_query_empty,
            query_len,
        ))
    }

    pub fn get_nth(&self, n: u32) -> Option<&T> {
        #[cfg(feature = "frizbee")]
        if self.engine == crate::config::MatcherEngineType::Frizbee {
            self.ensure_frizbee_cache();
            let snapshot = self.nucleo.snapshot();
            let cache = self.frizbee_cache.lock().unwrap();
            return cache
                .ranked_indices
                .get(n as usize)
                .and_then(|&(idx, _)| snapshot.get_item(idx).map(|item| item.data));
        }

        let snapshot = self.nucleo.snapshot();
        let total = snapshot.matched_item_count();
        if n >= total {
            return None;
        }

        let query_str = self.query.primary_column_query().unwrap_or_default();
        let is_query_empty = query_str.is_empty();

        if let Some(decorated) = self.get_sorted_decorated(snapshot) {
            decorated.get(n as usize).map(|d| d.item.data).or_else(|| {
                if is_query_empty && self.mode_index != 0 {
                    snapshot.get_item(n).map(|item| item.data)
                } else {
                    snapshot.get_matched_item(n).map(|item| item.data)
                }
            })
        } else if is_query_empty && self.mode_index != 0 {
            snapshot.get_item(n).map(|item| item.data)
        } else {
            snapshot.get_matched_item(n).map(|item| item.data)
        }
    }

    pub fn find_item_index<F>(&self, mut predicate: F) -> Option<usize>
    where
        F: FnMut(&T) -> bool,
    {
        #[cfg(feature = "frizbee")]
        if self.engine == crate::config::MatcherEngineType::Frizbee {
            self.ensure_frizbee_cache();
            let snapshot = self.nucleo.snapshot();
            let cache = self.frizbee_cache.lock().unwrap();
            for (pos, &(idx, _)) in cache.ranked_indices.iter().enumerate() {
                if let Some(item) = snapshot.get_item(idx)
                    && predicate(item.data)
                {
                    return Some(pos);
                }
            }
            return None;
        }

        let snapshot = self.nucleo.snapshot();
        let total = snapshot.matched_item_count();
        if total == 0 {
            return None;
        }

        let query_str = self.query.primary_column_query().unwrap_or_default();
        let is_query_empty = query_str.is_empty();

        if let Some(decorated) = self.get_sorted_decorated(snapshot) {
            for (idx, d) in decorated.iter().enumerate() {
                if predicate(d.item.data) {
                    return Some(idx);
                }
            }
            if is_query_empty && self.mode_index != 0 {
                let item_count = snapshot.item_count() as usize;
                for idx in decorated.len()..item_count {
                    if let Some(item) = snapshot.get_item(idx as u32)
                        && predicate(item.data)
                    {
                        return Some(idx);
                    }
                }
            } else {
                for idx in decorated.len()..total as usize {
                    if let Some(item) = snapshot.get_matched_item(idx as u32)
                        && predicate(item.data)
                    {
                        return Some(idx);
                    }
                }
            }
            None
        } else if is_query_empty && self.mode_index != 0 {
            let item_count = snapshot.item_count() as usize;
            for idx in 0..item_count {
                if let Some(item) = snapshot.get_item(idx as u32)
                    && predicate(item.data)
                {
                    return Some(idx);
                }
            }
            None
        } else {
            for idx in 0..total as usize {
                if let Some(item) = snapshot.get_matched_item(idx as u32)
                    && predicate(item.data)
                {
                    return Some(idx);
                }
            }
            None
        }
    }

    /// Retrieve all currently matched items in exact ranked (sorted) order.
    pub fn get_all_sorted(&self) -> Vec<&T> {
        #[cfg(feature = "frizbee")]
        if self.engine == crate::config::MatcherEngineType::Frizbee {
            self.ensure_frizbee_cache();
            let snapshot = self.nucleo.snapshot();
            let cache = self.frizbee_cache.lock().unwrap();
            return cache
                .ranked_indices
                .iter()
                .filter_map(|&(idx, _)| snapshot.get_item(idx).map(|item| item.data))
                .collect();
        }

        let snapshot = self.nucleo.snapshot();
        let total = snapshot.matched_item_count();
        if total == 0 {
            return Vec::new();
        }

        let query_str = self.query.primary_column_query().unwrap_or_default();
        let is_query_empty = query_str.is_empty();

        let mut res = Vec::with_capacity(total as usize);
        if let Some(decorated) = self.get_sorted_decorated(snapshot) {
            for d in decorated {
                res.push(d.item.data);
            }
            if is_query_empty && self.mode_index != 0 {
                let item_count = snapshot.item_count() as usize;
                for idx in res.len()..item_count {
                    if let Some(item) = snapshot.get_item(idx as u32) {
                        res.push(item.data);
                    }
                }
            } else {
                for idx in res.len()..total as usize {
                    if let Some(item) = snapshot.get_matched_item(idx as u32) {
                        res.push(item.data);
                    }
                }
            }
        } else if is_query_empty && self.mode_index != 0 {
            for idx in 0..snapshot.item_count() {
                if let Some(item) = snapshot.get_item(idx) {
                    res.push(item.data);
                }
            }
        } else {
            for idx in 0..total {
                if let Some(item) = snapshot.get_matched_item(idx) {
                    res.push(item.data);
                }
            }
        }
        res
    }

    /// The `start..end` window of the matches when they are re-sorted (sort order, frecency,
    /// depth penalty or dirs first): sorts up to `sort_cap` matches, then returns the window
    /// with each item's tier, and the tier of the item just before it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn sorted_window<'s>(
        &self,
        snapshot: &'s nucleo::Snapshot<T>,
        status: &Status,
        start: u32,
        end: u32,
        effective_sort_order: Option<crate::action::SortOrder>,
        effective_dir_first: bool,
        is_query_empty: bool,
        query_len: usize,
    ) -> (Vec<(nucleo::Item<'s, T>, u8)>, Option<u8>) {
        let total = status.matched_count;
        let col0 = &self.columns[0];
        let decorated = self.collect_sorted_decorated(
            snapshot,
            total,
            effective_sort_order,
            effective_dir_first,
            is_query_empty,
            query_len,
        );

        let range_start = start.min(total) as usize;
        let range_end = end.min(total) as usize;
        let prev_tier = if range_start > 0 && range_start <= decorated.len() {
            decorated.get(range_start - 1).map(|d| d.tier)
        } else {
            None
        };
        let items: Vec<_> = if range_start < decorated.len() {
            let take_count = range_end.saturating_sub(range_start);
            decorated
                .into_iter()
                .skip(range_start)
                .take(take_count)
                .map(|d| (d.item, d.tier))
                .collect()
        } else {
            snapshot
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
                .collect()
        };
        (items, prev_tier)
    }
}
