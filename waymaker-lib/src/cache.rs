//! Directory listing cache: the built-in walker's last listing of a directory, so the next run
//! there starts from it. Each listing is one file under the cache directory, named by a hash of
//! the directory's path and replaced atomically (temp file + rename). Disk use is the live data;
//! the oldest listings are deleted while the total exceeds the byte budget.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

pub const ZERO_COPY_MAGIC: u32 = 0x4D4D5A43; // 'MMZC'

/// Default budget for all cached listings together, about 600,000 paths. A listing larger than
/// this is not cached; otherwise the oldest listings are deleted until the new one fits.
pub const DIR_CACHE_MAX_BYTES: u64 = 32 * 1024 * 1024;

/// Extension of a cached listing file.
const LISTING_EXT: &str = "wmc";

/// Cache record for a directory listing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DirCacheRecord {
    pub root: String,
    pub timestamp: u64, // Unix timestamp in seconds
    #[serde(default)]
    pub mtime_nanos: u64, // Root directory mtime in nanoseconds for mtime validation
    pub items: Vec<String>,
}

impl DirCacheRecord {
    pub fn new(root: String, items: Vec<String>) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mtime_nanos = fs::metadata(&root)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        Self {
            root,
            timestamp,
            mtime_nanos,
            items,
        }
    }

    pub fn age_secs(&self) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(self.timestamp)
    }

    /// Encodes the record into a packed, zero-copy binary layout for sub-millisecond retrieval.
    pub fn to_zero_copy_bytes(&self) -> Vec<u8> {
        let root_bytes = self.root.as_bytes();
        let root_len = root_bytes.len() as u32;

        let mut capacity = 4 + 8 + 8 + 4 + root_bytes.len();
        for item in &self.items {
            capacity += item.len() + 1;
        }

        let mut buf = Vec::with_capacity(capacity);
        buf.extend_from_slice(&ZERO_COPY_MAGIC.to_le_bytes());
        buf.extend_from_slice(&self.timestamp.to_le_bytes());
        buf.extend_from_slice(&self.mtime_nanos.to_le_bytes());
        buf.extend_from_slice(&root_len.to_le_bytes());
        buf.extend_from_slice(root_bytes);

        for (i, item) in self.items.iter().enumerate() {
            if i > 0 {
                buf.push(0);
            }
            buf.extend_from_slice(item.as_bytes());
        }
        buf
    }

    /// Decodes record from packed zero-copy binary buffer without intermediate AST allocations.
    pub fn from_zero_copy_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 24 {
            return None;
        }

        let magic = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
        if magic != ZERO_COPY_MAGIC {
            return None;
        }

        let timestamp = u64::from_le_bytes(bytes[4..12].try_into().ok()?);
        let mtime_nanos = u64::from_le_bytes(bytes[12..20].try_into().ok()?);
        let root_len = u32::from_le_bytes(bytes[20..24].try_into().ok()?) as usize;

        if bytes.len() < 24 + root_len {
            return None;
        }

        let root = std::str::from_utf8(&bytes[24..24 + root_len])
            .ok()?
            .to_string();
        let payload = &bytes[24 + root_len..];

        let items = if payload.is_empty() {
            Vec::new()
        } else {
            let payload_str = std::str::from_utf8(payload).ok()?;
            payload_str.split('\0').map(|s| s.to_string()).collect()
        };

        Some(Self {
            root,
            timestamp,
            mtime_nanos,
            items,
        })
    }
}

/// Directory listing cache store. See the module docs.
#[derive(Debug, Clone)]
pub struct DirCacheStore {
    /// Where listings live; `None` when there is no state directory (caching is off).
    pub dir: Option<PathBuf>,
    max_bytes: u64,
}

impl DirCacheStore {
    /// `$WM_DIR_CACHE_DIR`, else `~/.local/state/waymaker/dir_cache/`.
    pub fn default_dir() -> Option<PathBuf> {
        if let Some(p) = std::env::var_os("WM_DIR_CACHE_DIR") {
            return Some(PathBuf::from(p));
        }
        let base = dirs::state_dir().or_else(dirs::data_local_dir)?;
        Some(base.join("waymaker").join("dir_cache"))
    }

    /// Opens the store at [`Self::default_dir`]. Deletes the redb file older versions kept the
    /// cache in (`dir_cache.redb` next to the directory), which nothing reads any more.
    pub fn open() -> Self {
        let Some(dir) = Self::default_dir() else {
            return Self::disabled();
        };
        if let Some(legacy) = dir.parent().map(|p| p.join("dir_cache.redb"))
            && legacy.is_file()
        {
            match fs::remove_file(&legacy) {
                Ok(()) => log::info!("Removed the old directory cache {legacy:?}"),
                Err(err) => log::debug!("Could not remove {legacy:?}: {err}"),
            }
        }
        Self::open_at(&dir).unwrap_or_else(|err| {
            log::warn!("Directory cache disabled: cannot create {dir:?}: {err}");
            Self::disabled()
        })
    }

    /// Opens (creating it if needed) the store in `dir`.
    pub fn open_at(dir: impl AsRef<Path>) -> anyhow::Result<Self> {
        let dir = dir.as_ref();
        fs::create_dir_all(dir)?;
        Ok(Self {
            dir: Some(dir.to_path_buf()),
            max_bytes: DIR_CACHE_MAX_BYTES,
        })
    }

    fn disabled() -> Self {
        Self {
            dir: None,
            max_bytes: DIR_CACHE_MAX_BYTES,
        }
    }

    /// Sets the byte budget for all cached listings together (default [`DIR_CACHE_MAX_BYTES`]).
    pub fn with_max_bytes(mut self, max_bytes: u64) -> Self {
        self.max_bytes = max_bytes;
        self
    }

    /// The normalized key and listing file for `raw_root`.
    fn locate(&self, raw_root: &str) -> Option<(String, PathBuf)> {
        let dir = self.dir.as_ref()?;
        let key = crate::frecency::normalize_path(raw_root);
        if key.is_empty() {
            return None;
        }
        let file = dir.join(format!("{:016x}.{LISTING_EXT}", fnv1a(key.as_bytes())));
        Some((key, file))
    }

    /// Retrieves the cached listing of a directory.
    pub fn get(&self, raw_root: &str) -> Option<DirCacheRecord> {
        let (key, file) = self.locate(raw_root)?;
        let rec = DirCacheRecord::from_zero_copy_bytes(&fs::read(file).ok()?)?;
        // A different root here is a hash collision: treat it as a miss.
        (rec.root == key).then_some(rec)
    }

    /// Retrieves cached directory listing record only if root directory mtime matches,
    /// invalidating automatically if files/folders were created, deleted, or renamed externally.
    pub fn get_valid(&self, raw_root: &str) -> Option<DirCacheRecord> {
        let rec = self.get(raw_root)?;
        let current_mtime_nanos = fs::metadata(raw_root)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);

        if current_mtime_nanos != 0
            && rec.mtime_nanos != 0
            && current_mtime_nanos != rec.mtime_nanos
        {
            log::debug!(
                "DirCache invalidated for {raw_root}: directory mtime changed ({current_mtime_nanos} != {})",
                rec.mtime_nanos
            );
            return None;
        }
        Some(rec)
    }

    /// Stores the listing of a directory, then deletes the oldest listings while the cache is
    /// over budget. A listing larger than the whole budget is not stored, and the directory's
    /// previous listing is deleted since it no longer matches.
    pub fn put(&self, raw_root: &str, items: Vec<String>) -> anyhow::Result<()> {
        let Some((key, file)) = self.locate(raw_root) else {
            return Ok(());
        };
        let bytes = DirCacheRecord::new(key.clone(), items).to_zero_copy_bytes();
        if bytes.len() as u64 > self.max_bytes {
            log::debug!(
                "DirCache skips {key}: {} bytes over the {} byte budget",
                bytes.len(),
                self.max_bytes
            );
            remove_if_exists(&file)?;
            return Ok(());
        }
        // A unique temp name per write, so concurrent writers never share one; the rename
        // replaces the listing atomically, and readers see the old or the new one whole.
        static WRITE_SEQ: AtomicU64 = AtomicU64::new(0);
        let tmp = file.with_extension(format!(
            "tmp.{}.{}",
            std::process::id(),
            WRITE_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&tmp, &bytes)?;
        if let Err(err) = fs::rename(&tmp, &file) {
            let _ = fs::remove_file(&tmp);
            return Err(err.into());
        }
        self.evict_to_budget(Some(&file));
        Ok(())
    }

    /// Deletes the cached listing of a directory. Returns whether there was one.
    pub fn remove(&self, raw_root: &str) -> anyhow::Result<bool> {
        let Some((_, file)) = self.locate(raw_root) else {
            return Ok(false);
        };
        remove_if_exists(&file)
    }

    /// The listing files with their size and modification time.
    fn listings(&self) -> Vec<(PathBuf, u64, SystemTime)> {
        let Some(Ok(entries)) = self.dir.as_ref().map(fs::read_dir) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == LISTING_EXT))
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                Some((e.path(), meta.len(), meta.modified().unwrap_or(UNIX_EPOCH)))
            })
            .collect()
    }

    /// Deletes listings until the total fits in the budget: any larger than the whole budget
    /// first, then the oldest. `keep` (the listing just written) is never deleted. Returns how
    /// many were deleted.
    fn evict_to_budget(&self, keep: Option<&Path>) -> usize {
        let mut listings = self.listings();
        let mut total: u64 = listings.iter().map(|l| l.1).sum();
        if total <= self.max_bytes {
            return 0;
        }
        listings.retain(|l| Some(l.0.as_path()) != keep);
        listings.sort_by_key(|&(_, len, modified)| (len <= self.max_bytes, modified));
        let mut removed = 0;
        for (path, len, _) in listings {
            if total <= self.max_bytes {
                break;
            }
            // Another process may have deleted or replaced it meanwhile; either way it is gone.
            let _ = fs::remove_file(&path);
            total = total.saturating_sub(len);
            removed += 1;
        }
        removed
    }

    /// Deletes listings whose directory no longer exists, unreadable listings and temp files
    /// left by interrupted writes, then the oldest listings while the cache is over budget.
    /// Returns how many listings were deleted.
    pub fn clean_stale(&self) -> anyhow::Result<usize> {
        let Some(dir) = self.dir.as_ref() else {
            return Ok(0);
        };
        let mut removed = 0;
        for entry in fs::read_dir(dir)?.flatten() {
            let path = entry.path();
            let is_listing = path.extension().is_some_and(|x| x == LISTING_EXT);
            let stale = if is_listing {
                read_root(&path).is_none_or(|root| !Path::new(&root).exists())
            } else {
                // `<hash>.tmp.<pid>.<n>`, older than any write still in progress.
                path.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.contains(".tmp."))
                    && entry
                        .metadata()
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|m| m.elapsed().ok())
                        .is_some_and(|age| age.as_secs() > 60)
            };
            if stale && fs::remove_file(&path).is_ok() && is_listing {
                removed += 1;
            }
        }
        Ok(removed + self.evict_to_budget(None))
    }
}

/// FNV-1a: a stable hash for listing file names (std's hasher may change between releases).
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Reads only the root path from a listing file's header.
fn read_root(path: &Path) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let mut head = [0u8; 24];
    file.read_exact(&mut head).ok()?;
    if head[0..4] != ZERO_COPY_MAGIC.to_le_bytes() {
        return None;
    }
    let root_len = u32::from_le_bytes(head[20..24].try_into().ok()?) as usize;
    let mut root = vec![0u8; root_len];
    file.read_exact(&mut root).ok()?;
    String::from_utf8(root).ok()
}

fn remove_if_exists(path: &Path) -> anyhow::Result<bool> {
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_copy_encoding_decoding() {
        let rec = DirCacheRecord {
            root: "/home/user/project".to_string(),
            timestamp: 1234567890,
            mtime_nanos: 987654321,
            items: vec![
                "src/lib.rs".to_string(),
                "Cargo.toml".to_string(),
                "docs/README.md".to_string(),
            ],
        };

        let encoded = rec.to_zero_copy_bytes();
        assert!(!encoded.is_empty());
        assert_eq!(&encoded[0..4], &ZERO_COPY_MAGIC.to_le_bytes());

        let decoded = DirCacheRecord::from_zero_copy_bytes(&encoded).unwrap();
        assert_eq!(decoded.root, rec.root);
        assert_eq!(decoded.timestamp, rec.timestamp);
        assert_eq!(decoded.mtime_nanos, rec.mtime_nanos);
        assert_eq!(decoded.items, rec.items);
    }

    #[test]
    fn test_zero_copy_edge_cases() {
        assert!(DirCacheRecord::from_zero_copy_bytes(&[]).is_none());
        assert!(DirCacheRecord::from_zero_copy_bytes(&[0u8; 23]).is_none());

        // Buffer with invalid magic
        let mut invalid_magic = vec![0u8; 32];
        invalid_magic[0..4].copy_from_slice(&0x12345678u32.to_le_bytes());
        assert!(DirCacheRecord::from_zero_copy_bytes(&invalid_magic).is_none());

        // Valid magic but root_len exceeds buffer
        let mut overflow_len = vec![0u8; 32];
        overflow_len[0..4].copy_from_slice(&ZERO_COPY_MAGIC.to_le_bytes());
        overflow_len[20..24].copy_from_slice(&1000u32.to_le_bytes());
        assert!(DirCacheRecord::from_zero_copy_bytes(&overflow_len).is_none());

        // Empty items payload
        let rec = DirCacheRecord {
            root: "test_root".to_string(),
            timestamp: 100,
            mtime_nanos: 200,
            items: vec![],
        };
        let bytes = rec.to_zero_copy_bytes();
        let decoded = DirCacheRecord::from_zero_copy_bytes(&bytes).unwrap();
        assert_eq!(decoded.root, "test_root");
        assert!(decoded.items.is_empty());
    }

    fn fresh_store(name: &str) -> anyhow::Result<(PathBuf, DirCacheStore)> {
        let temp_dir = std::env::temp_dir().join(name);
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir)?;
        let store = DirCacheStore::open_at(temp_dir.join("cache"))?;
        Ok((temp_dir, store))
    }

    fn listing(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("dir/file_{i:05}.rs")).collect()
    }

    /// Ages a stored listing by setting its file's modification time.
    fn age(store: &DirCacheStore, root: &str, secs_ago: u64) {
        let (_, file) = store.locate(root).unwrap();
        let when = SystemTime::now() - std::time::Duration::from_secs(secs_ago);
        fs::File::options()
            .write(true)
            .open(file)
            .unwrap()
            .set_modified(when)
            .unwrap();
    }

    #[test]
    fn test_dir_cache_store_put_get_remove() -> anyhow::Result<()> {
        let (temp_dir, store) = fresh_store("mm_test_dir_cache")?;
        let root_str = temp_dir.to_str().unwrap();

        assert!(store.get(root_str).is_none());
        assert!(!store.remove(root_str)?);

        let items = vec!["src/main.rs".to_string(), "README.md".to_string()];
        store.put(root_str, items.clone())?;
        assert_eq!(store.get(root_str).unwrap().items, items);

        // Replacing a listing leaves one file and no temp files.
        store.put(root_str, listing(3))?;
        assert_eq!(store.get(root_str).unwrap().items, listing(3));
        assert_eq!(fs::read_dir(temp_dir.join("cache"))?.count(), 1);

        assert!(store.remove(root_str)?);
        assert!(store.get(root_str).is_none());

        let _ = fs::remove_dir_all(&temp_dir);
        Ok(())
    }

    #[test]
    fn test_hash_collision_is_a_miss() -> anyhow::Result<()> {
        let (temp_dir, store) = fresh_store("mm_test_dir_cache_collision")?;
        store.put("/collide/a", listing(2))?;
        let (_, a) = store.locate("/collide/a").unwrap();
        let (_, b) = store.locate("/collide/b").unwrap();
        fs::copy(a, b)?;
        assert!(store.get("/collide/b").is_none());
        assert!(store.get("/collide/a").is_some());
        let _ = fs::remove_dir_all(&temp_dir);
        Ok(())
    }

    #[test]
    fn test_put_evicts_oldest_listings_over_budget() -> anyhow::Result<()> {
        let (temp_dir, store) = fresh_store("mm_test_dir_cache_budget")?;
        // 20 items of 17 bytes plus separators: about 400 bytes per listing.
        let store = store.with_max_bytes(1000);
        store.put("/budget/mid", listing(20))?;
        age(&store, "/budget/mid", 200);
        store.put("/budget/old", listing(20))?;
        age(&store, "/budget/old", 300);

        store.put("/budget/new", listing(20))?;

        assert!(store.get("/budget/old").is_none(), "oldest listing evicted");
        assert!(store.get("/budget/mid").is_some());
        assert_eq!(store.get("/budget/new").unwrap().items, listing(20));

        let _ = fs::remove_dir_all(&temp_dir);
        Ok(())
    }

    #[test]
    fn test_put_skips_listing_larger_than_budget() -> anyhow::Result<()> {
        let (temp_dir, store) = fresh_store("mm_test_dir_cache_oversize")?;
        let store = store.with_max_bytes(1000);
        store.put("/budget/other", listing(20))?;
        store.put("/budget/big", listing(5))?;
        assert!(store.get("/budget/big").is_some());

        store.put("/budget/big", listing(200))?;

        assert!(
            store.get("/budget/big").is_none(),
            "an outdated listing must not survive a skipped write"
        );
        assert!(store.get("/budget/other").is_some(), "nothing else evicted");

        let _ = fs::remove_dir_all(&temp_dir);
        Ok(())
    }

    #[test]
    fn test_eviction_drops_oversized_listings_before_old_ones() -> anyhow::Result<()> {
        let (temp_dir, store) = fresh_store("mm_test_dir_cache_oversized_first")?;
        store.put("/budget/old_small", listing(5))?;
        age(&store, "/budget/old_small", 300);
        store.put("/budget/huge", listing(200))?;
        // A smaller budget (an older version's cache, or a lowered limit) leaves `huge` over it.
        let store = store.with_max_bytes(1000);

        store.put("/budget/new", listing(5))?;

        assert!(store.get("/budget/huge").is_none());
        assert!(
            store.get("/budget/old_small").is_some(),
            "fits once huge is gone"
        );
        assert!(store.get("/budget/new").is_some());

        let _ = fs::remove_dir_all(&temp_dir);
        Ok(())
    }

    #[test]
    fn test_dir_cache_store_clean_stale_and_valid() -> anyhow::Result<()> {
        let (temp_dir, store) = fresh_store("mm_test_dir_cache_clean")?;
        let root = temp_dir.to_str().unwrap();

        store.put(root, vec!["file.txt".to_string()])?;
        assert!(store.get_valid(root).is_some());
        assert_eq!(store.clean_stale()?, 0, "existing directory kept");

        store.put("/non/existent/test_path_12345", vec!["x.txt".to_string()])?;
        assert!(store.get("/non/existent/test_path_12345").is_some());
        let cache = temp_dir.join("cache");
        fs::write(cache.join("garbage.wmc"), b"not a listing")?;
        let left_over = cache.join("0123456789abcdef.tmp.1.0");
        fs::write(&left_over, b"interrupted")?;
        let hour_ago = SystemTime::now() - std::time::Duration::from_secs(3600);
        fs::File::options()
            .write(true)
            .open(&left_over)?
            .set_modified(hour_ago)?;

        assert_eq!(
            store.clean_stale()?,
            2,
            "missing root and unreadable listing"
        );
        assert!(store.get("/non/existent/test_path_12345").is_none());
        assert!(!left_over.exists());
        assert!(store.get(root).is_some());

        let old_root = temp_dir.join("old");
        fs::create_dir_all(&old_root)?;
        let old_root = old_root.to_str().unwrap();
        store.put(old_root, listing(1))?;
        age(&store, old_root, 300);
        // Each listing is about 90 bytes, so a 120 byte budget keeps only the newer one.
        assert_eq!(store.clone().with_max_bytes(120).clean_stale()?, 1);
        assert!(store.get(old_root).is_none());
        assert!(store.get(root).is_some());

        let _ = fs::remove_dir_all(&temp_dir);
        Ok(())
    }

    #[test]
    fn test_open_removes_legacy_redb_file() -> anyhow::Result<()> {
        let _guard = crate::utils::lock_test_env();
        let temp_dir = std::env::temp_dir().join("mm_test_dir_cache_legacy");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir)?;
        let legacy = temp_dir.join("dir_cache.redb");
        fs::write(&legacy, b"old")?;
        // SAFETY: the test env lock serializes env mutation across tests.
        unsafe { std::env::set_var("WM_DIR_CACHE_DIR", temp_dir.join("dir_cache")) };
        let store = DirCacheStore::open();
        unsafe { std::env::remove_var("WM_DIR_CACHE_DIR") };

        assert!(!legacy.exists());
        assert_eq!(store.dir, Some(temp_dir.join("dir_cache")));
        let _ = fs::remove_dir_all(&temp_dir);
        Ok(())
    }
}
