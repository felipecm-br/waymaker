//! Helpers shared by this crate's unit tests.

use std::path::Path;
use std::sync::Once;

const DB_PREFIX: &str = "wm_test_frecency_";

/// Points the frecency store at a temporary database for the rest of this test
/// process. File-manager actions record their destination directory, so without
/// this every test that pastes writes its temp directories into the user's real
/// `~/.local/state/waymaker/frecency.redb`. Call it first in any test that
/// dispatches an action which opens [`waymaker::frecency::FrecencyStore`].
pub fn isolate_frecency() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let tmp = std::env::temp_dir();
        remove_stale_dbs(&tmp);
        let db = tmp.join(format!("{DB_PREFIX}{}.redb", std::process::id()));
        let _ = std::fs::remove_file(&db);
        // SAFETY: set once and never removed. Tests read the environment only
        // through `std::env`, which takes the same lock as `set_var`.
        unsafe { std::env::set_var("WM_FRECENCY_DB", &db) };
    });
}

/// The database outlives the test process, which cannot run cleanup at exit, so
/// each run deletes the ones left by earlier runs that are no longer alive.
fn remove_stale_dbs(tmp: &Path) {
    let Ok(entries) = std::fs::read_dir(tmp) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name
            .to_str()
            .and_then(|n| n.strip_prefix(DB_PREFIX))
            .and_then(|n| n.strip_suffix(".redb"))
        else {
            continue;
        };
        if !process_alive(pid) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(target_os = "linux")]
fn process_alive(pid: &str) -> bool {
    Path::new("/proc").join(pid).exists()
}

/// Without `/proc`, keep every file rather than risk deleting a live run's.
#[cfg(not(target_os = "linux"))]
fn process_alive(_pid: &str) -> bool {
    true
}
