use crate::clap::{BINARY_SHORT, LIBRARY_FULL};
use cba::{
    bait::ResultExt,
    bog::{self, BogOkExt},
};
use std::{fs::OpenOptions, path::Path};

/// Size at which the log starts over.
const LOG_MAX_BYTES: u64 = 5 * 1024 * 1024;

pub fn init_logger([q, v]: [u8; 2], log_path: &Path) {
    bog::init_bogger(true, true);
    bog::init_filter((4 + v).saturating_sub(q));

    let rust_log = std::env::var("RUST_LOG").ok().map(|val| val.to_lowercase());

    let mut builder = env_logger::Builder::from_default_env();

    if rust_log.is_none() {
        #[cfg(debug_assertions)]
        {
            builder
                .filter(None, log::LevelFilter::Info)
                .filter(Some(LIBRARY_FULL), log::LevelFilter::Trace)
                .filter(Some("cba"), log::LevelFilter::Trace)
                .filter(Some(BINARY_SHORT), log::LevelFilter::Trace)
                .format_timestamp_micros();
        }
        #[cfg(not(debug_assertions))]
        {
            builder
                .format_module_path(false)
                .format_target(false)
                .format_timestamp(None);

            let level = cba::bother::level_filter::from_qv(q, v);

            builder
                .filter(Some(LIBRARY_FULL), level)
                .filter(Some("cba"), level)
                .filter(Some(BINARY_SHORT), level);
        }
    }

    log_path.parent().map(cba::bs::create_dir);

    // Append, so a `wm` started by this one (a preview, `wm list`, a reload command) adds to
    // the log instead of wiping it; start afresh only once the file passes LOG_MAX_BYTES.
    let fresh = std::fs::metadata(log_path).is_ok_and(|m| m.len() > LOG_MAX_BYTES);
    if let Some(log_file) = OpenOptions::new()
        .append(!fresh)
        .truncate(fresh)
        .write(true)
        .create(true)
        .open(log_path)
        .prefix(format!(
            "Failed to open log file @ {}.",
            log_path.to_string_lossy()
        ))
        ._wbog()
    {
        builder.target(env_logger::Target::Pipe(Box::new(log_file)));
    }

    builder.init();
}
