//! Query history (fzf's `--history`): one query per line in a plain text file, oldest first.
//!
//! [`QueryHistory::prev`] and [`QueryHistory::next`] browse it from the filter; the query being
//! typed is kept as a draft and comes back after the newest entry. Accepting records the query.

use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct QueryHistory {
    path: Option<PathBuf>,
    max: usize,
    entries: Vec<String>,
    /// The entry shown while browsing.
    pos: Option<usize>,
    /// The query that was in the filter when browsing started.
    draft: String,
}

impl QueryHistory {
    /// Loads the history at `path`, keeping at most `max` entries. No path disables history.
    pub fn load(path: Option<PathBuf>, max: usize) -> Self {
        let entries = path.as_deref().map(read_entries).unwrap_or_default();
        let mut history = Self {
            path,
            max,
            entries,
            ..Self::default()
        };
        history.trim();
        history
    }

    pub fn is_enabled(&self) -> bool {
        self.path.is_some()
    }

    /// Whether there is anything to recall.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The file `query.history` names: `~/` is the home directory, and empty or `"auto"` (which
    /// only `wm` resolves) means no history.
    pub fn path_from_config(setting: &str) -> Option<PathBuf> {
        match setting {
            "" | "auto" => None,
            s => Some(match s.strip_prefix("~/") {
                Some(rest) => dirs::home_dir()?.join(rest),
                None => PathBuf::from(s),
            }),
        }
    }

    /// Whether the filter still shows the entry being browsed: editing the query, or recalling
    /// nothing yet, means the arrow keys move the cursor again.
    pub fn is_browsing(&self, query: &str) -> bool {
        self.pos
            .and_then(|p| self.entries.get(p))
            .is_some_and(|entry| entry == query)
    }

    /// The entry before the one shown, or the newest one when not browsing. `query` is the
    /// filter's text, kept as the draft when browsing starts. `None` at the oldest entry.
    pub fn prev(&mut self, query: &str) -> Option<&str> {
        let pos = match self.pos {
            Some(p) if self.is_browsing(query) => p.checked_sub(1)?,
            _ => {
                self.draft = query.to_string();
                self.entries.len().checked_sub(1)?
            }
        };
        self.pos = Some(pos);
        Some(&self.entries[pos])
    }

    /// The entry after the one shown; past the newest, the draft, which ends browsing.
    /// `None` when not browsing.
    pub fn next(&mut self, query: &str) -> Option<String> {
        if !self.is_browsing(query) {
            self.pos = None;
            return None;
        }
        let p = self.pos?;
        if p + 1 < self.entries.len() {
            self.pos = Some(p + 1);
            Some(self.entries[p + 1].clone())
        } else {
            self.pos = None;
            Some(std::mem::take(&mut self.draft))
        }
    }

    /// Appends `query` and saves. Empty queries and a repeat of the newest entry are skipped.
    /// The file is read again first, so entries written by another `wm` meanwhile are kept.
    pub fn record(&mut self, query: &str) -> std::io::Result<()> {
        let Some(path) = self.path.clone() else {
            return Ok(());
        };
        if query.is_empty() || query.contains('\n') {
            return Ok(());
        }
        self.entries = read_entries(&path);
        if self.entries.last().is_some_and(|last| last == query) {
            return Ok(());
        }
        self.entries.push(query.to_string());
        self.trim();
        self.pos = None;
        write_entries(&path, &self.entries)
    }

    fn trim(&mut self) {
        let excess = self.entries.len().saturating_sub(self.max);
        self.entries.drain(..excess);
    }
}

fn read_entries(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .map(|s| {
            s.lines()
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// Writes through a temporary file and a rename, so a reader never sees half a file.
fn write_entries(path: &Path, entries: &[String]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    let mut text = entries.join("\n");
    text.push('\n');
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("wm_history_{}_{name}", std::process::id()))
    }

    fn with_entries(name: &str, entries: &[&str]) -> QueryHistory {
        let path = temp_path(name);
        let _ = std::fs::remove_file(&path);
        let mut h = QueryHistory::load(Some(path), 100);
        for e in entries {
            h.record(e).unwrap();
        }
        h
    }

    #[test]
    fn browsing_walks_back_and_returns_the_draft() {
        let mut h = with_entries("walk", &["one", "two", "three"]);
        assert_eq!(h.prev("dra"), Some("three"));
        assert_eq!(h.prev("three"), Some("two"));
        assert_eq!(h.prev("two"), Some("one"));
        assert_eq!(h.prev("one"), None, "stops at the oldest");
        assert_eq!(h.next("one").as_deref(), Some("two"));
        assert_eq!(h.next("two").as_deref(), Some("three"));
        assert_eq!(
            h.next("three").as_deref(),
            Some("dra"),
            "past the newest: the draft"
        );
        assert!(!h.is_browsing("dra"));
        assert_eq!(h.next("dra"), None);
        let _ = std::fs::remove_file(temp_path("walk"));
    }

    #[test]
    fn editing_the_recalled_query_ends_browsing() {
        let mut h = with_entries("edit", &["one", "two"]);
        assert_eq!(h.prev(""), Some("two"));
        assert!(h.is_browsing("two"));
        assert!(!h.is_browsing("twox"));
        assert_eq!(h.next("twox"), None);
        // Browsing again starts from the newest entry, with the edited text as the draft.
        assert_eq!(h.prev("twox"), Some("two"));
        assert_eq!(h.next("two").as_deref(), Some("twox"));
        let _ = std::fs::remove_file(temp_path("edit"));
    }

    #[test]
    fn record_skips_empty_and_repeated_queries_and_trims() {
        let path = temp_path("trim");
        let _ = std::fs::remove_file(&path);
        let mut h = QueryHistory::load(Some(path.clone()), 3);
        for q in ["a", "", "b", "b", "c", "d"] {
            h.record(q).unwrap();
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "b\nc\nd\n");
        let reloaded = QueryHistory::load(Some(path.clone()), 3);
        assert_eq!(reloaded.entries, ["b", "c", "d"]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn record_keeps_entries_another_instance_wrote() {
        let path = temp_path("merge");
        let _ = std::fs::remove_file(&path);
        let mut first = QueryHistory::load(Some(path.clone()), 10);
        let mut second = QueryHistory::load(Some(path.clone()), 10);
        first.record("from first").unwrap();
        second.record("from second").unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "from first\nfrom second\n"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn up_after_editing_a_recalled_query_starts_again_from_the_newest() {
        let mut h = with_entries("reedit", &["one", "two"]);
        assert_eq!(h.prev(""), Some("two"));
        // Edited, then up again: the edit is the new draft and browsing restarts.
        assert_eq!(h.prev("twoX"), Some("two"));
        assert_eq!(h.next("two").as_deref(), Some("twoX"));
        let _ = std::fs::remove_file(temp_path("reedit"));
    }

    #[test]
    fn record_skips_a_query_spanning_lines() {
        let mut h = with_entries("multiline", &["a\nb", "ok"]);
        assert_eq!(h.prev(""), Some("ok"));
        assert_eq!(h.prev("ok"), None, "the two-line query was not saved");
        let _ = std::fs::remove_file(temp_path("multiline"));
    }

    #[test]
    fn is_empty_until_something_is_recorded() {
        let path = temp_path("empty");
        let _ = std::fs::remove_file(&path);
        let mut h = QueryHistory::load(Some(path.clone()), 10);
        assert!(h.is_enabled() && h.is_empty());
        h.record("q").unwrap();
        assert!(!h.is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn path_from_config_settings() {
        assert_eq!(QueryHistory::path_from_config(""), None);
        assert_eq!(
            QueryHistory::path_from_config("auto"),
            None,
            "only wm resolves auto"
        );
        assert_eq!(
            QueryHistory::path_from_config("/tmp/h"),
            Some(PathBuf::from("/tmp/h"))
        );
        let _env = crate::utils::lock_test_env();
        assert_eq!(
            QueryHistory::path_from_config("~/h"),
            Some(dirs::home_dir().unwrap().join("h"))
        );
    }

    #[test]
    fn without_a_path_nothing_is_recalled_or_written() {
        let mut h = QueryHistory::load(None, 10);
        assert!(!h.is_enabled());
        h.record("q").unwrap();
        assert_eq!(h.prev(""), None);
    }
}
