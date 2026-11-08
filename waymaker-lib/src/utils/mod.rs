mod percentage;
pub mod tokio;
pub use percentage::Percentage;

pub mod serde;
pub mod text;

mod types;
pub use types::*;

pub mod markdown;
pub mod mermaid;
pub mod string;
pub mod tree;

/// The most cells [`cells`] reports: wider than any terminal, and small enough that the
/// widths of 31 columns still add up without overflowing `u16`.
pub const MAX_CELLS: u16 = 2_048;

/// Converts a width or count to terminal cells, saturating at [`MAX_CELLS`]. A plain
/// `as u16` wraps: a 65,546-cell line measured 10 and was laid out as a sliver.
#[inline]
pub fn cells(n: usize) -> u16 {
    n.min(MAX_CELLS as usize) as u16
}

/// Serializes tests that set or remove process environment variables. Every
/// such test in the crate must hold it, since the environment is shared across
/// test threads. A tokio mutex so async tests can hold it across `.await`; it
/// also cannot be poisoned, so one failing test does not fail the rest.
#[cfg(test)]
static TEST_ENV_LOCK: ::tokio::sync::Mutex<()> = ::tokio::sync::Mutex::const_new(());

/// [`TEST_ENV_LOCK`] for synchronous tests.
#[cfg(test)]
pub(crate) fn lock_test_env() -> ::tokio::sync::MutexGuard<'static, ()> {
    TEST_ENV_LOCK.blocking_lock()
}

/// [`TEST_ENV_LOCK`] for async tests.
#[cfg(test)]
pub(crate) async fn lock_test_env_async() -> ::tokio::sync::MutexGuard<'static, ()> {
    TEST_ENV_LOCK.lock().await
}
