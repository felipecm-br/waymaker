#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MatchResult {
    pub score: u32,
    pub index: u32,
}

/// Abstract fuzzy matcher engine interface.
pub trait MatcherEngine: Send + Sync {
    /// Friendly name of the engine backend.
    fn name(&self) -> &'static str;

    /// Perform a search on a slice of items using a single thread.
    fn search(&mut self, query: &str, items: &[String]) -> Vec<MatchResult>;

    /// Perform a search in parallel across multiple threads.
    fn search_parallel(
        &mut self,
        query: &str,
        items: &[String],
        threads: usize,
    ) -> Vec<MatchResult>;

    /// Extract matching character offsets for highlight rendering in TUI.
    fn highlight_indices(&mut self, query: &str, haystack: &str) -> Vec<u32>;
}

/// Pure algorithm engine powered by Helix/Nucleo matcher.
pub struct NucleoEngine {
    matcher: nucleo::Matcher,
    case_matching: nucleo::pattern::CaseMatching,
    normalization: nucleo::pattern::Normalization,
}

impl Default for NucleoEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl NucleoEngine {
    pub fn new() -> Self {
        Self {
            matcher: nucleo::Matcher::new(nucleo::Config::DEFAULT),
            case_matching: nucleo::pattern::CaseMatching::Smart,
            normalization: nucleo::pattern::Normalization::Smart,
        }
    }
}

impl MatcherEngine for NucleoEngine {
    fn name(&self) -> &'static str {
        "nucleo"
    }

    fn search(&mut self, query: &str, items: &[String]) -> Vec<MatchResult> {
        if query.is_empty() {
            return items
                .iter()
                .enumerate()
                .map(|(i, _)| MatchResult {
                    score: 0,
                    index: i as u32,
                })
                .collect();
        }

        let pattern = nucleo::pattern::Pattern::new(
            query,
            self.case_matching,
            self.normalization,
            nucleo::pattern::AtomKind::Fuzzy,
        );

        let mut buf = Vec::new();
        let mut results = Vec::with_capacity(items.len());

        for (i, item) in items.iter().enumerate() {
            buf.clear();
            let utf32 = nucleo::Utf32Str::new(item, &mut buf);
            if let Some(score) = pattern.score(utf32, &mut self.matcher) {
                results.push(MatchResult {
                    score,
                    index: i as u32,
                });
            }
        }

        results.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.index.cmp(&b.index)));
        results
    }

    fn search_parallel(
        &mut self,
        query: &str,
        items: &[String],
        _threads: usize,
    ) -> Vec<MatchResult> {
        // Fallback to sequential for single Nucleo Matcher (or use nucleo worker in full TUI)
        self.search(query, items)
    }

    fn highlight_indices(&mut self, query: &str, haystack: &str) -> Vec<u32> {
        if query.is_empty() || haystack.is_empty() {
            return Vec::new();
        }

        let pattern = nucleo::pattern::Pattern::new(
            query,
            self.case_matching,
            self.normalization,
            nucleo::pattern::AtomKind::Fuzzy,
        );

        let mut buf = Vec::new();
        let utf32 = nucleo::Utf32Str::new(haystack, &mut buf);
        let mut indices = Vec::new();
        pattern.indices(utf32, &mut self.matcher, &mut indices);
        indices.sort_unstable();
        indices.dedup();
        indices
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nucleo_engine_basic() {
        let mut engine = NucleoEngine::new();
        let items = vec![
            "src/main.rs".to_string(),
            "src/lib.rs".to_string(),
            "README.md".to_string(),
        ];
        let res = engine.search("lib", &items);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].index, 1);
    }

    #[test]
    fn test_nucleo_engine_edge_cases() {
        let mut engine = NucleoEngine::default();
        assert_eq!(engine.name(), "nucleo");

        let items = vec!["alpha".into(), "beta".into(), "gamma".into()];
        let empty_search = engine.search("", &items);
        assert_eq!(empty_search.len(), 3);
        assert_eq!(empty_search[0].score, 0);

        let parallel_res = engine.search_parallel("bet", &items, 4);
        assert_eq!(parallel_res.len(), 1);
        assert_eq!(parallel_res[0].index, 1);

        assert!(engine.highlight_indices("", "sample").is_empty());
        assert!(engine.highlight_indices("test", "").is_empty());
        assert_eq!(engine.highlight_indices("bar", "bar.rs"), vec![0, 1, 2]);
    }
}
