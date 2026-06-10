//! File-based debug logging (enabled with --log-file)

use std::fs::OpenOptions;
use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use tracing_subscriber::EnvFilter;

/// Initialize tracing to append to the given file.
///
/// The level filter is read from the KEIFU_LOG environment variable
/// (RUST_LOG syntax) and defaults to "debug".
pub fn init(path: &Path) -> Result<()> {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("Failed to open log file: {}", path.display()))?;

    let filter = EnvFilter::try_from_env("KEIFU_LOG").unwrap_or_else(|_| EnvFilter::new("debug"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(Mutex::new(file))
        .with_ansi(false)
        .init();

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "keifu started");
    Ok(())
}
