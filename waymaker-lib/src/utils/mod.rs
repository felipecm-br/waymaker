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
