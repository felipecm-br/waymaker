use std::borrow::Cow;

use super::{
    Worker,
    sort::{cmp_ascii_case_insensitive, compute_item_score, get_item_tier_and_clean_path},
};
use crate::SSS;

/// Cached result set for the Frizbee matcher engine.
#[derive(Default, Debug, Clone)]
pub struct FrizbeeWorkerCache {
    pub query: String,
    pub snapshot_item_count: u32,
    pub mode_index: usize,
    pub sort_order: Option<crate::action::SortOrder>,
    pub ranked_indices: Vec<(u32, u8)>,
}

impl<T: SSS> Worker<T> {
    /// Ensure the Frizbee search cache is fresh for the current query and snapshot.
    /// Returns true if the cache was updated, false if it was already up to date.
    #[cfg(feature = "frizbee")]
    pub fn ensure_frizbee_cache(&self) -> bool {
        let snapshot = self.nucleo.snapshot();
        let item_count = snapshot.item_count();
        let query_str = self.query.primary_column_query().unwrap_or_default();

        let mut cache = self.frizbee_cache.lock().unwrap();
        if cache.snapshot_item_count == item_count
            && cache.query == query_str
            && cache.mode_index == self.mode_index
            && cache.sort_order == self.sort_order
        {
            return false;
        }

        if item_count == 0 {
            cache.query = query_str.to_string();
            cache.snapshot_item_count = 0;
            cache.mode_index = self.mode_index;
            cache.sort_order = self.sort_order;
            cache.ranked_indices.clear();
            return true;
        }

        let col0 = &self.columns[0];
        let effective_dir_first = self.dir_first && self.mode_index == 0;
        let effective_sort_order = if self.mode_index == 0 {
            self.sort_order
        } else {
            None
        };
        let is_query_empty = query_str.is_empty();
        let query_len = query_str.len();

        let snapshot_ref = if self.frecency {
            self.frecency_snapshot.as_ref()
        } else {
            None
        };
        let penalty = if is_query_empty || self.mode_index != 0 {
            0
        } else {
            self.depth_penalty
        };

        if is_query_empty {
            return self.fill_empty_query_frizbee_cache(
                snapshot,
                item_count,
                query_str,
                col0,
                effective_dir_first,
                effective_sort_order,
                &mut cache,
            );
        }

        let mut strings: Vec<String> = Vec::with_capacity(item_count as usize);
        for idx in 0..item_count {
            if let Some(item) = snapshot.get_item(idx) {
                strings.push(col0.raw(item.data).into_owned());
            } else {
                strings.push(String::new());
            }
        }

        let frizbee_config = frizbee::Config {
            max_typos: if self.typo_tolerance {
                Some(1)
            } else {
                Some(0)
            },
            ..frizbee::Config::default()
        };
        let mut matcher = frizbee::Matcher::new(query_str, &frizbee_config);
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        let matches = matcher.match_list_parallel(&strings, threads);

        let total = matches.len() as u32;
        struct ScoredMatch {
            snapshot_idx: u32,
            tier: u8,
            score: u64,
            raw_path: String,
        }

        let mut scored: Vec<ScoredMatch> = matches
            .into_iter()
            .enumerate()
            .map(|(rank, m)| {
                let raw_path = std::mem::take(&mut strings[m.index as usize]);
                let score = compute_item_score(
                    total,
                    rank,
                    &raw_path,
                    false,
                    query_len,
                    snapshot_ref,
                    self.frecency_weight,
                    self.location_bias,
                    penalty,
                );
                let (tier, _) = get_item_tier_and_clean_path(&raw_path, effective_dir_first);
                ScoredMatch {
                    snapshot_idx: m.index,
                    tier,
                    score,
                    raw_path,
                }
            })
            .collect();

        scored.sort_unstable_by(|a, b| {
            if let Some(sort_order) = effective_sort_order {
                use crate::action::SortOrder;
                if effective_dir_first && a.tier != b.tier {
                    return a.tier.cmp(&b.tier);
                }
                match sort_order {
                    SortOrder::Alphabetical => {
                        return cmp_ascii_case_insensitive(&a.raw_path, &b.raw_path)
                            .then_with(|| a.raw_path.cmp(&b.raw_path));
                    }
                    SortOrder::AlphabeticalReverse => {
                        return cmp_ascii_case_insensitive(&b.raw_path, &a.raw_path)
                            .then_with(|| b.raw_path.cmp(&a.raw_path));
                    }
                    SortOrder::Natural => {
                        return crate::utils::string::natural_cmp(&a.raw_path, &b.raw_path)
                            .then_with(|| a.raw_path.cmp(&b.raw_path));
                    }
                    SortOrder::NaturalReverse => {
                        return crate::utils::string::natural_cmp(&b.raw_path, &a.raw_path)
                            .then_with(|| b.raw_path.cmp(&a.raw_path));
                    }
                    _ => {}
                }
            }
            if a.tier != b.tier {
                return a.tier.cmp(&b.tier);
            }
            b.score
                .cmp(&a.score)
                .then_with(|| cmp_ascii_case_insensitive(&a.raw_path, &b.raw_path))
        });

        cache.query = query_str.to_string();
        cache.snapshot_item_count = item_count;
        cache.mode_index = self.mode_index;
        cache.sort_order = self.sort_order;
        cache.ranked_indices = scored
            .into_iter()
            .map(|s| (s.snapshot_idx, s.tier))
            .collect();
        true
    }

    #[cfg(feature = "frizbee")]
    #[allow(clippy::too_many_arguments)]
    fn fill_empty_query_frizbee_cache(
        &self,
        snapshot: &nucleo::Snapshot<T>,
        item_count: u32,
        query_str: &str,
        col0: &super::Column<T>,
        effective_dir_first: bool,
        effective_sort_order: Option<crate::action::SortOrder>,
        cache: &mut FrizbeeWorkerCache,
    ) -> bool {
        let mut items: Vec<(u32, u8, Cow<'_, str>)> = (0..item_count)
            .filter_map(|idx| {
                snapshot.get_item(idx).map(|item| {
                    let raw = col0.raw(item.data);
                    let (tier, _) = get_item_tier_and_clean_path(raw.as_ref(), effective_dir_first);
                    (idx, tier, raw)
                })
            })
            .collect();

        if let Some(sort_order) = effective_sort_order {
            use crate::action::SortOrder;
            items.sort_unstable_by(|a, b| {
                if effective_dir_first && a.1 != b.1 {
                    return a.1.cmp(&b.1);
                }
                match sort_order {
                    SortOrder::Alphabetical => {
                        cmp_ascii_case_insensitive(a.2.as_ref(), b.2.as_ref())
                            .then_with(|| a.2.cmp(&b.2))
                    }
                    SortOrder::AlphabeticalReverse => {
                        cmp_ascii_case_insensitive(b.2.as_ref(), a.2.as_ref())
                            .then_with(|| b.2.cmp(&a.2))
                    }
                    SortOrder::Natural => {
                        crate::utils::string::natural_cmp(a.2.as_ref(), b.2.as_ref())
                            .then_with(|| a.2.cmp(&b.2))
                    }
                    SortOrder::NaturalReverse => {
                        crate::utils::string::natural_cmp(b.2.as_ref(), a.2.as_ref())
                            .then_with(|| b.2.cmp(&a.2))
                    }
                    _ => a.0.cmp(&b.0),
                }
            });
        } else if effective_dir_first {
            items.sort_unstable_by(|a, b| {
                if a.1 != b.1 {
                    return a.1.cmp(&b.1);
                }
                if a.1 < 2 {
                    let cmp = cmp_ascii_case_insensitive(a.2.as_ref(), b.2.as_ref());
                    if cmp != std::cmp::Ordering::Equal {
                        return cmp;
                    }
                }
                a.0.cmp(&b.0)
            });
        }

        cache.query = query_str.to_string();
        cache.snapshot_item_count = item_count;
        cache.mode_index = self.mode_index;
        cache.sort_order = self.sort_order;
        cache.ranked_indices = items
            .into_iter()
            .map(|(idx, tier, _)| (idx, tier))
            .collect();
        return true;
    }

    /// Retrieve all currently matched items using the frizbee engine for scoring,
    /// with the same decoration logic (depth_penalty, frecency, dir_first) as nucleo path.
    #[cfg(feature = "frizbee")]
    pub fn get_all_sorted_frizbee<'a>(&'a self, _query: &str) -> Vec<&'a T> {
        self.ensure_frizbee_cache();
        let snapshot = self.nucleo.snapshot();
        let cache = self.frizbee_cache.lock().unwrap();
        cache
            .ranked_indices
            .iter()
            .filter_map(|&(idx, _)| snapshot.get_item(idx).map(|item| item.data))
            .collect()
    }

    /// Get frizbee highlight indices for a single item's column text.
    #[cfg(feature = "frizbee")]
    pub fn frizbee_highlight_indices(&self, query: &str, haystack: &str) -> Vec<u32> {
        compute_frizbee_highlights(query, haystack, self.typo_tolerance)
    }
}

/// Standalone helper to compute frizbee matching character highlight indices.
#[cfg(feature = "frizbee")]
pub(super) fn compute_frizbee_highlights(
    query: &str,
    haystack: &str,
    typo_tolerance: bool,
) -> Vec<u32> {
    if query.is_empty() || haystack.is_empty() {
        return Vec::new();
    }
    let frizbee_config = frizbee::Config {
        max_typos: if typo_tolerance { Some(1) } else { Some(0) },
        ..frizbee::Config::default()
    };
    let mut matcher = frizbee::Matcher::new(query, &frizbee_config);
    if let Some(m) = matcher.match_one_indices(haystack, 0) {
        let mut indices = m.indices;
        indices.reverse(); // frizbee returns in reverse order
        indices.sort_unstable();
        indices.dedup();
        indices
    } else {
        Vec::new()
    }
}
